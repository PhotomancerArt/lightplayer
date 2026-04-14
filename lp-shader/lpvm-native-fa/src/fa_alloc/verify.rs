//! Structural invariant checks for AllocOutput.
//!
//! These run on every allocation result to catch bugs that snapshot tests miss.
//! They verify the output is *self-consistent*, independent of register choice.

use crate::abi::FuncAbi;
use crate::fa_alloc::{Alloc, AllocOutput};
use crate::rv32::gpr;
use crate::vinst::{VInst, VReg};
use alloc::vec::Vec;

/// Verify all structural invariants of an AllocOutput.
/// Panics with a descriptive message on violation.
pub fn verify_alloc(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &AllocOutput,
    func_abi: &FuncAbi,
) {
    verify_every_use_allocated(vinsts, vreg_pool, output);
    verify_no_double_reg_assignment(vinsts, vreg_pool, output);
    verify_edits_sorted(output);
    verify_allocs_within_pool(vinsts, vreg_pool, output, func_abi);
    verify_call_abi(vinsts, vreg_pool, output);
}

/// Every use operand must be allocated to Reg or Stack, never None.
fn verify_every_use_allocated(vinsts: &[VInst], vreg_pool: &[VReg], output: &AllocOutput) {
    for (inst_idx, inst) in vinsts.iter().enumerate() {
        let offset = output.inst_alloc_offsets[inst_idx] as usize;

        let mut num_defs: usize = 0;
        inst.for_each_def(vreg_pool, |_| num_defs += 1);

        let mut use_idx: usize = 0;
        inst.for_each_use(vreg_pool, |use_vreg| {
            let alloc = output.allocs[offset + num_defs + use_idx];
            assert!(
                alloc != Alloc::None,
                "inst {}: use operand i{} has Alloc::None (must be Reg or Stack)",
                inst_idx,
                use_vreg.0
            );
            use_idx += 1;
        });
    }
}

/// At any single instruction, no two *use* operands should map to the same physical register
/// *without* intervening edits that move values around.
///
/// When there are Before-edits for an instruction (reloads, evictions), operands may
/// appear in the same register in the alloc table because the edits will sequence them.
/// We only flag it when there are no edits to explain the sharing.
fn verify_no_double_reg_assignment(vinsts: &[VInst], vreg_pool: &[VReg], output: &AllocOutput) {
    use crate::fa_alloc::EditPoint;

    for (inst_idx, inst) in vinsts.iter().enumerate() {
        let offset = output.inst_alloc_offsets[inst_idx] as usize;
        let inst_idx_u16 = inst_idx as u16;

        // If there are Before-edits, the allocator is sequencing moves so
        // sharing a register across operands is expected.
        let has_before_edits = output
            .edits
            .iter()
            .any(|(pt, _)| *pt == EditPoint::Before(inst_idx_u16));
        if has_before_edits {
            return;
        }

        let mut num_defs: usize = 0;
        inst.for_each_def(vreg_pool, |_| num_defs += 1);

        let mut use_regs: Vec<(VReg, gpr::PReg)> = Vec::new();
        let mut use_idx: usize = 0;
        inst.for_each_use(vreg_pool, |use_vreg| {
            let alloc = output.allocs[offset + num_defs + use_idx];
            if let Alloc::Reg(preg) = alloc {
                for &(other_vreg, other_preg) in &use_regs {
                    if preg == other_preg && use_vreg != other_vreg {
                        panic!(
                            "inst {}: two different vregs (i{}, i{}) both in register x{} with no edits to sequence them",
                            inst_idx, use_vreg.0, other_vreg.0, preg
                        );
                    }
                }
                use_regs.push((use_vreg, preg));
            }
            use_idx += 1;
        });
    }
}

/// Edit list must be sorted by EditPoint.
fn verify_edits_sorted(output: &AllocOutput) {
    for window in output.edits.windows(2) {
        assert!(
            window[0].0 <= window[1].0,
            "Edits not sorted: {:?} > {:?}",
            window[0].0,
            window[1].0
        );
    }
}

/// Every Reg allocation must be within the allocatable pool, OR be a precolored ABI register,
/// OR be an ARG/RET register on a Call instruction.
fn verify_allocs_within_pool(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &AllocOutput,
    func_abi: &FuncAbi,
) {
    let mut precolored_regs: Vec<u8> = Vec::new();
    for (_vreg_idx, preg) in func_abi.precolors() {
        precolored_regs.push(preg.hw);
    }

    let is_precolored_reg = |preg: u8| precolored_regs.iter().any(|&p| p == preg);

    for (inst_idx, inst) in vinsts.iter().enumerate() {
        let offset = output.inst_alloc_offsets[inst_idx] as usize;
        let is_call = inst.is_call();

        let mut op_idx: usize = 0;
        let mut def_idx: usize = 0;
        inst.for_each_def(vreg_pool, |_def_vreg| {
            let alloc = output.allocs[offset + op_idx];
            if let Alloc::Reg(preg) = alloc {
                let allowed = gpr::pool_contains(preg)
                    || is_precolored_reg(preg)
                    || (is_call && def_idx < gpr::RET_REGS.len());
                assert!(
                    allowed,
                    "inst {}: def allocated to non-allocatable register x{}",
                    inst_idx, preg
                );
            }
            op_idx += 1;
            def_idx += 1;
        });
        let mut use_idx: usize = 0;
        inst.for_each_use(vreg_pool, |_use_vreg| {
            let alloc = output.allocs[offset + op_idx];
            if let Alloc::Reg(preg) = alloc {
                let allowed = gpr::pool_contains(preg)
                    || is_precolored_reg(preg)
                    || (is_call && use_idx < gpr::ARG_REGS.len());
                assert!(
                    allowed,
                    "inst {}: use allocated to non-allocatable register x{}",
                    inst_idx, preg
                );
            }
            op_idx += 1;
            use_idx += 1;
        });
    }
}

/// Call-specific ABI checks: ret operands in RET_REGS, arg operands in ARG_REGS.
fn verify_call_abi(vinsts: &[VInst], vreg_pool: &[VReg], output: &AllocOutput) {
    for (inst_idx, inst) in vinsts.iter().enumerate() {
        if !inst.is_call() {
            continue;
        }
        let offset = output.inst_alloc_offsets[inst_idx] as usize;

        let mut def_idx: usize = 0;
        inst.for_each_def(vreg_pool, |_def_vreg| {
            if def_idx < gpr::RET_REGS.len() {
                let expected = gpr::RET_REGS[def_idx];
                let actual = output.allocs[offset + def_idx];
                assert!(
                    actual == Alloc::Reg(expected),
                    "inst {} (Call): ret[{}] should be x{}, got {:?}",
                    inst_idx,
                    def_idx,
                    expected,
                    actual
                );
            }
            def_idx += 1;
        });

        let num_defs = def_idx;
        let mut use_idx: usize = 0;
        inst.for_each_use(vreg_pool, |_use_vreg| {
            if use_idx < gpr::ARG_REGS.len() {
                let expected = gpr::ARG_REGS[use_idx];
                let actual = output.allocs[offset + num_defs + use_idx];
                assert!(
                    actual == Alloc::Reg(expected),
                    "inst {} (Call): arg[{}] should be x{}, got {:?}",
                    inst_idx,
                    use_idx,
                    expected,
                    actual
                );
            }
            use_idx += 1;
        });
    }
}
