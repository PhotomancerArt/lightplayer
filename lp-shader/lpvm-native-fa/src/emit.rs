//! Shared emission orchestrator: VInst → bytes via alloc + rv32_emit.

use alloc::vec::Vec;

use crate::compile::NativeReloc;
use crate::error::NativeError;
use crate::rv32::inst::PInst;
use crate::rv32::rv32_emit::Rv32Emitter;
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

/// Emit a sequence of VInsts to machine code.
///
/// This function orchestrates the allocation and emission pipeline:
/// 1. Allocate registers (VInst → PInst)
/// 2. Encode PInst to bytes
///
/// # Arguments
/// * `vinsts` - Virtual instructions to emit
/// * `func_abi` - Function ABI for register allocation
/// * `vreg_pool` - Pool for VRegSlice resolution
///
/// # Returns
/// Emitted machine code with relocations and debug info.
pub fn emit_vinsts(
    vinsts: &[VInst],
    func_abi: &crate::abi::FuncAbi,
    func: &lpir::IrFunction,
    vreg_pool: &[crate::vinst::VReg],
) -> Result<EmittedCode, NativeError> {
    // 1. Allocate registers: VInst → PInst
    let pinsts = crate::rv32::alloc::allocate(vinsts, func_abi, func, vreg_pool)
        .map_err(NativeError::FastAlloc)?;

    // 2. Emit PInst → bytes
    emit_pinsts(&pinsts)
}

/// Emit pre-allocated physical instructions to machine code.
///
/// This is a lower-level entry point for when you already have PInsts
/// (e.g., from a custom allocator or for testing).
///
/// # Arguments
/// * `pinsts` - Physical instructions to encode
///
/// # Returns
/// Emitted machine code with relocations and debug info.
pub fn emit_pinsts(pinsts: &[PInst]) -> Result<EmittedCode, NativeError> {
    let mut emitter = Rv32Emitter::new();

    for inst in pinsts {
        emitter.emit(inst);
    }

    let (code, phys_relocs) = emitter.finish_with_relocs();

    // Convert PhysReloc → NativeReloc
    let relocs = phys_relocs
        .into_iter()
        .map(|r| NativeReloc {
            offset: r.offset,
            symbol: r.symbol,
        })
        .collect();

    Ok(EmittedCode {
        code,
        relocs,
        debug_lines: Vec::new(), // TODO: wire up debug_lines from src_op mapping
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rv32::inst::{PInst, SymbolRef};
    use crate::rv32::gpr;

    #[test]
    fn test_emit_pinsts_single_li() {
        let pinsts = vec![PInst::Li { dst: gpr::RET_REGS[0], imm: 42 }];
        let emitted = emit_pinsts(&pinsts).unwrap();
        assert!(!emitted.code.is_empty());
        assert!(emitted.relocs.is_empty());
    }

    #[test]
    fn test_emit_pinsts_call_has_reloc() {
        let pinsts = vec![
            PInst::Call {
                target: SymbolRef {
                    name: alloc::string::String::from("foo"),
                },
            },
            PInst::Ret,
        ];
        let emitted = emit_pinsts(&pinsts).unwrap();
        assert!(!emitted.code.is_empty());
        assert_eq!(emitted.relocs.len(), 1);
        assert_eq!(emitted.relocs[0].symbol, "foo");
    }

    #[test]
    fn test_emit_pinsts_add_sub_sequence() {
        let pinsts = vec![
            PInst::Add {
                dst: 10,
                src1: 11,
                src2: 12,
            },
            PInst::Sub {
                dst: 10,
                src1: 10,
                src2: 11,
            },
            PInst::Ret,
        ];
        let emitted = emit_pinsts(&pinsts).unwrap();
        // 3 instructions * 4 bytes = 12 bytes minimum
        assert!(emitted.code.len() >= 12);
    }
}
