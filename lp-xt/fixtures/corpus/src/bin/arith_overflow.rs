//! Integer arithmetic + wrapping/checked overflow behavior.
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

fn main(_arg: u32) -> u32 {
    let a: u32 = 0xDEAD_BEEF;
    let b: u32 = 0x1234_5678;
    println!("wadd={}", a.wrapping_add(b));
    println!("wsub={}", b.wrapping_sub(a));
    println!("wmul={}", a.wrapping_mul(2654435761));
    println!("checked_none={:?}", u32::MAX.checked_add(1));
    println!("checked_some={:?}", 40u32.checked_add(2));
    let s: i32 = -1000;
    println!("sar={}", s >> 3);
    println!("shl={}", (s as u32) << 5);
    println!("imul={}", s.wrapping_mul(-7654321));
    println!("iwrap={}", i32::MIN.wrapping_sub(1));
    0
}
emu_main!(main);
