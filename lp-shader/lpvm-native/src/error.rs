//! Errors for the native backend (lowering, compile stubs).

use alloc::string::String;
use core::fmt;

/// Lowering failed: opcode not implemented in M1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerError {
    UnsupportedOp { description: String },
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LowerError::UnsupportedOp { description } => {
                write!(f, "unsupported LPIR op: {description}")
            }
        }
    }
}

impl core::error::Error for LowerError {}

/// Engine / module / instance errors.
#[derive(Debug)]
pub enum NativeError {
    NotYetImplemented(String),
    Lower(LowerError),
    EmptyModule,
    ImportsNotSupportedYet,
    TooManyVRegs { count: usize, max: usize },
    TooManyArgs(usize),
    TooManyReturns(usize),
    UnassignedVReg(u32),
    ObjectWrite(String),
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NativeError::NotYetImplemented(s) => write!(f, "not yet implemented: {s}"),
            NativeError::Lower(e) => write!(f, "{e}"),
            NativeError::EmptyModule => write!(f, "empty LPIR module"),
            NativeError::ImportsNotSupportedYet => {
                write!(f, "lpvm-native M2: imports are not supported yet")
            }
            NativeError::TooManyVRegs { count, max } => {
                write!(
                    f,
                    "too many vregs for greedy allocator: {count} (max {max})"
                )
            }
            NativeError::TooManyArgs(n) => write!(f, "too many arguments for RV32 ABI: {n}"),
            NativeError::TooManyReturns(n) => write!(f, "too many return values: {n}"),
            NativeError::UnassignedVReg(v) => write!(f, "unassigned vreg {v}"),
            NativeError::ObjectWrite(s) => write!(f, "ELF write error: {s}"),
        }
    }
}

impl core::error::Error for NativeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            NativeError::Lower(e) => Some(e),
            _ => None,
        }
    }
}

impl From<LowerError> for NativeError {
    fn from(e: LowerError) -> Self {
        NativeError::Lower(e)
    }
}
