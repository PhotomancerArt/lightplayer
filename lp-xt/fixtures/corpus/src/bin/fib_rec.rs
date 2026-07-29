//! Recursive Fibonacci — call-tree recursion with window pressure.
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

#[inline(never)]
fn fib(n: u32) -> u32 {
    if n < 2 {
        n
    } else {
        fib(n - 1).wrapping_add(fib(n - 2))
    }
}

fn main(_arg: u32) -> u32 {
    for n in [0u32, 1, 2, 5, 10, 15, 20] {
        println!("fib({})={}", n, fib(n));
    }
    0
}
emu_main!(main);
