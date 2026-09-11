//! Fixture (h) — a level-1 interrupt landing **between a `call8` and its
//! callee's `entry`**, on the deepest of a chain of CALL8 frames, through
//! xtensa-lx-rt's own `save_context` / `restore_context`, and then the returns.
//!
//! # The shape, and why it is this one
//!
//! `PS.CALLINC` is live for exactly one instruction: a `CALLn` writes it and
//! the callee's `ENTRY` consumes it. An interrupt that lands in that gap must
//! hand the value back untouched, and xtensa-lx-rt's `_UserExceptionVector`
//! makes that harder than it looks — it reaches its handler through `call0
//! __naked_user_exception` and only *then* does `rsr a0, PS` to save the
//! interruptee's `PS`. A hart on which CALL0 writes `PS.CALLINC` therefore
//! hands the handler a `PS` whose CALLINC is already gone, `restore_context` +
//! `rfe` put that `PS` back, and the interrupted `ENTRY` rotates by nothing:
//! the callee runs in its caller's window and the callee's stack pointer is
//! written into the caller's `a1`. M4 P4b found it on the classic as every
//! project load dying in `_WindowUnderflow8` with `a1 = 0`
//! (`lp-emu/esp/lp-emu-esp32v3/README.md`, "The window, across a context
//! save").
//!
//! The gap is hit **deterministically**, with `CCOMPARE0` rather than a
//! scripted line or `wsr.intset`: a `wsr.intset` lands on the very next
//! instruction (the hart's poll point (b)), which would be the `call8`
//! itself, and a scripted line lands at a slice boundary the fixture does not
//! choose. A timer match is delivered on the instruction that brings `CCOUNT`
//! to the compare (poll point (e)), so `p4b_round` arms `CCOMPARE0 = CCOUNT +
//! k` a fixed number of instructions before its `call8` and the fixture scans
//! `k`. The round whose interrupt has `EPC1 == p4b_leaf` is the one that landed
//! in the gap; the handler records the `PS` it was handed on that round.
//!
//! The chain `main -> d1 -> d2 -> d3 -> p4b_round -> p4b_leaf` is all CALL8,
//! so `WindowStart` is gapped (bits two apart) when the interrupt lands, the
//! `SPILL_REGISTERS` inside `save_context` has three older frames to spill
//! through `_WindowOverflow8`, and every return afterwards reloads one through
//! `_WindowUnderflow8` — the classic's exact shape.
//!
//! ## Result slots
//!
//! | slot | value |
//! |---|---|
//! | 1 | rounds run |
//! | 2 | rounds whose interrupt landed in the gap (`EPC1 == p4b_leaf`) |
//! | 3 | `k` of the first gap round |
//! | 4 | `PS.CALLINC` as the handler saw it on the first gap round — **must be 2** |
//! | 5 | `p4b_leaf`'s result on the first gap round — must be `ARG + 1` |
//! | 6 | interrupts taken |
//! | 7 | `WINDOWSTART` in `d3`, before the rounds (the gapped shape) |
//! | 8 | `WINDOWBASE` in `d3` |
//! | 9 | rounds whose result was not `ARG + 1` — must be 0 |
//! | 10 | 1 if the chain returned through every frame with the right sums |
//! | 11 | `EPC1` of the first gap round (the host resolves it to a symbol) |

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

use mach::{finish, record, sr};
use xtensa_lx_rt::exception::Context;

/// Line 6: level 1, `IntKind::Timer(0)` — `CCOMPARE0`'s line in the core
/// configuration the host test hands the hart.
const LINE_TIMER0_L1: u32 = 1 << 6;

const ARG: u32 = 100;
const K_FIRST: u32 = 3;
const K_LAST: u32 = 10;

static mut TAKEN: u32 = 0;
static mut LAST_PC: u32 = 0;
static mut LAST_PS: u32 = 0;

core::arch::global_asm!(
    "
    .section .text.p4b, \"ax\", @progbits

    // p4b_round(k: a2, arg: a3) -> a2
    //
    // Arms CCOMPARE0 = CCOUNT + k and, a fixed number of instructions later,
    // does the call8 whose callee's `entry` is the instruction under test.
    // The instruction count from the `rsr.ccount` to the `call8` is what
    // the scan over k is measured against; do not reorder these.
    .global p4b_round
    .p2align 2
    .type p4b_round, @function
p4b_round:
    entry   a1, 32
    rsr.ccount a4
    add     a4, a4, a2
    wsr.ccompare0 a4
    mov     a10, a3
    nop
    call8   p4b_leaf
    mov     a2, a10
    retw

    // p4b_leaf(x: a2) -> a2 = x + 1. Its `entry` is the interrupted
    // instruction on the gap round.
    .global p4b_leaf
    .p2align 2
    .type p4b_leaf, @function
p4b_leaf:
    entry   a1, 32
    addi    a2, a2, 1
    retw
    "
);

unsafe extern "C" {
    fn p4b_round(k: u32, arg: u32) -> u32;
    fn p4b_leaf(x: u32) -> u32;
}

/// The level-1 handler: remember what xtensa-lx-rt's `SAVE_CONTEXT` put in the
/// frame — `PC` is `EPC1`, `PS` is the `rsr a0, PS` it took *after* its
/// `call0` — and disarm the timer, whose request only a `CCOMPARE` write
/// clears.
#[unsafe(no_mangle)]
extern "C" fn __level_1_interrupt(frame: &mut Context) {
    // SAFETY: single-threaded bare-metal fixture; level 1 cannot nest (the
    // naked handler raised PS.INTLEVEL to 1 before calling).
    unsafe {
        TAKEN += 1;
        LAST_PC = frame.PC;
        LAST_PS = frame.PS;
        record(6, TAKEN);
    }
    sr::set_ccompare0(0);
}

fn callinc(ps: u32) -> u32 {
    (ps >> 16) & 3
}

#[inline(never)]
fn d3(x: u32) -> u32 {
    record(7, sr::windowstart());
    record(8, sr::windowbase());
    let leaf = p4b_leaf as *const () as usize as u32;
    let mut gap_rounds = 0u32;
    let mut mismatches = 0u32;
    let mut rounds = 0u32;
    for k in K_FIRST..=K_LAST {
        // SAFETY: `p4b_round` is the windowed-ABI function above; it arms a
        // timer interrupt that the handler disarms.
        unsafe {
            LAST_PC = 0;
            LAST_PS = 0;
        }
        // SAFETY: as above.
        let got = unsafe { p4b_round(k, ARG) };
        rounds += 1;
        record(1, rounds);
        if got != ARG + 1 {
            mismatches += 1;
            record(9, mismatches);
        }
        // SAFETY: the handler wrote these before this round returned.
        let (pc, ps) = unsafe { (LAST_PC, LAST_PS) };
        if pc == leaf {
            gap_rounds += 1;
            record(2, gap_rounds);
            if gap_rounds == 1 {
                record(3, k);
                record(4, callinc(ps));
                record(5, got);
                record(11, pc);
            }
        }
    }
    x + 3
}

#[inline(never)]
fn d2(x: u32) -> u32 {
    d3(x + 2) + 20
}

#[inline(never)]
fn d1(x: u32) -> u32 {
    d2(x + 1) + 10
}

#[xtensa_lx_rt::entry]
fn main() -> ! {
    record(9, 0);
    record(10, 0);
    // `Reset` leaves INTENABLE at 0.
    sr::set_intenable(LINE_TIMER0_L1);
    let got = d1(1000);
    // 1000 -> d1: 1001 -> d2: 1003 -> d3: 1006, then +20, +10.
    record(10, u32::from(got == 1036));
    finish()
}
