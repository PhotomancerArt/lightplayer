//! Ackermann — deep recursion far beyond the 64-register window ring
//! (spill/reload pressure, depth in the hundreds).
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

#[inline(never)]
fn ack(m: u32, n: u32) -> u32 {
    if m == 0 {
        n + 1
    } else if n == 0 {
        ack(m - 1, 1)
    } else {
        ack(m - 1, ack(m, n - 1))
    }
}

fn main(_arg: u32) -> u32 {
    println!("ack(1,10)={}", ack(1, 10));
    println!("ack(2,3)={}", ack(2, 3));
    println!("ack(3,3)={}", ack(3, 3));
    println!("ack(3,5)={}", ack(3, 5));
    0
}
emu_main!(main);
