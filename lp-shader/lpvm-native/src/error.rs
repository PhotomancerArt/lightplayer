//! Errors for the native backend (lowering, compile, emulation).

use alloc::string::String;
use core::fmt;

/// Lowering failed: opcode not implemented.
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
    Lower(LowerError),
    EmptyModule,
    TooManyVRegs {
        count: usize,
        max: usize,
    },
    TooManyArgs(usize),
    TooManyReturns(usize),
    UnassignedVReg(u32),
    ObjectWrite(String),
    #[cfg(feature = "emu")]
    Link(lpvm_cranelift::CompilerError),
    #[cfg(feature = "emu")]
    Call(lpvm::CallError),
    #[cfg(feature = "emu")]
    Alloc(String),
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NativeError::Lower(e) => write!(f, "{e}"),
            NativeError::EmptyModule => write!(f, "empty LPIR module"),
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
            #[cfg(feature = "emu")]
            NativeError::Link(e) => write!(f, "link: {e}"),
            #[cfg(feature = "emu")]
            NativeError::Call(e) => write!(f, "{e}"),
            #[cfg(feature = "emu")]
            NativeError::Alloc(s) => write!(f, "allocation error: {s}"),
        }
    }
}

impl core::error::Error for NativeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            NativeError::Lower(e) => Some(e),
            #[cfg(feature = "emu")]
            NativeError::Link(e) => Some(e),
            #[cfg(feature = "emu")]
            NativeError::Call(e) => Some(e),
            _ => None,
        }
    }
}

impl From<LowerError> for NativeError {
    fn from(e: LowerError) -> Self {
        NativeError::Lower(e)
    }
}

#[cfg(feature = "emu")]
impl From<lpvm_cranelift::CompilerError> for NativeError {
    fn from(e: lpvm_cranelift::CompilerError) -> Self {
        NativeError::Link(e)
    }
}

#[cfg(feature = "emu")]
impl From<lpvm::CallError> for NativeError {
    fn from(e: lpvm::CallError) -> Self {
        NativeError::Call(e)
    }
}
