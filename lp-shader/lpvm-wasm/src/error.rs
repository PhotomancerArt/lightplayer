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
    /// Guest wrote a trap code to the vmctx trap slot during a call
    /// (e.g. [`lpvm::TRAP_CODE_OUT_OF_FUEL`]). Return values from the
    /// trapped call are garbage and have been discarded.
    Trap {
        /// Trap code read from the vmctx trap slot (`lpvm::TRAP_CODE_*`).
        code: u32,
        /// Invocation index (vmctx fuel high word) at the time of the trap:
        /// linear pixel/sample index written by the render wrappers, or
        /// [`lpvm::INVOCATION_INDEX_ARMED`] when the trap occurred outside a
        /// per-invocation wrapper.
        invocation: u32,
    },
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

/// The wasm backends implement the vmctx trap contract: emitted fuel checks
/// write the trap code to the vmctx trap slot, hosts read the slot after
/// every call and surface [`WasmError::Trap`] (the former "wasmtime meters
/// its own store fuel" divergence from the lpvm-native fuel ADR is resolved
/// — see the sim-fuel plan).
impl lpvm::GuestTrapError for WasmError {
    fn guest_trap(&self) -> Option<lpvm::GuestTrap> {
        match self {
            WasmError::Trap { code, invocation } => Some(lpvm::GuestTrap {
                code: *code,
                invocation: *invocation,
            }),
            _ => None,
        }
    }
}

impl fmt::Display for WasmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MetadataMismatch(s) => write!(f, "{s}"),
            Self::Emit(s) => write!(f, "{s}"),
            Self::Runtime(s) => write!(f, "{s}"),
            // The out-of-fuel message must contain both "trap" and "fuel
            // exhausted" (filetest trap classification sniffs strings; typed
            // callers match on the variant instead).
            Self::Trap { code, invocation } => {
                if *code == lpvm::TRAP_CODE_OUT_OF_FUEL {
                    write!(f, "wasm trap: fuel exhausted (invocation {invocation})")
                } else {
                    write!(f, "wasm trap: code {code} (invocation {invocation})")
                }
            }
        }
    }
}

impl core::error::Error for WasmError {}
