//! Division and remainder: signed/unsigned 32-bit (quos/rems/quou/remu),
//! checked division edge cases, and 64-bit division libcalls.
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

#[inline(never)]
fn div32(a: i32, b: i32) -> (i32, i32) {
    (a / b, a % b)
}

#[inline(never)]
fn divu32(a: u32, b: u32) -> (u32, u32) {
    (a / b, a % b)
}

fn main(_arg: u32) -> u32 {
    println!("s={:?}", div32(-7, 2));
    println!("s2={:?}", div32(7, -2));
    println!("s3={:?}", div32(-2147483647, 3));
    println!("u={:?}", divu32(0xFFFF_FFFF, 10));
    println!("u2={:?}", divu32(12345, 12346));
    println!("edge={:?}", i32::MIN.checked_div(-1));
    println!("zero={:?}", 5i32.checked_div(0));
    let big: u64 = 0x0123_4567_89AB_CDEF;
    println!("d64={}", big / 1_000_000_007);
    println!("r64={}", big % 1_000_000_007);
    println!("i64={}", (-1234567890123i64) / 4096);
    0
}
emu_main!(main);
