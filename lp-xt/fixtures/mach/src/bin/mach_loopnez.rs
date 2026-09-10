//! Fixture (c) — `loopnez` with `LCOUNT > 1`.
//!
//! The failure this exists to catch is the silent single iteration: a hart that
//! executes the loop body once and falls through looks *exactly* like a correct
//! one to any test that only checks the value the loop computed, because the
//! loop is usually written so one iteration already produces something.
//!
//! `loopnez as, label` sets `LBEG` to the instruction after the `loop`, `LEND`
//! to `label`, and `LCOUNT` to `as - 1`; the body therefore runs `LCOUNT + 1`
//! times and `LCOUNT` lands at 0 (ISA RM §3.5.4.1 — the loop-back is taken on
//! the sequential next PC when it equals `LEND`, `LCOUNT != 0` and
//! `PS.EXCM == 0`).
//!
//! Three counts, in the exact-test-list's shape: `LCOUNT = 0` (the degenerate
//! one iteration), `LCOUNT = 1`, and `LCOUNT = 7`.
//!
//! ## Result slots
//!
//! | slot | value |
//! |---|---|
//! | 1,2 | body iterations / `LCOUNT` after, for `as = 1` (`LCOUNT` starts at 0) |
//! | 3,4 | ditto for `as = 2` (`LCOUNT` starts at 1) |
//! | 5,6 | ditto for `as = 8` (`LCOUNT` starts at 7) |
//! | 7 | `LCOUNT` read *inside* the first body iteration of the `as = 8` loop |

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

use mach::{finish, record};

/// Run a two-instruction `loopnez` body `n` times, returning
/// `(iterations, LCOUNT after the loop, LCOUNT seen in the first iteration)`.
///
/// The body is two instructions on purpose: a one-instruction zero-overhead
/// loop is a shape the ISA allows but that no compiler emits, and this fixture
/// is meant to stand for the code that ships.
#[inline(never)]
fn run_loopnez(n: u32) -> (u32, u32, u32) {
    let iterations: u32;
    let lcount_after: u32;
    let lcount_first: u32;
    // SAFETY: no memory is touched and no register outside the operand list is
    // clobbered. `LBEG`/`LEND`/`LCOUNT` are left at their post-loop values
    // (`LCOUNT == 0`), which is the architectural state after any loop.
    unsafe {
        core::arch::asm!(
            "movi   {it}, 0",
            "movi   {lf}, 0xFFFFFFFF",
            "loopnez {n}, 22f",
            // --- body ---
            "  addi {it}, {it}, 1",
            "  bnei {it}, 1, 21f",       // only the first iteration records
            "  rsr.lcount {lf}",
            "21:",
            "  nop",
            "22:",
            "rsr.lcount {la}",
            it = out(reg) iterations,
            la = out(reg) lcount_after,
            lf = out(reg) lcount_first,
            n = in(reg) n,
            options(nostack),
        );
    }
    (iterations, lcount_after, lcount_first)
}

#[xtensa_lx_rt::entry]
fn main() -> ! {
    let (it0, lc0, _) = run_loopnez(1);
    record(1, it0);
    record(2, lc0);

    let (it1, lc1, _) = run_loopnez(2);
    record(3, it1);
    record(4, lc1);

    let (it7, lc7, first7) = run_loopnez(8);
    record(5, it7);
    record(6, lc7);
    record(7, first7);

    finish()
}
