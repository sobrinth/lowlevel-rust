.global asm_trap_vector
asm_trap_vector:
    # This is invoked when the CPU is interrupted for any reason.
    mret