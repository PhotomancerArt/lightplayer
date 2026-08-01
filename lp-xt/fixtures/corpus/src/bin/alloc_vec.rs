//! Heap use through the guest bump allocator: Vec growth (realloc/memcpy),
//! sort_unstable, and String formatting.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use lp_xt_emu_guest::{emu_main, println};

fn main(_arg: u32) -> u32 {
    let mut v: Vec<u32> = Vec::new();
    let mut seed = 0xCAFE_F00Du32;
    for _ in 0..50 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        v.push(seed % 1000);
    }
    let sum = v.iter().fold(0u32, |acc, &x| acc.wrapping_add(x));
    println!("len={} sum={}", v.len(), sum);

    v.sort_unstable();
    println!("first={} last={}", v[0], v[49]);

    let mut s = String::new();
    for &x in v.iter().take(5) {
        let _ = write!(s, "{:03},", x);
    }
    println!("head={}", s);
    0
}
emu_main!(main);
