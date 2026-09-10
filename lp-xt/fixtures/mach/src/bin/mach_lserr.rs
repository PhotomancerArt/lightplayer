//! Fixture (g) — a load/store error, taken through xtensa-lx-rt's real user
//! exception vector.
//!
//! A load from an address the bus does not map raises `EXCCAUSE = 3`
//! (`LoadStoreError`) with `EXCVADDR` set to the faulting address and `EPC1`
//! naming the faulting instruction, and — because `PS.UM` is 1, as the
//! bootloader leaves it — vectors to `_UserExceptionVector` at `VECBASE +
//! 0x340`, not to the kernel vector. Every one of those four is a separate way
//! to be wrong; a hart that raises the right cause at the wrong vector, or the
//! right vector with a stale `EXCVADDR`, would look correct to a test that
//! checked only the cause.
//!
//! The handler resumes *past* the faulting instruction rather than retrying it,
//! which is what makes the fixture terminate. The skip distance comes from the
//! encoding ([`mach::inst_len`]), not from the mnemonic: the assembler narrows
//! `l32i` to `l32i.n` on its own and a hard-coded `+3` would land mid-
//! instruction.
//!
//! ## Result slots
//!
//! | slot | value |
//! |---|---|
//! | 1 | `EXCCAUSE` |
//! | 2 | `EXCVADDR` |
//! | 3 | `EPC1` (the host resolves it to a symbol) |
//! | 4 | the address the fixture faulted on |
//! | 5 | how many times the handler ran |

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

use mach::{finish, inst_len, record};
use xtensa_lx_rt::exception::{Context, ExceptionCause};

/// Unmapped, and nowhere near the one modeled region
/// (`0x3FC8_8000..0x3FCB_0000` and its I-bus alias). Word-aligned, so an
/// alignment fault cannot be mistaken for the mapping fault.
const BAD_ADDRESS: u32 = 0x6000_0000;

static mut HANDLER_RUNS: u32 = 0;

/// The one instruction in this image that faults. `#[inline(never)]` and
/// `#[no_mangle]` so `EPC1` resolves to this name and nothing else.
#[inline(never)]
#[unsafe(no_mangle)]
fn fault_load(address: u32) -> u32 {
    let mut value: u32 = 0;
    // SAFETY: the load is *expected* to fault; the handler resumes past it and
    // leaves `value` at its seed. `inout` rather than `out` so the register has
    // a defined value on the path where the instruction never completes.
    unsafe {
        core::arch::asm!(
            "l32i {v}, {a}, 0",
            v = inout(reg) value,
            a = in(reg) address,
            options(nostack),
        );
    }
    value
}

/// The general-exception handler. `exception.x` PROVIDEs `__exception =
/// __default_exception`, so defining it here takes over — the vector, the
/// context save and the `rfe` are still lx-rt's.
#[unsafe(no_mangle)]
unsafe extern "C" fn __exception(_cause: ExceptionCause, frame: &mut Context) {
    // SAFETY: single-threaded bare-metal fixture, and the handler is not
    // reentrant here (nothing inside it faults).
    unsafe {
        HANDLER_RUNS += 1;
        record(5, HANDLER_RUNS);
    }
    record(1, frame.EXCCAUSE);
    record(2, frame.EXCVADDR);
    record(3, frame.PC);

    // Resume after the faulting instruction. Its first byte is readable: this
    // image's `.text` lives in SRAM1, whose I-bus and D-bus views are the same
    // bytes.
    // SAFETY: `frame.PC` is inside this image's own `.text`.
    let first = unsafe { (frame.PC as *const u8).read_volatile() };
    frame.PC = frame.PC.wrapping_add(inst_len(first));
}

#[xtensa_lx_rt::entry]
fn main() -> ! {
    record(4, BAD_ADDRESS);
    let _ = core::hint::black_box(fault_load(BAD_ADDRESS));
    finish()
}
