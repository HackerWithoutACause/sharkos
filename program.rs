#![feature(start, naked_functions)]
#![no_std]
#![no_main]

use core::arch::asm;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    loop {}
}

macro_rules! explicit_syscall {
    ($($arg:expr),*; $($reg:tt),*) => {
        {
            let result: usize;
            unsafe {
                asm!(
                    "syscall",
                    $(in($reg) $arg,)*
                    lateout("rax") result,
                    out("rcx") _,
                    out("r11") _,
                    options(nostack)
                );
            }
            result
        }
    };
}

macro_rules! syscall {
    ($code:expr) => { explicit_syscall!($code; "rax") };
    ($code:expr, $r1:expr) => { explicit_syscall!($code, $r1; "rax", "rdi") };
    ($code:expr, $r1:expr, $r2:expr) => { explicit_syscall!($code, $r1, $r2; "rax", "rdi", "rsi") };
    ($code:expr, $r1:expr, $r2:expr, $r3:expr) => { explicit_syscall!($code, $r1, $r2, $r3; "rax", "rdi", "rsi", "rdx") };
    ($code:expr, $r1:expr, $r2:expr, $r3:expr, $r4:expr) => { explicit_syscall!($code, $r1, $r2, $r3, $r4; "rax", "rdi", "rsi", "rdx", "r10") };
    ($code:expr, $r1:expr, $r2:expr, $r3:expr, $r4:expr, $r5:expr) => { explicit_syscall!($code, $r1, $r2, $r3, $r4, $r5; "rax", "rdi", "rsi", "rdx", "r10", "r8") };
    ($code:expr, $r1:expr, $r2:expr, $r3:expr, $r4:expr, $r5:expr, $r6:expr) => { explicit_syscall!($code, $r1, $r2, $r3, $r4, $r5, $r6; "rax", "rdi", "rsi", "rdx", "r10", "r8", "r9") };
}

#[macro_export]
macro_rules! println {
    () => (print!("\n"));
    ($($arg:tt)*) => (print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::_print(format_args!($($arg)*)));
}

struct SystemWriter;

impl core::fmt::Write for SystemWriter {
    fn write_str(&mut self, message: &str) -> core::fmt::Result {
        syscall!(1, message.as_ptr(), message.len());

        Ok(())
    }
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    SystemWriter.write_fmt(args).ok();
}

extern "C" fn print(message: &str) {
    syscall!(1, message.as_ptr(), message.len());
}

fn exit(code: u64) -> ! {
    unsafe { asm!("mov rax, 0", "syscall", in("rdi") code, options(noreturn)) }
}

#[start]
#[no_mangle]
unsafe extern "C" fn _start() {
    loop {
        println!("from process 2");
        for _ in 0..10_000_000 {}
    }
    exit(0);
}
