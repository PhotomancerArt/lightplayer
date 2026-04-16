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
    SpilledVReg(u32),
    UnresolvedLabel(u32),
    DuplicateLabel(u32),
    BranchOffsetOutOfRange,
    MissingSretSlot,
    /// LPIR `slot_addr` referenced a slot missing from the frame layout.
    InvalidLpirSlot(u32),
    ObjectWrite(String),
    #[cfg(feature = "emu")]
    Link(lpvm_cranelift::CompilerError),
    #[cfg(any(feature = "emu", target_arch = "riscv32"))]
    Call(lpvm::CallError),
    #[cfg(any(feature = "emu", target_arch = "riscv32"))]
    Alloc(String),
    /// JIT relocation or symbol resolution failure (RISC-V firmware path).
    #[cfg(target_arch = "riscv32")]
    JitLink(String),
    /// Backward-walk register allocation failed.
    RegAlloc(crate::regalloc::AllocError),
    /// Internal error (e.g., during restructuring).
    Internal(String),
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
            NativeError::SpilledVReg(v) => {
                write!(f, "spilled vreg {v} (no physical register assigned)")
            }
            NativeError::UnresolvedLabel(id) => {
                write!(f, "unresolved control-flow label {id}")
            }
            NativeError::DuplicateLabel(id) => {
                write!(f, "duplicate control-flow label {id}")
            }
            NativeError::BranchOffsetOutOfRange => {
                write!(f, "branch/jal target out of RV32 immediate range")
            }
            NativeError::RegAlloc(e) => write!(f, "regalloc: {e}"),
            NativeError::Internal(s) => write!(f, "internal: {s}"),
            NativeError::MissingSretSlot => {
                write!(
                    f,
                    "sret call requires caller sret stack slot but frame has none"
                )
            }
            NativeError::InvalidLpirSlot(id) => {
                write!(f, "LPIR stack slot ss{id} has no frame offset")
            }
            NativeError::ObjectWrite(s) => write!(f, "ELF write error: {s}"),
            #[cfg(feature = "emu")]
            NativeError::Link(e) => write!(f, "link: {e}"),
            #[cfg(any(feature = "emu", target_arch = "riscv32"))]
            NativeError::Call(e) => write!(f, "{e}"),
            #[cfg(any(feature = "emu", target_arch = "riscv32"))]
            NativeError::Alloc(s) => write!(f, "allocation error: {s}"),
            #[cfg(target_arch = "riscv32")]
            NativeError::JitLink(s) => write!(f, "JIT link: {s}"),
        }
    }
}

impl core::error::Error for NativeError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            NativeError::Lower(e) => Some(e),
            NativeError::RegAlloc(e) => Some(e),
            #[cfg(feature = "emu")]
            NativeError::Link(e) => Some(e),
            #[cfg(any(feature = "emu", target_arch = "riscv32"))]
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

#[cfg(any(feature = "emu", target_arch = "riscv32"))]
impl From<lpvm::CallError> for NativeError {
    fn from(e: lpvm::CallError) -> Self {
        NativeError::Call(e)
    }
}
