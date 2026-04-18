//! Shared emission orchestrator: VInst → bytes via allocator + emitter.

use alloc::vec::Vec;

use crate::abi::{FrameLayout, FuncAbi, PregSet};
use crate::compile::NativeReloc;
use crate::error::NativeError;
use crate::regalloc::{AllocOutput, AllocResult, allocate};
use crate::isa::rv32::emit::emit_function;
use crate::vinst::VInst;

/// Emission result containing machine code and metadata.
#[derive(Clone, Debug)]
pub struct EmittedCode {
    /// RISC-V machine code bytes.
    pub code: Vec<u8>,
    /// Relocations for auipc+jalr call pairs.
    pub relocs: Vec<NativeReloc>,
    /// Debug line table: (code_offset, optional_src_op).
    pub debug_lines: Vec<(u32, Option<u32>)>,
    /// Allocation output for debug rendering.
    pub alloc_output: AllocOutput,
}

/// Emit a LoweredFunction to machine code.
///
/// This function orchestrates the allocation and emission pipeline:
/// 1. Allocate registers (VInst → AllocOutput) via [`crate::regalloc`]
/// 2. Emit VInst + AllocOutput → bytes via rv32::emit
///
/// # Arguments
/// * `lowered` - Lowered function with vinsts, region tree, vreg pool
/// * `func_abi` - Function ABI for register allocation
///
/// # Returns
/// Emitted machine code with relocations and debug info.
pub fn emit_lowered(
    lowered: &crate::lower::LoweredFunction,
    func_abi: &crate::abi::FuncAbi,
) -> Result<EmittedCode, NativeError> {
    emit_lowered_ex(lowered, func_abi, 0)
}

/// Emit with caller-side sret buffer size.
pub fn emit_lowered_ex(
    lowered: &crate::lower::LoweredFunction,
    func_abi: &crate::abi::FuncAbi,
    caller_sret_bytes: u32,
) -> Result<EmittedCode, NativeError> {
    log::debug!(
        "[native-fa] emit_lowered_ex: starting allocation for {} vinsts",
        lowered.vinsts.len()
    );
    let alloc_result = allocate(lowered, func_abi).map_err(NativeError::RegAlloc)?;
    log::debug!(
        "[native-fa] emit_lowered_ex: allocation complete, {} spill slots",
        alloc_result.spill_slots
    );
    emit_lowered_with_alloc(lowered, func_abi, alloc_result, caller_sret_bytes)
}

/// Emit using an existing [`AllocResult`] (avoids running the allocator twice).
pub fn emit_lowered_with_alloc(
    lowered: &crate::lower::LoweredFunction,
    func_abi: &crate::abi::FuncAbi,
    alloc_result: AllocResult,
    caller_sret_bytes: u32,
) -> Result<EmittedCode, NativeError> {
    let mut used_callee_saved = alloc_result.used_callee_saved;
    if let Some(p) = func_abi.sret_preservation_reg() {
        // sret functions preserve the designated GPR in the prologue; it must be
        // saved/restored even when the allocator never assigns it.
        used_callee_saved = used_callee_saved.union(PregSet::singleton(p));
    }
    let caller_outgoing_stack_bytes = max_outgoing_stack_bytes(&lowered.vinsts, func_abi);
    let is_leaf = !contains_call(&lowered.vinsts);
    let frame = FrameLayout::compute(
        func_abi,
        alloc_result.spill_slots,
        used_callee_saved,
        &lowered.lpir_slots,
        is_leaf,
        caller_sret_bytes,
        caller_outgoing_stack_bytes,
    );

    let emitted = emit_function(
        &lowered.vinsts,
        &lowered.vreg_pool,
        &alloc_result.output,
        frame,
        &lowered.symbols,
        func_abi.is_sret(),
    )
    .map_err(NativeError::RegAlloc)?;

    // Build EmittedCode with allocation output for debug rendering
    Ok(EmittedCode {
        code: emitted.code,
        relocs: emitted
            .relocs
            .into_iter()
            .map(|r| NativeReloc {
                offset: r.offset,
                symbol: r.symbol,
                r_type: crate::isa::rv32::link::R_RISCV_CALL_PLT,
            })
            .collect(),
        debug_lines: emitted.debug_lines,
        alloc_output: alloc_result.output,
    })
}

/// Returns true if the function contains any call instructions.
fn contains_call(vinsts: &[VInst]) -> bool {
    vinsts.iter().any(|inst| matches!(inst, VInst::Call { .. }))
}

/// Max bytes needed at `[SP+0]` for outgoing stack-passed call arguments.
pub fn max_outgoing_stack_bytes(vinsts: &[VInst], func_abi: &FuncAbi) -> u32 {
    let arg_regs = func_abi.arg_regs();
    let mut max_bytes = 0u32;
    for inst in vinsts {
        if let VInst::Call {
            args,
            callee_uses_sret,
            ..
        } = inst
        {
            let cap = if *callee_uses_sret {
                arg_regs.len() - 1
            } else {
                arg_regs.len()
            };
            let n = args.len();
            if n > cap {
                let stack_words = (n - cap) as u32;
                max_bytes = max_bytes.max(stack_words * 4);
            }
        }
    }
    max_bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_lowered_returns_success() {
        // After M1-M3: allocator should succeed for simple cases
        let vinsts = vec![];
        let vreg_pool = vec![];
        let mut lowered = crate::lower::LoweredFunction {
            vinsts,
            vreg_pool,
            symbols: crate::vinst::ModuleSymbols::default(),
            loop_regions: Vec::new(),
            region_tree: crate::region::RegionTree::new(),
            lpir_slots: Vec::new(),
        };

        // Set up a proper Linear region (required by allocator)
        let root = lowered
            .region_tree
            .push(crate::region::Region::Linear { start: 0, end: 0 });
        lowered.region_tree.root = root;

        let abi = crate::isa::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: alloc::string::String::from("test"),
                return_type: lps_shared::LpsType::Void,
                parameters: vec![],
            },
            0,
        );

        let result = emit_lowered(&lowered, &abi);
        assert!(result.is_ok(), "emit_lowered should succeed: {result:?}");
    }
}
