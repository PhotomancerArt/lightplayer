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

unsafe extern "C" {
    /// `_stack_start_cpu0` from `memory.x`. An absolute linker symbol: its
    /// *address* is the value.
    static _stack_start_cpu0: u32;
    /// `_boot_frame_sp` from `memory.x` — the stack pointer of the frame that
    /// "called" `Reset`. The host runner seeds `a1` with it.
    static _boot_frame_sp: u32;
    /// `_boot_frame_caller_sp` from `memory.x`.
    static _boot_frame_caller_sp: u32;
}

/// Seed the stack frame the second-stage bootloader would have left below
/// `Reset`.
///
/// `xtensa-lx-rt` PROVIDEs `__pre_init = no_init_hook` and calls it from
/// `Reset` as the first thing after the stack pointer is set; defining it here
/// takes over.
///
/// **Why it has to exist** is `memory.x`'s `_boot_frame_sp` comment in full;
/// the short version is that `XtHart::new` leaves frame 0 resident, `PS_BOOT`
/// makes `Reset` frame 2, and the first `SPILL_REGISTERS` therefore takes
/// `_WindowOverflow8` for frame 0 — whose `l32e a0, a1, -12` needs a stack
/// under it, and whose `s32e a4, a0, -32` needs one under *that*. Without both,
/// a load/store error inside a window handler becomes a double exception and
/// the run never returns.
///
/// This runs before `.bss` is zeroed, which is fine: it writes stack, not
/// statics.
#[unsafe(no_mangle)]
pub extern "C" fn __pre_init() {
    let boot_sp = (&raw const _boot_frame_sp) as u32;
    let boot_caller_sp = (&raw const _boot_frame_caller_sp) as u32;
    let reset_sp = (&raw const _stack_start_cpu0) as u32;
    // SAFETY: both save areas are in the reserved 4 KiB above the stack, inside
    // the one modeled region, and nothing else in the image writes them.
    unsafe {
        // The boot frame's own base save area: what its caller's spill would
        // have left. `a0 = 0` is the backtrace chain's terminator.
        ((boot_sp - 16) as *mut u32).write_volatile(0);
        ((boot_sp - 12) as *mut u32).write_volatile(boot_caller_sp);
        ((boot_sp - 8) as *mut u32).write_volatile(0);
        ((boot_sp - 4) as *mut u32).write_volatile(0);
        // `Reset`'s base save area. The boot frame's own spill writes exactly
        // these two values into it; seeding them here makes the chain walk read
        // the same thing whether or not that spill has happened yet.
        ((reset_sp - 16) as *mut u32).write_volatile(0);
        ((reset_sp - 12) as *mut u32).write_volatile(boot_sp);
        ((reset_sp - 8) as *mut u32).write_volatile(0);
        ((reset_sp - 4) as *mut u32).write_volatile(0);
    }
}

/// A global allocator that never allocates.
///
/// `lpc-shared` (fixture (a)'s backtrace walker) pulls `alloc` into the link
/// through its own dependency tree, so the image needs a `#[global_allocator]`
/// to build. It does not need one to *run*: nothing on a fixture's path
/// allocates, and if something ever starts to, a null return becomes
/// `handle_alloc_error` -> panic -> [`MAGIC_PANIC`] in slot 0, which the host
/// test reports by name. A bump allocator here would instead let an accidental
/// allocation succeed quietly and change what the fixture measures.
struct NeverAlloc;

// SAFETY: returning null is the documented "allocation failed" answer, and
// `dealloc` is unreachable because nothing is ever handed out.
unsafe impl core::alloc::GlobalAlloc for NeverAlloc {
    unsafe fn alloc(&self, _layout: core::alloc::Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {
        unreachable!("a mach fixture allocated and then freed")
    }
}

#[global_allocator]
static ALLOCATOR: NeverAlloc = NeverAlloc;

/// Special-register access the fixtures need, spelled once.
///
/// Every one of these is a single instruction with no memory effect; they are
/// `#[inline(always)]` so a fixture's own function extents stay meaningful when
/// a captured PC is attributed to a symbol.
pub mod sr {
    macro_rules! reader {
        ($(#[$m:meta])* $name:ident, $insn:literal) => {
            $(#[$m])*
            #[inline(always)]
            #[must_use]
            pub fn $name() -> u32 {
                let v: u32;
                // SAFETY: a single `rsr`, no memory effect.
                unsafe { core::arch::asm!($insn, v = out(reg) v, options(nomem, nostack)) };
                v
            }
        };
    }
    macro_rules! writer {
        ($(#[$m:meta])* $name:ident, $insn:literal) => {
            $(#[$m])*
            #[inline(always)]
            pub fn $name(value: u32) {
                // SAFETY: a single `wsr`. The caller owns the consequences —
                // `intset` in particular raises an interrupt at the next
                // instruction (the hart's poll point (b)).
                unsafe { core::arch::asm!($insn, v = in(reg) value, options(nostack)) };
            }
        };
    }

    reader!(/// `PS`.
        ps, "rsr.ps {v}");
    reader!(/// `EPC3`, the PC a level-3 interrupt preempted.
        epc3, "rsr.epc3 {v}");
    reader!(/// `EPS3`, the PS a level-3 interrupt preempted.
        eps3, "rsr.eps3 {v}");
    reader!(/// `DEBUGCAUSE` (RM §4.7.6.2, Table 4-123).
        debugcause, "rsr.debugcause {v}");
    reader!(/// `INTERRUPT` (SR 226): the lines pending right now.
        interrupt, "rsr.interrupt {v}");
    reader!(/// `WINDOWSTART`: one bit per resident frame.
        windowstart, "rsr.windowstart {v}");
    reader!(/// `WINDOWBASE`: the current frame's base, in groups of four.
        windowbase, "rsr.windowbase {v}");

    writer!(/// `INTENABLE`. `Reset` leaves this at 0.
        set_intenable, "wsr.intenable {v}");
    writer!(/// `INTSET`: raise a software interrupt. Only software lines
        /// respond (RM Table 5-170).
        set_intset, "wsr.intset {v}");
    writer!(/// `INTCLEAR`: drop an edge or software line.
        set_intclear, "wsr.intclear {v}");
    writer!(/// `CCOMPARE0`: arm timer 0, and clear its pending request (RM
        /// §4.4.6.2 — the one thing that does).
        set_ccompare0, "wsr.ccompare0 {v}");
    writer!(/// `DBREAKA0`, the watched address.
        set_dbreaka0, "wsr.dbreaka0 {v}");
    writer!(/// `DBREAKC0`: bit 31 = break on stores, bit 30 = on loads,
        /// bits 5:0 = the address mask (`111111` = one byte).
        set_dbreakc0, "wsr.dbreakc0 {v}");
}

/// The length in bytes of the instruction whose first byte is `b0`.
///
/// The Density Option's rule (ISA RM §5, the instruction-format tables): `op0`
/// — the low nibble of the first byte — selects the format, and `op0 >= 8` is a
/// 16-bit (`*.n`) instruction. A handler that wants to *skip* the instruction
/// it faulted on needs this, and needs it from the encoding rather than from
/// the mnemonic it wrote: the assembler narrows `l32i` to `l32i.n` on its own,
/// so a hard-coded `+3` silently lands mid-instruction.
#[must_use]
pub const fn inst_len(b0: u8) -> u32 {
    if b0 & 0x0F >= 0x08 { 2 } else { 3 }
}
