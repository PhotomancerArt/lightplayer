//! Bit manipulation: popcount, clz/ctz (NSAU paths), rotates, byte swaps,
//! bit reversal — on u32 and u64.
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

fn main(_arg: u32) -> u32 {
    let v: u32 = 0xF00D_CAFE;
    println!("ones={}", v.count_ones());
    println!("zeros={}", v.count_zeros());
    println!("lz={}", 0x0000_1000u32.leading_zeros());
    println!("tz={}", 0x0000_1000u32.trailing_zeros());
    println!("lz0={}", 0u32.leading_zeros());
    println!("rotl={}", v.rotate_left(13));
    println!("rotr={}", v.rotate_right(7));
    println!("swap={}", v.swap_bytes());
    println!("rev={}", v.reverse_bits());
    let w: u64 = 0x0123_4567_89AB_CDEF;
    println!("ones64={}", w.count_ones());
    println!("lz64={}", w.leading_zeros());
    println!("swap64={}", w.swap_bytes());
    0
}
emu_main!(main);
