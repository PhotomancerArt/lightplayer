//! Errors from Naga → LPIR lowering.

use alloc::boxed::Box;
use alloc::string::String;
use core::fmt;

#[derive(Debug)]
pub enum LowerError {
    /// Naga [`naga::Expression`] form is not implemented for scalar lowering (detail string).
    UnsupportedExpression(String),
    /// Naga [`naga::Statement`] form is not implemented (detail string).
    UnsupportedStatement(String),
    /// Type or signature is outside the scalar lowering subset (detail string).
    UnsupportedType(String),
    /// Invariant violated or missing internal mapping (detail string).
    Internal(String),
    /// Failure while lowering a specific GLSL user function.
    InFunction {
        name: String,
        inner: Box<LowerError>,
    },
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LowerError::UnsupportedExpression(s) => write!(f, "unsupported expression: {s}"),
            LowerError::UnsupportedStatement(s) => write!(f, "unsupported statement: {s}"),
            LowerError::UnsupportedType(s) => write!(f, "unsupported type: {s}"),
            LowerError::Internal(s) => write!(f, "internal lowering error: {s}"),
            LowerError::InFunction { name, inner } => {
                write!(f, "in function '{name}': {inner}")
            }
        }
    }
}

impl core::error::Error for LowerError {}
