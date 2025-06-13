.global asm_trap_vector
asm_trap_vector:
    # This is invoked when the CPU is interrupted for any reason.
    li t6, 0xFFFF
    wfi
    j       asm_trap_vector