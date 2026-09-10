//! Fixture (b) — a level-1 and a level-3 interrupt, taken and returned.
//!
//! The two halves are architecturally different and that is the point:
//!
//! - **Level 1 has no vector of its own.** It arrives through the general
//!   exception vector — `_UserExceptionVector` at `VECBASE + 0x340`, because
//!   `PS.UM` is 1 — carrying `EXCCAUSE = 4` (`Level1InterruptCause`), and
//!   returns through `rfe`, which clears `PS.EXCM`. "Interrupts vector by
//!   level" is wrong here and right at 2..7.
//! - **Level 3 does have one**, `_Level3InterruptVector` at `VECBASE + 0x1C0`,
//!   banks its `EPC3`/`EPS3`, and returns through `rfi 3`, which must restore
//!   `PS` *and* `PC` exactly and let the interrupted instruction stream carry
//!   on where it stopped.
//!
//! Both are raised with `wsr.intset`, which is what a software interrupt is
//! (RM Table 5-170: only software lines respond). That makes the preemption
//! point an architectural fact — the hart's poll point (b), the instruction
//! after the `wsr` — rather than a scheduling coincidence, so `EPC3` has one
//! right answer and the fixture can name it.
//!
//! ## Result slots
//!
//! | slot | value |
//! |---|---|
//! | 1 | how many times the level-1 handler ran |
//! | 2 | `EXCCAUSE` as the level-1 handler saw it |
//! | 3 | `PS` captured immediately before raising the level-1 interrupt |
//! | 4 | `PS` after the level-1 return |
//! | 5 | how many times the level-3 handler ran |
//! | 6 | `EPC3` (the host resolves it to a symbol) |
//! | 7 | `EPS3` |
//! | 8 | `PS` captured immediately before raising the level-3 interrupt |
//! | 9 | `PS` after the level-3 return |
//! | 10 | the marker the interrupted instruction stream wrote after `rfi 3` |

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

use mach::{finish, record, sr};
use xtensa_lx_rt::exception::Context;

/// Line 7: level 1, `IntKind::Software` — the classic's shape, and the table
/// the host test hands the hart through `CoreConfig`.
const LINE_SOFTWARE_L1: u32 = 1 << 7;
/// Line 29: level 3, `IntKind::Software`.
const LINE_SOFTWARE_L3: u32 = 1 << 29;

/// What the resumed instruction stream writes after `rfi 3`. Arbitrary, but it
/// has to be a value nothing else in the image produces.
const RESUME_MARKER: u32 = 0xA5A5_1234;

static mut L1_RUNS: u32 = 0;
static mut L3_RUNS: u32 = 0;

#[unsafe(no_mangle)]
extern "C" fn __level_1_interrupt(frame: &mut Context) {
    // SAFETY: single-threaded bare-metal fixture; level 1 cannot nest here
    // because the naked handler raises `PS.INTLEVEL` to 1 before calling.
    unsafe {
        L1_RUNS += 1;
        record(1, L1_RUNS);
    }
    record(2, frame.EXCCAUSE);
    // A software line stays latched until INTCLEAR drops it; without this the
    // `rfe` would walk straight back into the vector.
    sr::set_intclear(LINE_SOFTWARE_L1);
}

#[unsafe(no_mangle)]
extern "C" fn __level_3_interrupt(_frame: &mut Context) {
    // SAFETY: as above; the level-3 naked handler raises PS.INTLEVEL to 3.
    unsafe {
        L3_RUNS += 1;
        record(5, L3_RUNS);
    }
    // Read the banked registers from the SRs rather than from the save frame:
    // the frame is what lx-rt copied out of them, and the claim under test is
    // what the *hart* put there.
    record(6, sr::epc3());
    record(7, sr::eps3());
    sr::set_intclear(LINE_SOFTWARE_L3);
}

/// Raise the level-1 software interrupt and observe `PS` on both sides of it.
#[inline(never)]
#[unsafe(no_mangle)]
fn raise_level1() {
    let before = sr::ps();
    sr::set_intset(LINE_SOFTWARE_L1);
    // The interrupt is taken here (poll point (b): a `wsr` to INTSET is
    // delivery turning on).
    record(3, before);
    record(4, sr::ps());
}

/// Raise the level-3 software interrupt. The marker write and the second `PS`
/// read are inside the same `asm!` block as the `wsr.intset`, so they are
/// literally the interrupted instruction stream resuming after `rfi 3` — not a
/// later statement that merely proves the program did not crash.
#[inline(never)]
#[unsafe(no_mangle)]
fn raise_level3() {
    let before: u32;
    let after: u32;
    let marker: u32;
    // SAFETY: three `rsr`/`wsr`s and a `mov`; no memory effect. The interrupt
    // is taken between the `wsr.intset` and the `mov`.
    unsafe {
        core::arch::asm!(
            "rsr.ps {before}",
            "wsr.intset {line}",
            // --- everything below runs after `rfi 3` ---
            "mov {marker}, {seed}",
            "rsr.ps {after}",
            before = out(reg) before,
            after = out(reg) after,
            marker = out(reg) marker,
            seed = in(reg) RESUME_MARKER,
            line = in(reg) LINE_SOFTWARE_L3,
            options(nostack),
        );
    }
    record(8, before);
    record(9, after);
    record(10, marker);
}

#[xtensa_lx_rt::entry]
fn main() -> ! {
    // `Reset` does `wsr.intenable a0` with a0 = 0, so nothing is deliverable
    // until this.
    sr::set_intenable(LINE_SOFTWARE_L1 | LINE_SOFTWARE_L3);

    raise_level1();
    raise_level3();

    finish()
}
