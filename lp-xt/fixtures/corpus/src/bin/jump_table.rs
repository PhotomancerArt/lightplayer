//! Dense match dispatch — exercises the compiler's jump-table lowering
//! (l32r + jx or branch trees).
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

#[inline(never)]
fn dispatch(op: u32, x: u32) -> u32 {
    match op % 12 {
        0 => x.wrapping_add(1),
        1 => x.wrapping_mul(3),
        2 => x ^ 0x5A5A,
        3 => x >> 3,
        4 => x << 2,
        5 => x.rotate_left(7),
        6 => x.count_ones(),
        7 => x.wrapping_sub(99),
        8 => x | 0x0001_0101,
        9 => x & 0xFFFF,
        10 => x.swap_bytes(),
        _ => !x,
    }
}

fn main(_arg: u32) -> u32 {
    let mut x = 1u32;
    for i in 0..48u32 {
        x = dispatch(i, x).wrapping_add(i);
    }
    println!("x={}", x);
    0
}
emu_main!(main);
