#![no_std]
#![no_main]
#![feature(lang_items)]
#![allow(internal_features)]
extern crate alloc;

use crate::page::{PAGE_ORDER, PAGE_SIZE};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use core::arch::asm;

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

#[macro_export]
macro_rules! print {
    ($($args:tt)+) => ({
        use core::fmt::Write;
        let _ = write!($crate::uart::Uart::new(0x1000_0000), $($args)+);
    });
}

#[macro_export]
macro_rules! println {
    () => ({
        print!("\r\n")
    });
    ($fmt:expr) => ({
        print!(concat!($fmt, "\r\n"))
    });
    ($fmt:expr, $($args:tt)+) => ({
        print!(concat!($fmt, "\r\n"), $($args)+)
    });
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    print!("Aborting: ");
    if let Some(p) = info.location() {
        println!("line {}, file {}: {}", p.line(), p.file(), info.message());
    } else {
        println!("No information available.")
    }
    abort()
}

#[unsafe(no_mangle)]
extern "C" fn abort() -> ! {
    loop {
        unsafe { asm!("wfi") }
    }
}

// The following symbols come from asm/mem.s. We can use
// the symbols directly, but the address of the symbols
// themselves are their values, which can cause issues.
// Instead, I created doubleword values in mem.s in the .rodata and .data
// sections.
unsafe extern "C" {
    static TEXT_START: usize;
    static TEXT_END: usize;
    static DATA_START: usize;
    static DATA_END: usize;
    static RODATA_START: usize;
    static RODATA_END: usize;
    static BSS_START: usize;
    static BSS_END: usize;
    static KERNEL_STACK_START: usize;
    static KERNEL_STACK_END: usize;
    static HEAP_START: usize;
    static HEAP_SIZE: usize;
    static MEMORY_END: usize;
}

#[unsafe(no_mangle)]
extern "C" fn __init() {
    // Main should initialize all the things
    let uart = uart::Uart::new(0x1000_0000);
    uart.init();
    println!("\n~~~~~~~  __init() starting  ~~~~~~~~\n\n");
    page::init();
    kmem::init();

    // map heap allocations
    let root_ptr = kmem::get_page_table();
    let root_u = root_ptr as usize;
    let root = unsafe { root_ptr.as_mut().unwrap() };
    let kheap_head = kmem::get_head() as usize;
    let total_pages = kmem::get_num_allocations();

    unsafe {
        println!("TEXT:   0x{:x} -> 0x{:x}", TEXT_START, TEXT_END);
        println!("RODATA: 0x{:x} -> 0x{:x}", RODATA_START, RODATA_END);
        println!("DATA:   0x{:x} -> 0x{:x}", DATA_START, DATA_END);
        println!("BSS:    0x{:x} -> 0x{:x}", BSS_START, BSS_END);
        println!(
            "STACK:  0x{:x} -> 0x{:x}",
            KERNEL_STACK_START, KERNEL_STACK_END
        );
        println!("HEAP:   0x{:x} -> 0x{:x}", HEAP_START, MEMORY_END);
        println!(
            "KHEAP:  0x{:x} -> 0x{:x}",
            kheap_head,
            kheap_head + total_pages * PAGE_SIZE
        );
    }

    id_map_range(
        root,
        kheap_head,
        kheap_head + total_pages * PAGE_SIZE,
        page::EntryBits::ReadWrite.val(),
    );

    unsafe {
        // Map heap descriptors
        let num_pages = HEAP_SIZE / PAGE_SIZE;
        id_map_range(
            root,
            HEAP_START,
            HEAP_START + num_pages,
            page::EntryBits::ReadWrite.val(),
        );

        // Map executable section
        id_map_range(
            root,
            TEXT_START,
            TEXT_END,
            page::EntryBits::ReadExecute.val(),
        );

        // Map RODATA
        // We put the ROdata section into the text section, so they can potentially overlap, however,
        // we only care that it's read-only.
        id_map_range(
            root,
            RODATA_START,
            RODATA_END,
            page::EntryBits::ReadExecute.val(),
        );

        // Map DATA section
        id_map_range(root, DATA_START, DATA_END, page::EntryBits::ReadWrite.val());

        // Map BSS section
        id_map_range(root, BSS_START, BSS_END, page::EntryBits::ReadWrite.val());

        // Map kernel stack
        id_map_range(
            root,
            KERNEL_STACK_START,
            KERNEL_STACK_END,
            page::EntryBits::ReadWrite.val(),
        );
    }

    // UART
    page::map(
        root,
        0x1000_0000,
        0x1000_0000,
        page::EntryBits::ReadWrite.val(),
        0,
    );

    // CLINT
    // -> MSIP
    page::map(
        root,
        0x0200_0000,
        0x0200_0000,
        page::EntryBits::ReadWrite.val(),
        0,
    );

    // MTIMECMP
    page::map(
        root,
        0x0200_b000,
        0x0200_b000,
        page::EntryBits::ReadWrite.val(),
        0,
    );

    // MTIME
    page::map(
        root,
        0x0200_c000,
        0x0200_c000,
        page::EntryBits::ReadWrite.val(),
        0,
    );

    // PLIC
    page::map(
        root,
        0x0c00_0000,
        0x0c00_0000,
        page::EntryBits::ReadWrite.val(),
        0,
    );
    id_map_range(
        root,
        0x0c20_0000,
        0x0c20_0000,
        page::EntryBits::ReadWrite.val(),
    );

    let satp_value = cpu::build_satp(cpu::SatpMode::SV39, 0, root_u);
    unsafe {
        cpu::mscratch_write((&mut cpu::KERNEL_TRAP_FRAME[0] as *mut cpu::TrapFrame) as usize);
        cpu::KERNEL_TRAP_FRAME[0].satp = satp_value;

        // Move the stack pointer to the very bottom
        cpu::KERNEL_TRAP_FRAME[0].trap_stack = page::zalloc_page(1).add(PAGE_SIZE);

        id_map_range(
            root,
            cpu::KERNEL_TRAP_FRAME[0].trap_stack.sub(PAGE_SIZE) as usize,
            cpu::KERNEL_TRAP_FRAME[0].trap_stack as usize,
            page::EntryBits::ReadWrite.val(),
        );

        // The trap frame itself is stored in mscratch
        id_map_range(
            root,
            cpu::mscratch_read(),
            cpu::mscratch_read() + size_of::<cpu::TrapFrame>(),
            page::EntryBits::ReadWrite.val(),
        );

        page::print_page_allocations();
        let p = cpu::KERNEL_TRAP_FRAME[0].trap_stack as usize - 1;
        let m = page::virt_to_phys(root, p).unwrap_or(0);
        println!("Validate id_map for TRAP_FRAME: 0x{:x} -> 0x{:x}", p, m);
    }

    println!("Setting SATP to: 0x{:x}", satp_value);
    println!("MSCRATCH = 0x{:x}", cpu::mscratch_read());
    cpu::satp_write(satp_value);
    cpu::satp_fence_asid(0);
    println!("~~~~~~~  __init() finished  ~~~~~~~~\n\n")
}

#[unsafe(no_mangle)]
extern "C" fn __main() {
    println!("\n~~~~~~~  __main() starting  ~~~~~~~~\n");
    // `kmain()` starts in supervisor mode. So, we should have the trap vector setup and the MMU
    // turned on when we get here.

    // We initialized UART in machine mod under `kinit()`, so just grab a pointer
    let uart = uart::Uart::new(0x1000_0000);
    {
        println!("  ~~~~~  Start allocator testing  ~~~~~~  \n");
        // Try out the allocator
        let k = Box::<u32>::new(100);
        println!("Boxed value = {}", *k);

        let rainbow_vec = vec![
            240, 159, 143, 179, 239, 184, 143, 226, 128, 141, 240, 159, 140, 136,
        ];
        let rainbow = String::from_utf8(rainbow_vec).unwrap();
        println!("String = {}", rainbow);
        kmem::print_table();
    }
    println!("\n\nEverything should now be free:");
    kmem::print_table();

    println!("\n  ~~~~~  End allocator testing  ~~~~~~  \n");

    unsafe {
        println!("  ~~~~~  Start trap handling testing  ~~~~~~  \n");
        make_syscall();
        // Force a page fault
        let v = core::ptr::dangling_mut::<u64>();
        *v = 0;
        println!("\n  ~~~~~  End trap handling testing  ~~~~~~  \n");
    }

    loop {
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
}

unsafe extern "C" {
    fn make_syscall();
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __init_hart(hartid: usize) {
    // All non-0 harts init here
    unsafe {
        cpu::mscratch_write((&mut cpu::KERNEL_TRAP_FRAME[hartid] as *mut cpu::TrapFrame) as usize);

        cpu::sscratch_write(cpu::mscratch_read());
        cpu::KERNEL_TRAP_FRAME[hartid].hartid = hartid;
    }
}

/// Identity map range
/// Takes a contiguous allocation of memory and maps it using PAGE_SIZE
/// This assumes start <= end
pub fn id_map_range(root: &mut page::Table, start: usize, end: usize, bits: usize) {
    let mut memaddr = start & !(page::PAGE_SIZE - 1);
    let num_kb_pages = (page::align_val(end, 12) - memaddr) / page::PAGE_SIZE;

    for _ in 0..num_kb_pages {
        page::map(root, memaddr, memaddr, bits, 0);
        memaddr += 1 << PAGE_ORDER;
    }
}

mod assembly;
mod cpu;
mod kmem;
mod page;
mod trap;
mod uart;
