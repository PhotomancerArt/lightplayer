//! Fixture (d) — `s32c1i` compare-and-swap, success **and** failure.
//!
//! `s32c1i at, as, imm` (ISA RM, the Conditional Store Option) is a
//! read-compare-write in one instruction: it loads `[as + imm]`, compares it
//! with `SCOMPARE1`, stores `at` there **only** if they matched, and — in
//! either case — leaves the value it *loaded* in `at`.
//!
//! Two ways to get it plausibly wrong: store unconditionally (which passes any
//! test that only exercises the matching case) and leave the *stored* value in
//! `at` rather than the loaded one (which passes any test that only checks
//! memory). Both directions are checked here, in both memory and the register.
//!
//! ## Result slots
//!
//! | slot | value |
//! |---|---|
//! | 1 | the cell after the CAS that should succeed |
//! | 2 | what that CAS left in the destination register |
//! | 3 | the cell after the CAS that should fail |
//! | 4 | what that CAS left in the destination register |
//! | 5,6,7 | the initial, new and deliberately-wrong values |

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

use mach::{finish, record};

const INITIAL: u32 = 0x1111_2222;
const NEW: u32 = 0x3333_4444;
/// Deliberately not the cell's value, so the second CAS must fail.
const WRONG: u32 = 0x5555_6666;
/// What the failing CAS would have written. Distinct from everything else, so
/// "nothing was stored" is a claim with a witness.
const OTHER: u32 = 0x7777_8888;

static mut CELL: u32 = 0;

/// `SCOMPARE1 <- expect; s32c1i new -> *cell`. Returns what the instruction
/// left in the destination register: the value it loaded, either way.
#[inline(never)]
fn cas(cell: *mut u32, expect: u32, new: u32) -> u32 {
    let mut value = new;
    // SAFETY: `cell` is a live, 4-byte-aligned `u32` in this image's `.bss`.
    unsafe {
        core::arch::asm!(
            "wsr.scompare1 {e}",
            "s32c1i {v}, {p}, 0",
            e = in(reg) expect,
            v = inout(reg) value,
            p = in(reg) cell,
            options(nostack),
        );
    }
    value
}

fn read_cell(cell: *const u32) -> u32 {
    // SAFETY: as `cas`.
    unsafe { cell.read_volatile() }
}

#[xtensa_lx_rt::entry]
fn main() -> ! {
    let cell = &raw mut CELL;
    // SAFETY: single-threaded bare-metal fixture.
    unsafe { cell.write_volatile(INITIAL) };

    let got = cas(cell, INITIAL, NEW);
    record(1, read_cell(cell));
    record(2, got);

    let got = cas(cell, WRONG, OTHER);
    record(3, read_cell(cell));
    record(4, got);

    record(5, INITIAL);
    record(6, NEW);
    record(7, WRONG);

    finish()
}
