//! Exception causes, the vector table, and the core-configuration constants.
//!
//! Cause numbers are the Xtensa ISA Reference Manual's (§4.4.1.2, Table 4-64
//! and the option tables that extend it). The *process* — what is written
//! where on entry, and what `rfe`/`rfi`/`rfde` undo — is implemented on the
//! hart in `mod.rs`, next to the state it touches; this file is the numbers.
//!
//! # Provenance of the core-configuration constants
//!
//! The vector offsets and the `NUM_*` / level constants are Cadence
//! `XCHAL_*` core-configuration parameters. They are copied from
//! `xtensa-lx-rt` 0.22.0's generated `config/esp32.rs` and
//! `config/esp32s3.rs` (crates.io, MIT/Apache-2.0), on which the two files
//! agree on every value below, and independently confirmed by that crate's
//! linker template `exception-esp32.x.template:55-93`, which lays the vector
//! sections out at exactly these addresses. `lp-xt-emu` must not depend on
//! `xtensa-lx-rt` (it is a device crate; the fence forbids it), so the
//! values are copied with this citation rather than imported.

/// The synchronous exception causes this hart raises (`EXCCAUSE`).
pub mod cause {
    pub use crate::error::{
        EXC_COPROCESSOR0_DISABLED as COPROCESSOR0_DISABLED,
        EXC_ILLEGAL_INSTRUCTION as ILLEGAL_INSTRUCTION,
        EXC_INTEGER_DIVIDE_BY_ZERO as INTEGER_DIVIDE_BY_ZERO,
        EXC_LOAD_STORE_ALIGNMENT as LOAD_STORE_ALIGNMENT, EXC_SYSCALL as SYSCALL,
    };
    pub use crate::memory::{
        EXC_INSTR_FETCH_ERROR as INSTRUCTION_FETCH_ERROR, EXC_LOAD_STORE_ERROR as LOAD_STORE_ERROR,
    };

    /// `Level1InterruptCause` (RM Table 4-69): a level-1 interrupt has no
    /// vector of its own — it arrives through the user/kernel exception
    /// vector with this cause, which is why "interrupts vector by level" is
    /// wrong at level 1 and right at 2..7.
    pub const LEVEL1_INTERRUPT: u32 = 4;
    /// `AllocaCause`: `movsp` with the caller's registers not resident.
    pub const ALLOCA: u32 = 5;
    /// `PrivilegedCause` (RM Table 4-64): a privileged instruction with
    /// `CRING != 0`. **Never raised here**: this hart models no rings
    /// (`PS.RING` is stored and ignored, as on a core without the MMU
    /// Option), so `CRING` is always 0. Named so the omission is visible.
    pub const PRIVILEGED: u32 = 8;
}

/// `DEBUGCAUSE` bits (RM §4.7.6.2, Table 4-123).
pub mod debugcause {
    /// Bit 0: ICOUNT exception (not raised by this hart — ICOUNT is stored,
    /// not counted).
    pub const ICOUNT: u32 = 1 << 0;
    /// Bit 1: instruction breakpoint (`IBREAKA[i]` matched the fetch).
    pub const IBREAK: u32 = 1 << 1;
    /// Bit 2: data breakpoint (a `DBREAK[i]` slot matched a load/store).
    pub const DBREAK: u32 = 1 << 2;
    /// Bit 3: the `break` instruction.
    pub const BREAK: u32 = 1 << 3;
    /// Bit 4: the `break.n` instruction.
    pub const BREAK_N: u32 = 1 << 4;
    /// Bits 11:8: which `DBREAK` slot matched.
    pub const DBNUM_SHIFT: u32 = 8;
}

// --- vector offsets from VECBASE (XCHAL_*_VECOFS; see the module doc) ---

pub const VECOFS_WINDOW_OF4: u32 = 0x000;
pub const VECOFS_WINDOW_UF4: u32 = 0x040;
pub const VECOFS_WINDOW_OF8: u32 = 0x080;
pub const VECOFS_WINDOW_UF8: u32 = 0x0C0;
pub const VECOFS_WINDOW_OF12: u32 = 0x100;
pub const VECOFS_WINDOW_UF12: u32 = 0x140;
/// `_Level2InterruptVector`; levels 3..7 follow at +0x40 each (see
/// [`level_vecofs`]).
pub const VECOFS_LEVEL2: u32 = 0x180;
pub const VECOFS_LEVEL3: u32 = 0x1C0;
pub const VECOFS_LEVEL4: u32 = 0x200;
pub const VECOFS_LEVEL5: u32 = 0x240;
/// Level 6 is `DEBUGLEVEL`: the debug-exception vector.
pub const VECOFS_LEVEL6_DEBUG: u32 = 0x280;
/// Level 7 is the NMI.
pub const VECOFS_LEVEL7_NMI: u32 = 0x2C0;
pub const VECOFS_KERNEL: u32 = 0x300;
pub const VECOFS_USER: u32 = 0x340;
pub const VECOFS_DOUBLE: u32 = 0x3C0;
/// The dynamic vector group is 1 KiB from VECBASE.
pub const VECTOR_TABLE_SIZE: u32 = 0x400;

/// The interrupt vector offset for level 2..=7.
#[inline]
#[must_use]
pub const fn level_vecofs(level: u8) -> u32 {
    debug_assert!(level >= 2 && level <= 7);
    VECOFS_LEVEL2 + 0x40 * (level as u32 - 2)
}

/// `_WindowOverflow{4,8,12}` for a call increment of 1, 2, 3.
#[inline]
#[must_use]
pub const fn overflow_vecofs(inc: u8) -> u32 {
    match inc {
        1 => VECOFS_WINDOW_OF4,
        2 => VECOFS_WINDOW_OF8,
        _ => VECOFS_WINDOW_OF12,
    }
}

/// `_WindowUnderflow{4,8,12}` for a call increment of 1, 2, 3.
#[inline]
#[must_use]
pub const fn underflow_vecofs(inc: u8) -> u32 {
    match inc {
        1 => VECOFS_WINDOW_UF4,
        2 => VECOFS_WINDOW_UF8,
        _ => VECOFS_WINDOW_UF12,
    }
}

// --- core configuration (XCHAL_*; see the module doc) ---

/// `XCHAL_NUM_AREGS`.
pub const NUM_AREGS: usize = 64;
/// `XCHAL_NUM_INTERRUPTS`.
pub const NUM_INTERRUPTS: usize = 32;
/// `XCHAL_NUM_INTLEVELS` — the maskable levels 1..=6.
pub const NUM_INTLEVELS: u8 = 6;
/// The NMI sits one above the maskable levels.
pub const NMI_LEVEL: u8 = 7;
/// `XCHAL_EXCM_LEVEL`: while `PS.EXCM = 1`, interrupts at this level and
/// below are masked (`CINTLEVEL = max(PS.INTLEVEL, EXCM ? 3 : 0)`, RM
/// §4.4.1.4).
pub const EXCM_LEVEL: u8 = 3;
/// `XCHAL_DEBUGLEVEL`: the level debug exceptions are taken at.
pub const DEBUGLEVEL: u8 = 6;
/// `XCHAL_NUM_TIMERS`: `CCOMPARE0..2`.
pub const NUM_TIMERS: usize = 3;
/// `XCHAL_NUM_DBREAK`.
pub const NUM_DBREAK: usize = 2;
/// `XCHAL_NUM_IBREAK`.
pub const NUM_IBREAK: usize = 2;

/// `XCHAL_VECBASE_RESET_VADDR` and `XCHAL_RESET_VECTOR_VADDR` on both the
/// LX6 (esp32) and the LX7 (esp32s3). **Not** baked into the hart: the
/// machine passes them through [`super::CoreConfig`]. Here so a test or a
/// fixture can name the real values.
pub const VECBASE_RESET_ESP32: u32 = 0x4000_0000;
pub const RESET_VECTOR_ESP32: u32 = 0x4000_0400;

/// Is `pc` one of the 16 vector entry addresses of the table at `vecbase`?
/// The hart's twin of RV32's "a fault at the vector we would jump to".
#[inline]
#[must_use]
pub fn is_vector_entry(vecbase: u32, pc: u32) -> bool {
    let off = pc.wrapping_sub(vecbase);
    off < VECTOR_TABLE_SIZE && off % 0x40 == 0
}
