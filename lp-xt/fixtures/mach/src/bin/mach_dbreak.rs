//! Fixture (f) — a `DBREAK` stack guard.
//!
//! A data watchpoint is only useful if it traps **before** the access. A guard
//! word below a task's stack that fires *after* the overrun has already been
//! written has told you about a corruption it failed to prevent, and every
//! forensic claim built on it is a claim about memory that was already wrong.
//!
//! So the assertion is not just "the debug exception was taken": the handler
//! reads the guarded word back and reports what it held. It must still be the
//! pre-store value.
//!
//! `DBREAKC0 = STORE | 0x3F` is an exact one-byte match on `DBREAKA0`
//! (RM §4.7.6.3, Table 4-124: bit 31 breaks on stores, bits 5:0 are the address
//! mask, all ones = one byte). The debug exception is taken at `DEBUGLEVEL` (6)
//! and vectors to `VECBASE + 0x280`, carrying `DEBUGCAUSE` bit 2 (`DBREAK`)
//! with the matching slot in bits 11:8.
//!
//! The handler disarms the slot and returns; `rfi 6` re-executes the store,
//! which now completes — so slot 5 (the word at the end) is the proof that the
//! store was the *same* store, resumed, and not something the handler faked.
//!
//! ## Result slots
//!
//! | slot | value |
//! |---|---|
//! | 1 | the guarded word as the handler read it back |
//! | 2 | `DEBUGCAUSE` |
//! | 3 | the guarded word before the store |
//! | 4 | how many times the handler ran |
//! | 5 | the guarded word after the run |
//! | 6 | the value the store meant to write |
//! | 7 | the `DBREAK` slot named by `DEBUGCAUSE` bits 11:8 |

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

use mach::{finish, record, sr};
use xtensa_lx_rt::exception::Context;

/// `DBREAKC`: break on stores, exact byte (mask = all ones).
const DBREAKC_STORE_EXACT: u32 = (1 << 31) | 0x3F;
/// `DEBUGCAUSE` bits 11:8 name the slot that matched.
const DEBUGCAUSE_DBNUM_SHIFT: u32 = 8;

/// The sentinel a stack guard holds, and the value the "overrun" writes.
const GUARD_PATTERN: u32 = 0xFEED_FACE;
const OVERRUN_VALUE: u32 = 0x0BAD_0BAD;

/// The guarded word. In `.bss`, so its address is a link-time constant the
/// handler and `main` agree on without passing it around.
static mut GUARD: u32 = 0;
static mut HANDLER_RUNS: u32 = 0;

/// The debug exception arrives at level 6 (`XCHAL_DEBUGLEVEL`), so lx-rt's
/// `__default_naked_level_6_interrupt` saves the context and calls this.
#[unsafe(no_mangle)]
extern "C" fn __level_6_interrupt(_frame: &mut Context) {
    let cause = sr::debugcause();
    // SAFETY: single-threaded bare-metal fixture.
    unsafe {
        HANDLER_RUNS += 1;
        record(4, HANDLER_RUNS);
        // THE point of the fixture: what does the guarded word hold *now*?
        record(1, (&raw const GUARD).read_volatile());
    }
    record(2, cause);
    record(7, (cause >> DEBUGCAUSE_DBNUM_SHIFT) & 0xF);

    // Disarm, so the `rfi 6` that follows re-executes the store to completion
    // instead of trapping again forever.
    sr::set_dbreakc0(0);
}

/// The "overrun". `#[inline(never)]` so the store has one home.
#[inline(never)]
#[unsafe(no_mangle)]
fn overrun(target: *mut u32, value: u32) {
    // SAFETY: `target` is this image's own `GUARD`.
    unsafe { target.write_volatile(value) };
}

#[xtensa_lx_rt::entry]
fn main() -> ! {
    let guard = &raw mut GUARD;
    // SAFETY: single-threaded bare-metal fixture.
    unsafe { guard.write_volatile(GUARD_PATTERN) };
    record(3, GUARD_PATTERN);
    record(6, OVERRUN_VALUE);

    sr::set_dbreaka0(guard as u32);
    sr::set_dbreakc0(DBREAKC_STORE_EXACT);

    overrun(guard, OVERRUN_VALUE);

    // SAFETY: as above.
    record(5, unsafe { guard.read_volatile() });
    finish()
}
