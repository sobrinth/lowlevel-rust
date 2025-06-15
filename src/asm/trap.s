.global asm_trap_vector
asm_trap_vector:
    # This is invoked when the CPU is interrupted for any reason.
    li t6, 0xFFFF
    wfi
    j       asm_trap_vector


.option norvc
.altmacro
.set NUM_GP_REGS, 32
.set REG_SIZE, 8

# Macros to load and store registers
.macro save_gp i, basereg=t6
    sd  x\i, ((\i)*REG_SIZE)(\basereg)
.endm

.macro load_gp i, basereg=t6
    ld x\i, ((\i)*REG_SIZE)(\basereg)
.endm

.macro save_fp i, basereg=t6
    fsd f\i, ((NUM_GP_REGS+(\i))*REG_SIZE)(\basereg)
.endm

.macro load_fp i, basereg=t6
    fld f\i, ((NUM_GP_REGS+(\i))*REG_SIZE)(\basereg)
.endm


/* New actual trap handling */
.section .text
.global _m_trap_dispatch
# This must be aligned by 4 since the last two bits of the mtvec register do not contribute to the address
.align 4
_m_trap_dispatch:
    # All registers are volatile here, we need to save them before doing anything.
    
    /*
    csrrw will atomically swap `t6` and `mscratch`

    In `cpu.rs` we have a structure of:
        32 gp registers     0   byte
        32 fp registers     256
        SATP register       512
        Trap stack          520
        CPU HARTID          528

    `t6` is used as the temporary register as it's the very bottom register (`x32`)
    */
    csrrw   t6, mscratch, t6

    .set    i, 1
    .rept   30
        save_gp %i # t6 is implicitly used as basereg for the macro. t6 contains trap frame
        .set    i, i+1
    .endr

    # Save the swapped out `t6` register
    mv      t5, t6
    csrr    t6, mscratch
    save_gp 31, t5 # At this point t5 contains the address of the trap frame, which is used in the macro

    # Restore kernel trap frame into mscratch
    csrw    mscratch, t5

    # Get ready to call into the rust side
    csrr    a0, mepc
    csrr    a1, mtval
    csrr    a2, mcause
    csrr    a3, mhartid
    csrr    a4, mstatus
    mv      a5, t5
    ld      sp, 520(a5)
    call    __machine_trap

    # Restore registers and return. Return value is in `a0`
    csrw    mepc, a0
    
    # Load trap frame back into `t6`
    csrr    t6, mscratch

    # Restore all gp registers
    .set    i, 1
    .rept   31
        load_gp %i
        .set    i, i+1
    .endr

    # the last loop automatically reloaded the correct value into `t6`

    mret

.global make_syscall
make_syscall:
    ecall
    ret
