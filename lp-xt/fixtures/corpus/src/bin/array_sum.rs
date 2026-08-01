//! Array fill (memset-style loops) + summation.
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

fn main(_arg: u32) -> u32 {
    let mut arr = [0u32; 64];
    for (i, slot) in arr.iter_mut().enumerate() {
        *slot = (i as u32).wrapping_mul(2654435761) >> 16;
    }
    let sum = arr.iter().fold(0u32, |acc, &v| acc.wrapping_add(v));
    println!("sum={}", sum);

    arr[10..30].fill(7);
    let sum2 = arr.iter().fold(0u32, |acc, &v| acc.wrapping_add(v));
    println!("sum2={}", sum2);

    let mut bytes = [0u8; 33];
    bytes.fill(0xAB);
    let bsum: u32 = bytes.iter().map(|&b| b as u32).sum();
    println!("bsum={}", bsum);
    0
}
emu_main!(main);
