//! Insertion sort over an LCG-filled array — nested loops, element moves,
//! comparisons.
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

fn main(_arg: u32) -> u32 {
    let mut arr = [0u32; 32];
    let mut seed = 0x1234_5678u32;
    for slot in arr.iter_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *slot = seed >> 8;
    }

    // Insertion sort.
    for i in 1..arr.len() {
        let key = arr[i];
        let mut j = i;
        while j > 0 && arr[j - 1] > key {
            arr[j] = arr[j - 1];
            j -= 1;
        }
        arr[j] = key;
    }

    let sorted = arr.windows(2).all(|w| w[0] <= w[1]);
    let checksum = arr
        .iter()
        .enumerate()
        .fold(0u32, |acc, (i, &v)| acc.wrapping_add(v.rotate_left(i as u32)));
    println!("sorted={}", sorted);
    println!("min={}", arr[0]);
    println!("max={}", arr[31]);
    println!("checksum={}", checksum);
    0
}
emu_main!(main);
