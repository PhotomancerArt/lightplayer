//! RV32 ISA-specific code: encoding, GPR, ABI, allocation, PInst emission.

pub mod abi;
pub mod alloc;
pub mod debug;
pub mod encode;
pub mod gpr;
pub mod inst;
pub mod rv32_emit;

use ::alloc::vec::Vec;

use lpir::{FloatMode, IrFunction, LpirModule};
use lps_shared::LpsFnSig;

use crate::abi::ModuleAbi;
use crate::error::NativeError;

/// Lower, fast-allocate, and emit one function to raw RISC-V bytes (no ELF).
pub fn emit_function_fastalloc_bytes(
    func: &IrFunction,
    ir: &LpirModule,
    module_abi: &ModuleAbi,
    fn_sig: &LpsFnSig,
    float_mode: FloatMode,
) -> Result<Vec<u8>, NativeError> {
    let mut lowered = crate::lower::lower_ops(func, ir, module_abi, float_mode)?;
    crate::peephole::optimize(&mut lowered.vinsts);
    let func_abi = crate::rv32::abi::func_abi_rv32(fn_sig, func.total_param_slots() as usize);
    let phys = alloc::allocate(&lowered.vinsts, &func_abi, func, &lowered.vreg_pool)
        .map_err(NativeError::FastAlloc)?;
    let mut emitter = rv32_emit::Rv32Emitter::new();
    for p in &phys {
        emitter.emit(p);
    }
    Ok(emitter.finish())
}
