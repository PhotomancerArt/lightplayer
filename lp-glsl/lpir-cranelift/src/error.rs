//! Compilation errors for LPIR → Cranelift translation.

use alloc::string::String;

/// Failure during JIT compilation from LPIR.
#[derive(Debug)]
pub enum CompileError {
    /// LPIR construct not yet supported by this stage of the emitter.
    Unsupported(String),
    /// Error from Cranelift module or codegen.
    Cranelift(String),
}

impl CompileError {
    pub fn unsupported(msg: impl Into<String>) -> Self {
        CompileError::Unsupported(msg.into())
    }

    pub fn cranelift(msg: impl Into<String>) -> Self {
        CompileError::Cranelift(msg.into())
    }
}

impl core::fmt::Display for CompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CompileError::Unsupported(s) => write!(f, "unsupported LPIR: {s}"),
            CompileError::Cranelift(s) => write!(f, "cranelift: {s}"),
        }
    }
}

impl std::error::Error for CompileError {}
