//! Shared emission orchestrator: VInst → bytes via allocator + emitter.

use alloc::vec::Vec;

use crate::abi::FrameLayout;
use crate::compile::NativeReloc;
use crate::error::NativeError;
use crate::fa_alloc::{AllocOutput, allocate};
use crate::rv32::emit::{EmittedCode as Rv32EmittedCode, emit_function};
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
}

impl From<Rv32EmittedCode> for EmittedCode {
    fn from(code: Rv32EmittedCode) -> Self {
        Self {
            code: code.code,
            relocs: code
                .relocs
                .into_iter()
                .map(|r| NativeReloc {
                    offset: r.offset,
                    symbol: r.symbol,
                })
                .collect(),
            debug_lines: code.debug_lines,
        }
    }
}

/// Emit a LoweredFunction to machine code.
///
/// This function orchestrates the allocation and emission pipeline:
/// 1. Allocate registers (VInst → AllocOutput) via fa_alloc
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
    // 1. Allocate registers: VInst → AllocOutput
    let alloc_result = allocate(lowered, func_abi).map_err(NativeError::FastAlloc)?;

    // 2. Build frame layout using actual spill slots from allocator
    let frame = FrameLayout::compute(
        func_abi,
        alloc_result.spill_slots,
        crate::abi::PregSet::EMPTY,
        &[],
        false, // is_leaf: false = save RA (conservative)
        0,
        0,
    );

    // 3. Emit: VInst + AllocOutput → bytes
    let emitted = emit_function(
        &lowered.vinsts,
        &lowered.vreg_pool,
        &alloc_result.output,
        frame,
        &lowered.symbols,
        func_abi.is_sret(),
    )
    .map_err(NativeError::FastAlloc)?;

    Ok(emitted.into())
}

/// Emit a sequence of VInsts to machine code.
///
/// This function is DEPRECATED - use `emit_lowered` instead.
/// It constructs a minimal LoweredFunction wrapper for the given VInsts.
pub fn emit_vinsts(
    vinsts: &[VInst],
    func_abi: &crate::abi::FuncAbi,
    _func: &lpir::IrFunction,
    vreg_pool: &[crate::vinst::VReg],
) -> Result<EmittedCode, NativeError> {
    // Build a minimal LoweredFunction for the new allocator
    let mut lowered = crate::lower::LoweredFunction {
        vinsts: vinsts.to_vec(),
        vreg_pool: vreg_pool.to_vec(),
        symbols: crate::vinst::ModuleSymbols::default(),
        loop_regions: Vec::new(),
        region_tree: crate::region::RegionTree::new(),
    };

    // Build a Linear region covering all instructions
    if !vinsts.is_empty() {
        let root = lowered.region_tree.push(crate::region::Region::Linear {
            start: 0,
            end: vinsts.len() as u16,
        });
        lowered.region_tree.root = root;
    }

    emit_lowered(&lowered, func_abi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fa_alloc::AllocError;

    #[test]
    fn emit_lowered_returns_not_implemented() {
        // M1: allocator returns NotImplemented
        let vinsts = vec![];
        let vreg_pool = vec![];
        let mut lowered = crate::lower::LoweredFunction {
            vinsts,
            vreg_pool,
            symbols: crate::vinst::ModuleSymbols::default(),
            loop_regions: Vec::new(),
            region_tree: crate::region::RegionTree::new(),
        };

        let abi = crate::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: alloc::string::String::from("test"),
                return_type: lps_shared::LpsType::Void,
                parameters: vec![],
            },
            0,
        );

        let result = emit_lowered(&lowered, &abi);
        assert!(matches!(
            result,
            Err(NativeError::FastAlloc(AllocError::NotImplemented))
        ));
    }
}
