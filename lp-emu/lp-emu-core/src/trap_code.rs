//! Arch-neutral trap codes for emulated guest code.
//!
//! Mirrors the encoding of `cranelift_codegen::ir::TrapCode` (a `NonZeroU8`
//! wrapper): a small set of reserved codes at the high end of the byte space
//! and user-defined codes from 1 up. Emulator frontends that receive trap
//! metadata from cranelift convert at their boundary (see `lp-riscv-emu`),
//! preserving the raw byte so diagnostics and equality checks behave
//! identically.

use core::num::NonZeroU8;

/// A trap code describing the reason for a trap.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct TrapCode(NonZeroU8);

impl TrapCode {
    /// Number of trap codes reserved at the high end of the byte space
    /// (matching cranelift's reserved range).
    const RESERVED: u8 = 5;
    const RESERVED_START: u8 = u8::MAX - Self::RESERVED + 1;

    /// Internal helper to create new reserved trap codes.
    const fn reserved(byte: u8) -> TrapCode {
        if let Some(code) = byte.checked_add(Self::RESERVED_START) {
            if let Some(nz) = NonZeroU8::new(code) {
                return TrapCode(nz);
            }
        }
        panic!("invalid reserved trap code")
    }

    /// The current stack space was exhausted.
    pub const STACK_OVERFLOW: TrapCode = TrapCode::reserved(0);
    /// An integer arithmetic operation caused an overflow.
    pub const INTEGER_OVERFLOW: TrapCode = TrapCode::reserved(1);
    /// An out-of-bounds heap access was detected.
    pub const HEAP_OUT_OF_BOUNDS: TrapCode = TrapCode::reserved(2);
    /// An integer division by zero.
    pub const INTEGER_DIVISION_BY_ZERO: TrapCode = TrapCode::reserved(3);
    /// Failed float-to-int conversion.
    pub const BAD_CONVERSION_TO_INTEGER: TrapCode = TrapCode::reserved(4);

    /// Create a user-defined trap code.
    ///
    /// Returns `None` if `code` is zero or falls in the reserved range.
    pub const fn user(code: u8) -> Option<TrapCode> {
        if code >= Self::RESERVED_START {
            return None;
        }
        match NonZeroU8::new(code) {
            Some(nz) => Some(TrapCode(nz)),
            None => None,
        }
    }

    /// Alias for [`TrapCode::user`] with a panic built-in.
    pub const fn unwrap_user(code: u8) -> TrapCode {
        match TrapCode::user(code) {
            Some(code) => code,
            None => panic!("invalid user trap code"),
        }
    }

    /// Returns the raw byte representing this trap.
    pub const fn as_raw(&self) -> NonZeroU8 {
        self.0
    }

    /// Creates a trap code from its raw byte, likely returned by
    /// [`TrapCode::as_raw`] previously.
    pub const fn from_raw(byte: NonZeroU8) -> TrapCode {
        TrapCode(byte)
    }
}

/// Convert a TrapCode to a human-readable string.
pub fn trap_code_to_string(code: TrapCode) -> &'static str {
    match code {
        TrapCode::STACK_OVERFLOW => "stack overflow",
        TrapCode::INTEGER_OVERFLOW => "integer overflow",
        TrapCode::HEAP_OUT_OF_BOUNDS => "heap out of bounds",
        TrapCode::INTEGER_DIVISION_BY_ZERO => "integer division by zero",
        TrapCode::BAD_CONVERSION_TO_INTEGER => "bad conversion to integer",
        _ => {
            // Check for user-defined trap codes
            let raw = code.as_raw().get();
            if raw == 1 {
                "vector/matrix index out of bounds"
            } else {
                "unknown trap"
            }
        }
    }
}
