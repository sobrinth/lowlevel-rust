use crate::cpu::TrapFrame;
use crate::{plic, uart};

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
                // Machine Software Interrupt — clear CLINT MSIP before returning
                // to prevent infinite re-entry (CLINT does not auto-clear on trap entry)
                let msip = (0x0200_0000usize + 4 * hart) as *mut u32;
                unsafe { msip.write_volatile(0); }
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
                // This is a bad idea to handle here in the trap context.
                if let Some(interrupt) = plic::next() {
                    match interrupt {
                        10 => handle_uart_input(),
                        _ => {
                            print!("Non-UART external interrupt: {}", interrupt);
                        }
                    }
                    plic::complete(interrupt);
                }
            }
            _ => {
                panic!("Unhandled async trap CPU#{} -> {}\n", hart, trap_cause);
            }
        }
    } else {
        // Sync trap
        match trap_cause {
            2 => {
                // Illegal instruction — no process to recover, halt
                panic!(
                    "Illegal instruction CPU#{} -> 0x{:08x}: 0x{:08x}\n",
                    hart, epc, tval
                );
            }
            8 => {
                // ECALL from User mode
                println!("ECALL from User mode CPU#{} -> 0x{:08x}\n", hart, epc);
                return_pc += 4;
            }
            9 => {
                // ECALL from Supervisor mode
                println!("ECALL from Supervisor mode CPU#{} -> 0x{:08x}\n", hart, epc);
                return_pc += 4;
            }
            11 => {
                // ECALL from Machine mode
                panic!("ECALL from Machine mode CPU#{} -> 0x{:08x}\n", hart, epc);
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
                return_pc += 4;
            }
            _ => {
                panic!("Unhandled sync trap CPU#{} -> {}\n", hart, trap_cause);
            }
        }
    }
    return_pc
}

fn handle_uart_input() {
    // UART
    // This really shouldn't happen here, but it's okay for now.
    let uart = uart::Uart::new(0x1000_0000);
    if let Some(c) = uart.get() {
        match c {
            8 => {
                // This is a backspace, so we essentially have to write a space and backup again:
                print!("{}{}{}", 8 as char, ' ', 8 as char);
            }
            10 | 13 => {
                // CR or LF
                println!();
            }
            0x1b => {
                // ANSI escape sequence (ESC[...). These are multibyte sequences, so we take a chance and read ourselves.
                if let Some(next_byte) = uart.get()
                    && next_byte == 0x5b
                {
                    // [
                    if let Some(b) = uart.get() {
                        match b as char {
                            'A' => {
                                println!("That's the up arrow");
                            }
                            'B' => {
                                println!("That's the down arrow");
                            }
                            'C' => {
                                println!("That's the right arrow");
                            }
                            'D' => {
                                println!("That's the left arrow");
                            }
                            _ => {
                                println!("Unhandled escape sequence");
                            }
                        }
                    }
                }
            }
            _ => {
                print!("{}", c as char);
            }
        }
    }
}
