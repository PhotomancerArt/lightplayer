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

#[cfg(feature = "std")]
impl std::error::Error for CompileError {}

/// Full compiler pipeline errors (parse, lowering, codegen).
#[derive(Debug)]
pub enum CompilerError {
    /// GLSL parse / naga frontend (line-oriented message).
    Parse(String),
    /// Naga → LPIR lowering (only when the `std` feature enables `lp-glsl-naga`).
    #[cfg(feature = "std")]
    Lower(lp_glsl_naga::LowerError),
    /// LPIR → machine code.
    Codegen(CompileError),
}

impl core::fmt::Display for CompilerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CompilerError::Parse(s) => write!(f, "{s}"),
            #[cfg(feature = "std")]
            CompilerError::Lower(e) => write!(f, "{e}"),
            CompilerError::Codegen(e) => write!(f, "{e}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CompilerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CompilerError::Codegen(e) => Some(e),
            _ => None,
        }
    }
}

impl From<CompileError> for CompilerError {
    fn from(value: CompileError) -> Self {
        CompilerError::Codegen(value)
    }
}
