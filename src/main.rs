#![no_std]
#![no_main]
#![feature(lang_items)]
#![allow(internal_features)]

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

#[unsafe(no_mangle)]
extern "C" fn kmain() {
    // Main should initialize all the things
    let uart = uart::Uart::new(0x1000_0000);
    uart.init();
    page::init();

    println!();
    println!("/*******************************/");
    println!("/**** Welcome to Rust land! ****/");
    println!("/*******************************/");

    _ = page::alloc_page(1);
    let a = page::alloc_page(1);
    _ = page::alloc_page(128);
    _ = page::zalloc_page(1);
    _ = page::zalloc_page(1000);
    page::print_page_allocations();

    page::free_page(a);
    page::print_page_allocations();

    // Terrible keyboard input echo loop. It will miss stuff, and just bruteforce type, but it's a start.
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

mod assembly;
mod page;
mod uart;
