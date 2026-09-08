//! The speed probe's workload: a trip-counted mixed integer/memory/call loop.
//!
//! Every other fixture in this corpus is a short conformance program — the
//! longest, `ackermann`, retires 292 k instructions. This one is long *by
//! request*: it reads the `arg` the emulator passes to `main` as a round
//! count, so a single run at a large `arg` retires as many instructions as
//! `scripts/emu/bench-xt.sh` needs (~1.5 k instructions per round), and the
//! probe never has to re-create the emulator to reach a probe-sized run.
//!
//! `arg = 0` means one round, so the fixture still behaves like a normal
//! member of the corpus when run with the default argument.
//!
//! The round blends the three shapes the probe cares about:
//!
//!   * **memory** — a 64-word working set read and written every round;
//!   * **calls** — a non-inlined `step` per element, so the windowed
//!     call/entry/retw path is paid ~64 times a round;
//!   * **window spill** — one 24-deep recursion a round, past the 64-AR
//!     register ring, which is the traffic `ackermann` used to contribute.
#![no_std]
#![no_main]

use lp_xt_emu_guest::{emu_main, println};

const N: usize = 64;

/// One element's mix. `#[inline(never)]` so the inner loop pays a real
/// windowed call rather than being flattened into straight-line arithmetic.
#[inline(never)]
fn step(v: u32, acc: u32, i: u32) -> u32 {
    let r = (v ^ acc).rotate_left(i & 31);
    r.wrapping_add(v >> 3).wrapping_mul(2654435761) ^ (acc >> 11)
}

/// A recursion whose caller-side value stays live across the call, so each
/// level needs a real frame — at depth 24 (`call8`) that is 192 ARs against a
/// 64-AR ring, i.e. guaranteed window overflow/underflow traffic.
#[inline(never)]
fn spill(n: u32, x: u32) -> u32 {
    if n == 0 {
        x
    } else {
        spill(n - 1, x.rotate_left(3).wrapping_add(n)) ^ (n << 2)
    }
}

fn main(arg: u32) -> u32 {
    let rounds = if arg == 0 { 1 } else { arg };

    let mut buf = [0u32; N];
    let mut seed = 0x1234_5678u32;
    for slot in buf.iter_mut() {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *slot = seed;
    }

    let mut acc = 0xC0FF_EE00u32;
    for r in 0..rounds {
        for i in 0..N {
            let w = step(buf[i], acc, i as u32);
            buf[i] = w;
            acc = acc.wrapping_mul(1664525).wrapping_add(1013904223) ^ (w >> 7);
        }
        acc = acc.wrapping_add(spill(24, r ^ acc));
    }

    let checksum = buf
        .iter()
        .enumerate()
        .fold(0u32, |a, (i, &v)| a.wrapping_add(v.rotate_left(i as u32 & 31)));

    println!("rounds={}", rounds);
    println!("acc={}", acc);
    println!("checksum={}", checksum);
    0
}
emu_main!(main);
