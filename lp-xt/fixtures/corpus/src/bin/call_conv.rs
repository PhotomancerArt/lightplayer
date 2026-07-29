//! Calling-convention exercise: many arguments (spilling past the register
//! args), small and large struct returns, u64 args/returns on a 32-bit target.
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

#[inline(never)]
fn many(a: u32, b: u32, c: u32, d: u32, e: u32, f: u32, g: u32, h: u32) -> u32 {
    (a ^ b)
        .wrapping_add(c.wrapping_mul(d))
        .wrapping_sub(e)
        .wrapping_add(f << 2)
        .wrapping_add(g % h)
}

#[derive(Clone, Copy)]
struct Quad {
    a: u32,
    b: u32,
    c: u32,
    d: u32,
}

#[inline(never)]
fn make_quad(x: u32) -> Quad {
    Quad {
        a: x.wrapping_add(1),
        b: x.wrapping_mul(3),
        c: x ^ 0x00FF_00FF,
        d: x.rotate_left(9),
    }
}

#[inline(never)]
fn make_big(x: u32) -> [u32; 8] {
    let mut out = [0u32; 8];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = x.wrapping_mul(i as u32 + 1) ^ (i as u32);
    }
    out
}

#[inline(never)]
fn wide(a: u64, b: u64) -> u64 {
    a.wrapping_mul(3).wrapping_add(b)
}

fn main(_arg: u32) -> u32 {
    println!("many={}", many(1, 2, 3, 4, 5, 6, 7, 8));
    println!(
        "many2={}",
        many(0xDEAD, 0xBEEF, 0x1234, 0x5678, 99, 1000, 123456, 789)
    );
    let q = make_quad(0xABCD_EF01);
    println!("quad={},{},{},{}", q.a, q.b, q.c, q.d);
    let big = make_big(0x1357_9BDF);
    let bsum = big.iter().fold(0u32, |acc, &v| acc.wrapping_add(v));
    println!("bigsum={}", bsum);
    println!("wide={}", wide(0x1_0000_0001, 0xFFFF_FFFF));
    0
}
emu_main!(main);
