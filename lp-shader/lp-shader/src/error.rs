//! Errors from the lp-shader compilation and rendering pipeline.

use alloc::string::String;
use core::fmt;

/// Errors from the lp-shader compilation and rendering pipeline.
#[derive(Debug)]
pub enum LpsError {
    /// GLSL parse failure (naga frontend).
    Parse(String),
    /// LPIR lowering failure.
    Lower(String),
    /// Backend compilation failure.
    Compile(String),
    /// Render-time failure (trap, type mismatch, etc.).
    Render(String),
    /// Pixel shader contract validation failure (e.g. missing `render`,
    /// wrong signature, return type mismatch with output format).
    Validation(String),
}

impl fmt::Display for LpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LpsError::Parse(msg) => write!(f, "parse: {msg}"),
            LpsError::Lower(msg) => write!(f, "lower: {msg}"),
            LpsError::Compile(msg) => write!(f, "compile: {msg}"),
            LpsError::Render(msg) => write!(f, "render: {msg}"),
            LpsError::Validation(msg) => write!(f, "validation: {msg}"),
        }
    }
}

impl core::error::Error for LpsError {}
