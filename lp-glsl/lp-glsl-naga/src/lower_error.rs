//! Errors from Naga → LPIR lowering.

use alloc::string::String;
use core::fmt;

#[derive(Debug)]
pub enum LowerError {
    UnsupportedExpression(String),
    UnsupportedStatement(String),
    UnsupportedType(String),
    Internal(String),
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LowerError::UnsupportedExpression(s) => write!(f, "unsupported expression: {s}"),
            LowerError::UnsupportedStatement(s) => write!(f, "unsupported statement: {s}"),
            LowerError::UnsupportedType(s) => write!(f, "unsupported type: {s}"),
            LowerError::Internal(s) => write!(f, "internal lowering error: {s}"),
        }
    }
}

impl core::error::Error for LowerError {}
