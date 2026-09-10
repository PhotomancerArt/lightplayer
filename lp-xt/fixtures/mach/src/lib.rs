//! Shared runtime for the `mach` fixtures.
//!
//! There is deliberately no guest ABI here — no syscalls, no `println!`, no
//! host trap. `lp_xt_emu::mach::XtHart` runs against a plain [`Bus`] with no
//! MMIO in it, so a fixture's only channel to the host is **memory**: it writes
//! `u32` slots into [`MACH_RESULT`] and ends on `break`, and the host test
//! reads the slots back out of the bus by resolving the `MACH_RESULT` symbol in
//! the ELF.
//!
//! That keeps the fixtures architecture-only (no peripheral, no memory map) and
//! keeps the goldens host-independent: a slot is an architectural value —
//! `EXCCAUSE`, `LCOUNT`, a loaded word — not an address the linker chose.
//!
//! # The result protocol
//!
//! | slot | meaning |
//! |---|---|
//! | 0 | [`MAGIC_DONE`] when the fixture ran to completion, [`MAGIC_PANIC`] if it panicked, 0 if it never got there |
//! | 1.. | the fixture's own values, documented in its own file |
//!
//! The host asserts slot 0 **before** looking at anything else: a fixture that
//! fell into an exception loop leaves it 0, and a run that stopped early would
//! otherwise be read as a run whose values happened to be zero.

#![no_std]
#![feature(asm_experimental_arch)]

use core::sync::atomic::{Ordering, compiler_fence};

/// Slot 0 after [`finish`]: the fixture ran to its end.
pub const MAGIC_DONE: u32 = 0x4D41_4348; // "MACH"
/// Slot 0 after a panic.
pub const MAGIC_PANIC: u32 = 0xDEAD_4348;

/// Number of `u32` result slots. Sized for the widest fixture (the 25-deep
/// backtrace, which needs 32 frames plus its own bookkeeping).
pub const RESULT_SLOTS: usize = 64;

/// The fixture → host channel. `.bss`, `#[no_mangle]`, so the host test can
/// resolve it by name in the ELF symbol table and read it out of the bus.
///
/// Zero-initialized on purpose: an initialized static would land in `.data`,
/// whose LMA differs from its VMA under xtensa-lx-rt's linker script (see
/// `memory.x`).
#[unsafe(no_mangle)]
pub static mut MACH_RESULT: [u32; RESULT_SLOTS] = [0; RESULT_SLOTS];

/// Write one result slot.
///
/// # Panics
/// If `slot` is 0 (reserved for the completion magic) or out of range.
#[inline(never)]
pub fn record(slot: usize, value: u32) {
    assert!(slot > 0 && slot < RESULT_SLOTS, "result slot out of range");
    // SAFETY: single-threaded bare-metal fixture; the host reads this only
    // after the hart has stopped on `break`.
    unsafe {
        (&raw mut MACH_RESULT).cast::<u32>().add(slot).write_volatile(value);
    }
}

/// Read one result slot back (fixtures that accumulate).
#[must_use]
pub fn slot(slot: usize) -> u32 {
    assert!(slot < RESULT_SLOTS, "result slot out of range");
    // SAFETY: as `record`.
    unsafe { (&raw const MACH_RESULT).cast::<u32>().add(slot).read_volatile() }
}

/// Stamp slot 0 and stop the hart.
///
/// `break 1, 15` is the architectural stop: the hart reports
/// `SliceEnd::Ebreak { pc }` with `pc` *not* advanced, and the host test ends
/// its run there. The loop is unreachable in the emulator (the host never calls
/// `deliver_breakpoint`) and exists so the signature is `-> !` without a
/// `loop {}` that the optimizer could hoist the `break` out of.
pub fn finish() -> ! {
    stamp(MAGIC_DONE)
}

fn stamp(magic: u32) -> ! {
    // SAFETY: as `record`.
    unsafe {
        (&raw mut MACH_RESULT).cast::<u32>().write_volatile(magic);
    }
    compiler_fence(Ordering::SeqCst);
    loop {
        // SAFETY: `break` is a plain architectural stop with no operands.
        unsafe { core::arch::asm!("break 1, 15", options(nomem, nostack)) };
    }
}

#[panic_handler]
fn panicked(_info: &core::panic::PanicInfo) -> ! {
    stamp(MAGIC_PANIC)
}
