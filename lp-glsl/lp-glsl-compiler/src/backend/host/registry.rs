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
    Log,
}

impl HostId {
    /// Get the symbol name for this host function.
    pub fn name(&self) -> &'static str {
        match self {
            HostId::Log => "__host_log",
        }
    }

    /// Get the Cranelift signature for this host function.
    ///
    /// Host log function takes: level (u8 as i32), module_path (ptr, len), message (ptr, len).
    /// On RISC-V 32-bit, this is represented as five i32 parameters.
    pub fn signature(&self) -> Signature {
        let mut sig = Signature::new(CallConv::SystemV);
        // level (u8 as i32)
        sig.params.push(AbiParam::new(types::I32)); // level
        // module_path (pointer, length)
        sig.params.push(AbiParam::new(types::I32)); // module_path pointer
        sig.params.push(AbiParam::new(types::I32)); // module_path length
        // message (pointer, length)
        sig.params.push(AbiParam::new(types::I32)); // message pointer
        sig.params.push(AbiParam::new(types::I32)); // message length
        // No return value
        sig
    }

    /// Get all host IDs.
    pub fn all() -> &'static [HostId] {
        &[HostId::Log]
    }
}

/// Get function pointer for a host function (JIT mode only).
///
/// Returns the function pointer that can be registered with JITModule.
#[cfg(feature = "std")]
pub fn get_host_function_pointer(host: HostId) -> Option<*const u8> {
    use crate::backend::host::impls;

    match host {
        HostId::Log => Some(impls::__host_log as *const u8),
    }
}

/// Get function pointer for a host function (no_std mode).
///
/// Returns pointers to extern functions that must be provided by the user.
/// Users must define `lp_jit_host_log` with signature:
/// `extern "C" fn(level: u8, module_path_ptr: *const u8, module_path_len: usize, msg_ptr: *const u8, msg_len: usize)`
#[cfg(not(feature = "std"))]
pub fn get_host_function_pointer(host: HostId) -> Option<*const u8> {
    use crate::backend::host::lp_jit_host_log;

    match host {
        HostId::Log => {
            let ptr = lp_jit_host_log as *const u8;
            // Safety: We're just getting the address, not calling it
            // The function must be defined by the user in their binary
            Some(ptr)
        }
    }
}

/// Declare host functions as external symbols.
///
/// Host log function takes: level (i32), module_path (ptr: i32, len: i32), message (ptr: i32, len: i32).
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
