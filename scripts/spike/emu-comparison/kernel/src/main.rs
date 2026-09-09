// SPIKE — throwaway. The common yardstick kernel.
//
// `kernel()` is the measured work and is IDENTICAL across every platform: the
// same Rust, the same rv32imac codegen. Only the console/exit shim differs,
// and it runs a few dozen instructions at the very end. The checksum it prints
// is the identity oracle — every emulator must print the same word, or its
// number is meaningless.
#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

/// Integer-heavy, branch-light, load/store-mixed. Deterministic. No allocation,
/// no libc, no floating point (so nothing depends on an FPU the C6 lacks).
#[inline(never)]
fn kernel(iters: u32) -> u32 {
    let mut state = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        state[i] = (i as u32).wrapping_mul(2_654_435_761);
        i += 1;
    }
    let mut x: u32 = 0x1234_5678;
    let mut acc: u32 = 0;
    let mut n = 0u32;
    while n < iters {
        let mut j = 0usize;
        while j < 256 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let idx = ((x >> 8) as usize) & 255;
            let v = state[idx]
                .wrapping_add(state[j])
                .rotate_left(7)
                .wrapping_mul(0x85EB_CA6B);
            state[j] = v ^ x;
            acc = acc.wrapping_add(v).wrapping_mul(0x9E37_79B1) ^ (acc >> 11);
            j += 1;
        }
        n += 1;
    }
    acc ^ state[0] ^ state[255]
}

// ---------------------------------------------------------------- platforms

#[cfg(feature = "plat-c6")]
mod plat {
    const UART0_FIFO: *mut u32 = 0x6000_0000 as *mut u32;
    pub fn putc(b: u8) {
        unsafe { core::ptr::write_volatile(UART0_FIFO, b as u32) }
    }
    pub fn exit() -> ! {
        // lp-emu-esp32c6 stops on --exit-on / --timeout; spin until it does.
        loop {
            unsafe { core::arch::asm!("wfi") }
        }
    }
}

#[cfg(feature = "plat-virt")]
mod plat {
    const UART_THR: *mut u8 = 0x1000_0000 as *mut u8;
    const SIFIVE_TEST: *mut u32 = 0x0010_0000 as *mut u32;
    pub fn putc(b: u8) {
        unsafe { core::ptr::write_volatile(UART_THR, b) }
    }
    pub fn exit() -> ! {
        unsafe { core::ptr::write_volatile(SIFIVE_TEST, 0x5555) }
        loop {}
    }
}

#[cfg(feature = "plat-syscall")]
mod plat {
    pub fn putc(b: u8) {
        let buf = [b];
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a7") 64usize,          // write
                in("a0") 1usize,           // stdout
                in("a1") buf.as_ptr(),
                in("a2") 1usize,
                lateout("a0") _,
                options(nostack)
            );
        }
    }
    pub fn exit() -> ! {
        unsafe {
            core::arch::asm!("ecall", in("a7") 93usize, in("a0") 0usize, options(noreturn));
        }
    }
}

fn puts(s: &str) {
    for b in s.bytes() {
        plat::putc(b);
    }
}

fn put_hex(mut v: u32) {
    let digits = b"0123456789abcdef";
    let mut out = [0u8; 8];
    let mut i = 8;
    while i > 0 {
        i -= 1;
        out[i] = digits[(v & 0xf) as usize];
        v >>= 4;
    }
    for b in out {
        plat::putc(b);
    }
}

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    // ITERS is baked in at build time so every emulator runs identical work.
    let iters: u32 = match option_env!("YARDSTICK_ITERS") {
        Some(s) => parse_u32(s),
        None => 200,
    };
    let r = kernel(iters);
    puts("yardstick iters=");
    put_hex(iters);
    puts(" checksum=");
    put_hex(r);
    puts("\nYARDSTICK DONE\n");
    plat::exit()
}

const fn parse_u32(s: &str) -> u32 {
    let b = s.as_bytes();
    let mut v = 0u32;
    let mut i = 0;
    while i < b.len() {
        v = v * 10 + (b[i] - b'0') as u32;
        i += 1;
    }
    v
}

core::arch::global_asm!(
    ".section .text.init",
    ".globl _start",
    "_start:",
    "  la sp, _stack_top",
    "  j rust_main",
);
