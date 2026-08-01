//! core::fmt stress: widths, fills, hex/binary/octal, signed values, u64
//! decimal (64-bit division in the formatter).
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

fn main(_arg: u32) -> u32 {
    println!("[{:>8}]", 42);
    println!("[{:<8}]", -42);
    println!("[{:08x}]", 0xBEEFu32);
    println!("[{:#010X}]", 0xDEAD_BEEFu32);
    println!("[{:b}]", 0b1011_0101u32);
    println!("[{:o}]", 0o755u32);
    println!("[{:+}]", 17i32);
    println!("[{}]", i32::MIN);
    println!("[{}]", u64::MAX);
    println!("[{}]", 1234567890123456789u64);
    println!("[{:^11}]", "mid");
    println!("[{:?}]", (1u8, -2i16, 3u32));
    0
}
emu_main!(main);
