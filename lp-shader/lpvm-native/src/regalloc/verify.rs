//! Structural invariant checks for AllocOutput.
//!
//! These run on every allocation result to catch bugs that snapshot tests miss.
//! They verify the output is *self-consistent*, independent of register choice.

use crate::abi::{FuncAbi, PReg, RegClass};
use crate::regalloc::classes::{def_class, use_class};
use crate::regalloc::{Alloc, AllocOutput};
use crate::vinst::{VInst, VReg};
use alloc::vec::Vec;

/// Verify all structural invariants of an AllocOutput.
/// Panics with a descriptive message on violation.
///
/// This is a **test-time** oracle — `regalloc/test/builder.rs` runs it after
/// every allocation it builds. It is not on the on-device compile path, where a
/// panic would take the firmware down. The always-on counterpart for the
/// register class is [`SpillAlloc::get_or_assign`](crate::regalloc::spill)'s
/// debug assertion.
pub fn verify_alloc(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    output: &AllocOutput,
    func_abi: &FuncAbi,
) {
    verify_every_use_allocated(vinsts, vreg_pool, output);
    verify_no_double_reg_assignment(vinsts, vreg_pool, output);
    verify_edits_sorted(output);
    verify_operand_classes(vinsts, vreg_pool, output);
    verify_allocs_within_pool(vinsts, vreg_pool, output, func_abi);
    verify_call_abi(vinsts, vreg_pool, output, func_abi);
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

/// Every register allocation must be in the class its operand requires.
///
/// This is the oracle for a two-class allocator. Everything else in this file
/// is about *which* register was chosen; this one is about which register
/// *file*, and getting that wrong does not produce a crash or a bad address —
/// it produces a silent bit reinterpretation, an integer read as a float or a
/// float read as an integer, with plausible-looking wrong pixels at the end of
/// it. Nothing downstream can tell that apart from a correct result, so the
/// allocator has to be the thing that catches it.
fn verify_operand_classes(vinsts: &[VInst], vreg_pool: &[VReg], output: &AllocOutput) {
    for (inst_idx, inst) in vinsts.iter().enumerate() {
        let offset = output.inst_alloc_offsets[inst_idx] as usize;

        let mut op_idx: usize = 0;
        let mut def_idx: usize = 0;
        inst.for_each_def(vreg_pool, |def_vreg| {
            check_operand_class(
                output.allocs[offset + op_idx],
                def_class(inst, def_idx),
                inst_idx,
                "def",
                def_vreg,
            );
            op_idx += 1;
            def_idx += 1;
        });
        let mut use_idx: usize = 0;
        inst.for_each_use(vreg_pool, |use_vreg| {
            check_operand_class(
                output.allocs[offset + op_idx],
                use_class(inst, use_idx),
                inst_idx,
                "use",
                use_vreg,
            );
            op_idx += 1;
            use_idx += 1;
        });
    }
}

fn check_operand_class(alloc: Alloc, expected: RegClass, inst_idx: usize, role: &str, vreg: VReg) {
    if let Some(actual) = alloc.reg_class() {
        assert!(
            actual == expected,
            "inst {inst_idx}: {role} operand i{} needs a {expected:?} register but was allocated \
             to a {actual:?} one ({alloc:?})",
            vreg.0
        );
    }
}

/// At any single instruction, no two *use* operands should map to the same physical register
/// *without* intervening edits that move values around.
///
/// When there are Before-edits for an instruction (reloads, evictions), operands may
/// appear in the same register in the alloc table because the edits will sequence them.
/// We only flag it when there are no edits to explain the sharing.
fn verify_no_double_reg_assignment(vinsts: &[VInst], vreg_pool: &[VReg], output: &AllocOutput) {
    use crate::regalloc::EditPoint;

    for (inst_idx, inst) in vinsts.iter().enumerate() {
        let offset = output.inst_alloc_offsets[inst_idx] as usize;
        let inst_idx_u16 = inst_idx as u16;

        // If there are Before- or After-edits for this instruction, the
        // allocator is sequencing moves so sharing a register across
        // operands is expected.
        let has_edits = output.edits.iter().any(|(pt, _)| {
            *pt == EditPoint::Before(inst_idx_u16) || *pt == EditPoint::After(inst_idx_u16)
        });
        if has_edits {
            continue;
        }

        let mut num_defs: usize = 0;
        inst.for_each_def(vreg_pool, |_| num_defs += 1);

        // `PReg` equality includes the class, so the same hardware index in the
        // two files is correctly *not* a collision.
        let mut use_regs: Vec<(VReg, PReg)> = Vec::new();
        let mut use_idx: usize = 0;
        inst.for_each_use(vreg_pool, |use_vreg| {
            let alloc = output.allocs[offset + num_defs + use_idx];
            if let Some(preg) = alloc.preg() {
                for &(other_vreg, other_preg) in &use_regs {
                    if preg == other_preg && use_vreg != other_vreg {
                        panic!(
                            "inst {}: two different vregs (i{}, i{}) both in register x{} with no edits to sequence them",
                            inst_idx, use_vreg.0, other_vreg.0, preg.hw
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
    let isa = func_abi.isa();
    let mut precolored_regs: Vec<PReg> = Vec::new();
    for (_vreg_idx, preg) in func_abi.precolors() {
        precolored_regs.push(*preg);
    }

    let is_precolored_reg = |preg: PReg| precolored_regs.iter().any(|&p| p == preg);

    for (inst_idx, inst) in vinsts.iter().enumerate() {
        let offset = output.inst_alloc_offsets[inst_idx] as usize;
        let (is_call, callee_uses_sret, caller_passes_sret_ptr, caller_sret_vm_abi_swap) =
            match inst {
                VInst::Call {
                    callee_uses_sret,
                    caller_passes_sret_ptr,
                    caller_sret_vm_abi_swap,
                    ..
                } => (
                    true,
                    *callee_uses_sret,
                    *caller_passes_sret_ptr,
                    *caller_sret_vm_abi_swap,
                ),
                _ => (false, false, false, false),
            };

        let mut op_idx: usize = 0;
        let mut def_idx: usize = 0;
        inst.for_each_def(vreg_pool, |_def_vreg| {
            let alloc = output.allocs[offset + op_idx];
            if let Some(preg) = alloc.preg() {
                let allowed = isa.is_in_allocatable_pool(preg)
                    || is_precolored_reg(preg)
                    || (is_call
                        && !callee_uses_sret
                        && def_idx < isa.direct_ret_reg_count(preg.class));
                assert!(
                    allowed,
                    "inst {inst_idx}: def allocated to non-allocatable register x{}",
                    preg.hw
                );
            }
            op_idx += 1;
            def_idx += 1;
        });
        let mut use_idx: usize = 0;
        inst.for_each_use(vreg_pool, |_use_vreg| {
            let alloc = output.allocs[offset + op_idx];
            if let Some(preg) = alloc.preg() {
                let call_arg_slot = is_call
                    && isa
                        .lpir_call_arg_target(
                            preg.class,
                            callee_uses_sret,
                            caller_passes_sret_ptr,
                            caller_sret_vm_abi_swap,
                            use_idx,
                        )
                        .is_some();
                let allowed =
                    isa.is_in_allocatable_pool(preg) || is_precolored_reg(preg) || call_arg_slot;
                assert!(
                    allowed,
                    "inst {inst_idx}: use allocated to non-allocatable register x{}",
                    preg.hw
                );
            }
            op_idx += 1;
            use_idx += 1;
        });
    }
}

/// Call-specific ABI checks: ret operands in RET_REGS, arg operands in ARG_REGS.
/// For sret calls: rets are generic (not constrained to RET_REGS), args shifted by 1.
fn verify_call_abi(vinsts: &[VInst], vreg_pool: &[VReg], output: &AllocOutput, func_abi: &FuncAbi) {
    let isa = func_abi.isa();
    for (inst_idx, inst) in vinsts.iter().enumerate() {
        let (callee_uses_sret, caller_passes_sret_ptr, caller_sret_vm_abi_swap) = match inst {
            VInst::Call {
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                ..
            } => (
                *callee_uses_sret,
                *caller_passes_sret_ptr,
                *caller_sret_vm_abi_swap,
            ),
            _ => continue,
        };

        let offset = output.inst_alloc_offsets[inst_idx] as usize;

        if !callee_uses_sret {
            let mut def_idx: usize = 0;
            inst.for_each_def(vreg_pool, |_def_vreg| {
                if let Some(expected) = isa.direct_ret_reg(def_class(inst, def_idx), def_idx) {
                    let actual = output.allocs[offset + def_idx];
                    assert!(
                        actual == Alloc::reg(expected),
                        "inst {inst_idx} (Call): ret[{def_idx}] should be x{}, got {actual:?}",
                        expected.hw
                    );
                }
                def_idx += 1;
            });
        }

        let mut num_defs: usize = 0;
        inst.for_each_def(vreg_pool, |_| num_defs += 1);

        let mut use_idx: usize = 0;
        inst.for_each_use(vreg_pool, |_use_vreg| {
            if let Some(expected) = isa.lpir_call_arg_target(
                use_class(inst, use_idx),
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                use_idx,
            ) {
                let actual = output.allocs[offset + num_defs + use_idx];
                assert!(
                    actual == Alloc::reg(expected),
                    "inst {inst_idx} (Call): arg[{use_idx}] should be x{}, got {actual:?}",
                    expected.hw
                );
            }
            use_idx += 1;
        });
    }
}
