# Disable generation of compressed instructions
.option norvc

.section .data
/*
    Define a `.text.init section`. The .text.init is put at the starting address
    so that the entry `_start` is put at the RISC-V adress 0x8000_0000.
*/
.section .text.init

.global _start
_start:
    /*
        Disable linker instruction relaxation for the `la` instruction below.
        This disallows the assembler from assuming that `gp` is already initialized.
        This causes the value stored in `gp` to be calculated from `pc`
    */
    .option push
    .option norelax
    la      gp, _global_pointer
    .option pop

    # SATP should be zero, but make sure
    csrw    satp, zero

    # Any hardware threads (hart) that are not bootstrapping neet to wait for an IPI
    csrr    t0, mhartid
    bnez    t0, 3f

    # The BSS section is expected to be zero, and we have to ensure that
    la      a0, _bss_start
    la      a1, _bss_end
    bgeu    a0, a1, 2f

1:
    # zero out range between _bss_start and _bss_end in doubleword chunks (8 bytes)
    sd      zero, (a0)
    addi    a0, a0, 8
    bltu    a0, a1, 1b
2:
    /*
        Control registers, set the stack, mstatus, mepc, and mtvec to return to the main function.
        Some is still commented out for later
    */
    la      sp, _stack_end

    /*
        Setting `mstatus` register:
        0b11 << 11  : Machine's previous protection mode is 3 (MPP=3)
        1 << 7      : Machine's previous interrupt-enable bit is 1 (MPIE=1)
        1 << 3      : Machine's interrupt-enable bit is 1 (MIE=1)
        Interrupts disabled for now as trapping leads to `wfi`
    */
    li      t0, (0b11 << 11)
    csrw    mstatus, t0

    # Do not allow interrupts while running `kinit()`
    csrw    mie, zero
    
    # Machine's exception program counter (MEPC) is set to `kinit()`
    la      t1, kinit
    csrw    mepc, t1

    # Set the return address to get us into supervisor mode
    la      ra, 2f

    # Usage of mret so that the mstatus register is properly updated (previous values -> current values e.g MPIE)
    mret
2:
    # CPU will return to here once `kinit` returns from Rust.


    # Set `mstatus` register
    # 0b01 << 11    : MPP=1 [Supervisor]
    # 1 << 7        : MPIE=1 [Enabled]
    # 1 << 5        : SPIE=1 [Enabled]
    li      t0, (0b01 << 11) | (1 << 7) | (1 << 5)
    csrw    mstatus, t0

    # Set machine mode trap vector
    la      t2, asm_trap_vector
    csrw    mtvec, t2

    la      t1, kmain
    csrw    mepc, t1

    # set `mie` register (machine interrupt enable)
    li      t2, 0x808 # MSIE, MTIE, MEIE // TODO: Reenable timer interrupt once done (0x888)
    csrw    mie, t2

    # sidestep PMP
    li      t3, 0
    csrw    pmpaddr0, t3
    li      t4, 7 | (1 << 3) # RWX | A=TOR
    csrw    pmpcfg0, t4


    mret
3:
/*
    Parked HARTs go here. They need to be set to only awaken if they receive a software interrupts,
    which is going to call the SIPI (Software Inter-Processor Interrupt).
    This may change, but is what is done for now.
*/

4:
    wfi
    j       4b
    