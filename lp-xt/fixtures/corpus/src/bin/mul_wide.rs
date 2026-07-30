//! Wide multiplies: 32x32→64 (mull/muluh/mulsh paths), 64x64, and
//! mixed-signedness products.
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

#[inline(never)]
fn mulhi_u(a: u32, b: u32) -> u32 {
    ((a as u64 * b as u64) >> 32) as u32
}

#[inline(never)]
fn mulhi_s(a: i32, b: i32) -> i32 {
    ((a as i64 * b as i64) >> 32) as i32
}

fn main(_arg: u32) -> u32 {
    println!("lo={}", 0xDEAD_BEEFu32.wrapping_mul(0xCAFE_BABE));
    println!("hiu={}", mulhi_u(0xDEAD_BEEF, 0xCAFE_BABE));
    println!("his={}", mulhi_s(-559038737, 19088743));
    println!("m64={}", 0x1234_5678_9ABCu64.wrapping_mul(0xFEDC_BA98));
    println!("m64s={}", (-123456789012i64).wrapping_mul(987654321));
    println!("sq={}", 0xFFFF_FFFFu64.wrapping_mul(0xFFFF_FFFF));
    0
}
emu_main!(main);
