//! RV32 ISA-specific code: encoding, GPR, ABI, emission.

pub mod abi;
pub mod debug;
pub mod emit;
pub mod encode;
pub mod gpr;

use ::alloc::vec::Vec;

use lpir::{FloatMode, IrFunction, LpirModule};
use lps_shared::LpsFnSig;

use crate::abi::ModuleAbi;
use crate::error::NativeError;

// Re-exports from emit module
pub use emit::{EmittedCode, NativeReloc, emit_function};

/// Lower, fast-allocate, and emit one function to raw RISC-V bytes (no ELF).
/// TODO(M2): Update this to use the new allocator and emitter.
pub fn emit_function_fastalloc_bytes(
    _func: &IrFunction,
    _ir: &LpirModule,
    _module_abi: &ModuleAbi,
    _fn_sig: &LpsFnSig,
    _float_mode: FloatMode,
) -> Result<Vec<u8>, NativeError> {
    // TODO(M2): Implement using new allocator and emitter
    Err(NativeError::FastAlloc(crate::emit_err!()))
}
