use crate::cpu::TrapFrame;

#[unsafe(no_mangle)]
unsafe extern "C" fn __machine_trap(
    epc: usize,
    tval: usize,
    cause: usize,
    hart: usize,
    _status: usize,
    _frame: &mut TrapFrame,
) -> usize {
    // All traps will be handled in machine mode, as that is easier than swapping virtual memory around.

    let is_async = {
        cause >> 63 & 1 == 1 // The last bit being 1 signifies an async trap.
    };

    let trap_cause = cause & 0xfff;
    let mut return_pc = epc;

    if is_async {
        // Async trap
        match trap_cause {
            3 => {
                // Machine Software Interrupt
                println!("Machine Software Interrupt CPU#{}", hart);
            }
            7 => unsafe {
                // Machine Timer Interrupt
                let mtimecmp = 0x0200_4000 as *mut u64;
                let mtime = 0x200_bff8 as *const u64;
                // QEMU frequency is 10 MHz, so this will set the next interrupt to one second in the future.
                mtimecmp.write_volatile(mtime.read_volatile() + 10_000_000);
            },
            11 => {
                // Machine External Interrupt (PLIC)
                println!("Machine External Interrupt CPU#{}", hart);
            }
            _ => {
                panic!("Unhandled async trap CPU#{} -> {}\n", hart, trap_cause);
            }
        }
    } else {
        // Sync trap
        match trap_cause {
            2 => {
                // Illegal instruction
                println!(
                    "Illegal instruction CPU#{} -> 0x{:08x}: 0x{:08x}\n",
                    hart, epc, tval
                );
            }
            8 => {
                // ECALL from User mode
                println!(
                    "ECALL from User mode CPU#{} -> 0x{:08x}: 0x{:08x}\n",
                    hart, epc, tval
                );
            }
            9 => {
                // ECALL from Supervisor mode
                println!(
                    "ECALL from Supervisor mode CPU#{} -> 0x{:08x}: 0x{:08x}\n",
                    hart, epc, tval
                );
            }
            11 => {
                // ECALL from Machine mode
                println!(
                    "ECALL from Machine mode CPU#{} -> 0x{:08x}: 0x{:08x}\n",
                    hart, epc, tval
                );
            }
            12 => {
                // Instruction page fault
                println!(
                    "Instruction page fault CPU#{} -> 0x{:08x}: 0x{:08x}\n",
                    hart, epc, tval
                );
                return_pc += 4;
            }
            13 => {
                // Load page fault
                println!(
                    "Load page fault CPU#{} -> 0x{:08x}: 0x{:08x}\n",
                    hart, epc, tval
                );
                return_pc += 4;
            }
            15 => {
                // Store/AMO page fault
                println!(
                    "Store/AMO page fault CPU#{} -> 0x{:08x}: 0x{:08x}\n",
                    hart, epc, tval
                );
            }
            _ => {
                panic!("Unhandled sync trap CPU#{} -> {}\n", hart, trap_cause);
            }
        }
    }
    return_pc
}
