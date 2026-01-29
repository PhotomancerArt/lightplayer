//! Host function registry implementation.
//!
//! Provides enum-based registry for host functions with support for JIT linking.

use crate::error::{ErrorCode, GlslError};
use alloc::format;
use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{Linkage, Module};

/// Enum identifying host functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostId {
    Debug,
    Println,
}

impl HostId {
    /// Get the symbol name for this host function.
    pub fn name(&self) -> &'static str {
        match self {
            HostId::Debug => "__host_debug",
            HostId::Println => "__host_println",
        }
    }

    /// Get the Cranelift signature for this host function.
    ///
    /// Host functions take a `&str` parameter (pointer + length).
    /// On RISC-V 32-bit, this is represented as two i32 parameters (pointer, length).
    pub fn signature(&self) -> Signature {
        let mut sig = Signature::new(CallConv::SystemV);
        // &str is represented as (pointer: i32, length: i32) on RISC-V 32-bit
        sig.params.push(AbiParam::new(types::I32)); // pointer
        sig.params.push(AbiParam::new(types::I32)); // length
        // No return value
        sig
    }

    /// Get all host IDs.
    pub fn all() -> &'static [HostId] {
        &[HostId::Debug, HostId::Println]
    }
}

/// Get function pointer for a host function (JIT mode only).
///
/// Returns the function pointer that can be registered with JITModule.
#[cfg(feature = "std")]
pub fn get_host_function_pointer(host: HostId) -> Option<*const u8> {
    use crate::backend::host::impls;

    match host {
        HostId::Debug => Some(impls::__host_debug as *const u8),
        HostId::Println => Some(impls::__host_println as *const u8),
    }
}

/// Get function pointer for a host function (no_std mode).
///
/// Returns pointers to extern functions that must be provided by the user.
/// Users must define `lp_jit_host_debug` and `lp_jit_host_println` with signature:
/// `extern "C" fn(ptr: *const u8, len: usize)`
#[cfg(not(feature = "std"))]
pub fn get_host_function_pointer(host: HostId) -> Option<*const u8> {
    use crate::backend::host::{lp_jit_host_debug, lp_jit_host_println};

    match host {
        HostId::Debug => Some(lp_jit_host_debug as *const u8),
        HostId::Println => Some(lp_jit_host_println as *const u8),
    }
}

/// Declare host functions as external symbols.
///
/// Host functions take (pointer: i32, length: i32) parameters representing a string.
pub fn declare_host_functions<M: Module>(module: &mut M) -> Result<(), GlslError> {
    for host in HostId::all() {
        let name = host.name();
        let sig = host.signature();

        module
            .declare_function(name, Linkage::Import, &sig)
            .map_err(|e| {
                GlslError::new(
                    ErrorCode::E0400,
                    format!("Failed to declare host function '{name}': {e}"),
                )
            })?;
    }

    Ok(())
}
