//! Errors for WASM emission and runtime execution.

use std::string::String;

use core::fmt;

/// Unified error type for `lpvm-wasm`.
#[derive(Debug)]
pub enum WasmError {
    /// LPIR metadata does not match the IR module (names or function count).
    MetadataMismatch(String),
    /// WASM emission failed.
    Emit(String),
    /// Linking / wasmtime / browser execution failure.
    Runtime(String),
}

impl WasmError {
    pub(crate) fn metadata_mismatch(msg: impl Into<String>) -> Self {
        Self::MetadataMismatch(msg.into())
    }

    pub(crate) fn emit(msg: impl Into<String>) -> Self {
        Self::Emit(msg.into())
    }

    pub(crate) fn runtime(msg: impl Into<String>) -> Self {
        Self::Runtime(msg.into())
    }
}

/// No vmctx-trap contract on the WASM backends: wasmtime meters its own
/// store fuel and reports exhaustion as a plain runtime error (accepted
/// divergence — see the lpvm-native fuel ADR). Default `None` applies.
impl lpvm::GuestTrapError for WasmError {}

impl fmt::Display for WasmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MetadataMismatch(s) => write!(f, "{s}"),
            Self::Emit(s) => write!(f, "{s}"),
            Self::Runtime(s) => write!(f, "{s}"),
        }
    }
}

impl core::error::Error for WasmError {}
