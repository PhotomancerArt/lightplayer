//! Fixture (a) — a 25-deep call chain, walked by the **real** crash-report
//! backtrace. The milestone's headline test.
//!
//! `lpc_shared::backtrace::capture_frames` is what the shipping firmware calls
//! when it panics. On Xtensa it works in two steps
//! (`lp-core/lpc-shared/src/backtrace.rs`):
//!
//! 1. `force_window_spill` nests calls until `WindowBase` has rotated through
//!    all 16 units, so **the hardware's own window-overflow handler** — the one
//!    at `VECBASE + 0x000/0x080/0x100` in this image, xtensa-lx-rt's — writes
//!    every live frame to its stack save area. There is no software spill loop;
//!    `ENTRY`'s overflow check is the mechanism.
//! 2. `walk_save_area_chain` follows the base save-area chain from the live
//!    `a0`/`a1`: `[sp-16]` is the caller's `a0` and `[sp-12]` its `a1`, for
//!    every overflow width.
//!
//! So this fixture is a claim about the hart's *window exceptions*, made by
//! code that does not know it is being tested. The known wrong answer is on
//! record: a 25-deep chain reporting **19 identical PCs** — a spill that wrote
//! every frame's save area to the same place. Twenty-five distinct, correctly
//! ordered PCs is the right one.
//!
//! The 25 frames are 25 **separate functions** rather than one recursion on
//! purpose: a recursive chain's return addresses are legitimately identical, so
//! it could not tell a correct walk from the historical failure.
//!
//! ## Result slots
//!
//! | slot | value |
//! |---|---|
//! | 1 | frames the walk reported |
//! | 2 | the chain's depth (25) |
//! | 3.. | the captured PCs, innermost first |

#![no_std]
#![no_main]

use lpc_shared::backtrace::capture_frames;
use mach::{finish, record};

/// The chain's depth, and the number the assertion is about.
const DEPTH: u32 = 25;
/// Room for the 25 chain frames plus `main`, `Reset` and any capture
/// scaffolding, without saturating (a walk that saturates cannot be
/// distinguished from one that stopped).
const FRAMES: usize = 32;
/// Result slot the first captured PC lands in.
const FRAME_BASE: usize = 3;

/// The capture, in a frame of its own.
///
/// `#[inline(never)]` so that whether or not the optimizer folds
/// `capture_frames` into it, the chain's innermost function `bt_f25` is still a
/// *caller* and therefore still appears in the walk. Without it, an inlined
/// capture would read `bt_f25`'s own `a0`/`a1` and the walk would start at
/// `bt_f24` — 24 frames, right in every other respect, and wrong.
#[inline(never)]
#[unsafe(no_mangle)]
fn take_backtrace(buf: &mut [u32; FRAMES]) -> usize {
    capture_frames(buf)
}

/// Each link calls the next and returns its answer. `black_box` on both sides
/// so nothing becomes a tail call: a tail-called frame does not exist, and the
/// chain has to be as deep as it says it is.
macro_rules! link {
    ($name:ident => $next:ident) => {
        #[inline(never)]
        #[unsafe(no_mangle)]
        fn $name(buf: &mut [u32; FRAMES]) -> usize {
            let n = $next(core::hint::black_box(buf));
            core::hint::black_box(n)
        }
    };
}

link!(bt_f25 => take_backtrace);
link!(bt_f24 => bt_f25);
link!(bt_f23 => bt_f24);
link!(bt_f22 => bt_f23);
link!(bt_f21 => bt_f22);
link!(bt_f20 => bt_f21);
link!(bt_f19 => bt_f20);
link!(bt_f18 => bt_f19);
link!(bt_f17 => bt_f18);
link!(bt_f16 => bt_f17);
link!(bt_f15 => bt_f16);
link!(bt_f14 => bt_f15);
link!(bt_f13 => bt_f14);
link!(bt_f12 => bt_f13);
link!(bt_f11 => bt_f12);
link!(bt_f10 => bt_f11);
link!(bt_f09 => bt_f10);
link!(bt_f08 => bt_f09);
link!(bt_f07 => bt_f08);
link!(bt_f06 => bt_f07);
link!(bt_f05 => bt_f06);
link!(bt_f04 => bt_f05);
link!(bt_f03 => bt_f04);
link!(bt_f02 => bt_f03);
link!(bt_f01 => bt_f02);

#[xtensa_lx_rt::entry]
fn main() -> ! {
    let mut buf = [0u32; FRAMES];
    let n = bt_f01(&mut buf);

    record(1, n as u32);
    record(2, DEPTH);
    for (i, pc) in buf.iter().take(n).enumerate() {
        record(FRAME_BASE + i, *pc);
    }

    finish()
}
