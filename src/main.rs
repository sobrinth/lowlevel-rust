#![no_std]
#![no_main]
#![feature(lang_items)]
#![allow(internal_features)]

use core::arch::asm;

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

#[macro_export]
macro_rules! print {
    ($($args:tt)+) => {{}};
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
    // Welcome to Rust land!
}

pub mod assembly;
