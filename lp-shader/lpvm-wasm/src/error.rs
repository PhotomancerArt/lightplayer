//! Errors for emission and, when the `runtime` feature is enabled (default), execution.

use alloc::string::String;
use core::fmt;

/// Unified error type for `lpvm-wasm`.
#[derive(Debug)]
pub enum WasmError {
    /// LPIR metadata does not match the IR module (names or function count).
    MetadataMismatch(String),
    /// WASM emission failed.
    Emit(String),
    /// wasmtime / linking / execution failure (present when `runtime` is enabled).
    #[cfg(feature = "runtime")]
    Runtime(String),
}

impl WasmError {
    pub(crate) fn metadata_mismatch(msg: impl Into<String>) -> Self {
        Self::MetadataMismatch(msg.into())
    }

    pub(crate) fn emit(msg: impl Into<String>) -> Self {
        Self::Emit(msg.into())
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn runtime(msg: impl Into<String>) -> Self {
        Self::Runtime(msg.into())
    }
}

impl fmt::Display for WasmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MetadataMismatch(s) => write!(f, "{s}"),
            Self::Emit(s) => write!(f, "{s}"),
            #[cfg(feature = "runtime")]
            Self::Runtime(s) => write!(f, "{s}"),
        }
    }
}

impl core::error::Error for WasmError {}
