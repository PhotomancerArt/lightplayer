//! Backward walk allocator for Linear regions.
//!
//! Walks instructions in reverse order, allocating registers for uses,
//! freeing registers for defs, and recording spill/reload edits.

use crate::abi::FuncAbi;
use crate::fa_alloc::pool::RegPool;
use crate::fa_alloc::spill::SpillAlloc;
use crate::fa_alloc::trace::{AllocTrace, TraceEntry};
use crate::fa_alloc::{Alloc, AllocError, AllocOutput, Edit, EditPoint};
use crate::rv32::gpr::{self, PReg};
use crate::vinst::{VInst, VReg};
use alloc::string::String;
use alloc::vec::Vec;

/// Walk a Linear region backward, producing AllocOutput.
pub fn walk_linear(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &FuncAbi,
) -> Result<AllocOutput, AllocError> {
    walk_linear_with_pool(vinsts, vreg_pool, func_abi, RegPool::new())
}

/// Walk a Linear region backward with a configured pool.
pub fn walk_linear_with_pool(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &FuncAbi,
    mut pool: RegPool,
) -> Result<AllocOutput, AllocError> {
    // Count total operands and build offset table
    let mut inst_alloc_offsets = Vec::with_capacity(vinsts.len());
    let mut total_operands: usize = 0;

    for inst in vinsts {
        let mut num_operands: usize = 0;
        inst.for_each_def(vreg_pool, |_def| num_operands += 1);
        inst.for_each_use(vreg_pool, |_use| num_operands += 1);
        inst_alloc_offsets.push(total_operands as u16);
        total_operands += num_operands;
    }

    // Allocate the flat allocation table
    let mut allocs: Vec<Alloc> = vec![Alloc::None; total_operands];
    let max_vreg_idx = vreg_pool.iter().map(|v| v.0).max().unwrap_or(0) as usize + 32;
    let mut spill = SpillAlloc::new(max_vreg_idx + 16);
    let mut trace = AllocTrace::new();
    let mut edits: Vec<(EditPoint, Edit)> = Vec::new();

    // Record entry precolors (ABI reg → vreg) for generating entry moves later.
    // We do NOT seed these into the pool — params get pool regs lazily when
    // the backward walk encounters their uses. This avoids stale allocations
    // when a call clobbers the ABI registers.
    let mut entry_precolors: Vec<(VReg, PReg)> = Vec::new();
    for (vreg_idx, preg) in func_abi.precolors() {
        let vreg = VReg(*vreg_idx as u16);
        let abi_reg = preg.hw;
        entry_precolors.push((vreg, abi_reg));
    }

    // Walk instructions backward
    for (inst_idx, inst) in vinsts.iter().enumerate().rev() {
        let inst_idx_u16 = inst_idx as u16;
        let offset = inst_alloc_offsets[inst_idx] as usize;

        if inst.is_call() {
            process_call(
                inst,
                inst_idx,
                inst_idx_u16,
                offset,
                vreg_pool,
                &mut pool,
                &mut spill,
                &mut allocs,
                &mut edits,
                &mut trace,
            );
        } else {
            process_generic(
                inst,
                inst_idx,
                inst_idx_u16,
                offset,
                vreg_pool,
                &mut pool,
                &mut spill,
                &mut allocs,
                &mut edits,
                &mut trace,
            );
        }
    }

    // Reverse edits (recorded in backward order, need forward order)
    edits.reverse();

    // Generate entry moves: ABI register → pool register (or spill slot).
    // Since params are NOT pre-seeded, every used param needs an entry move.
    let mut entry_edits: Vec<(EditPoint, Edit)> = Vec::new();
    for (vreg, abi_reg) in entry_precolors {
        if let Some(final_preg) = pool.home(vreg) {
            entry_edits.push((
                EditPoint::Before(0),
                Edit::Move {
                    from: Alloc::Reg(abi_reg),
                    to: Alloc::Reg(final_preg),
                },
            ));
            trace.push(TraceEntry {
                vinst_idx: 0,
                vinst_mnemonic: String::from("entry_move"),
                decision: alloc::format!("x{} -> x{}", abi_reg, final_preg),
                register_state: String::new(),
            });
        } else if let Some(slot) = spill.has_slot(vreg) {
            entry_edits.push((
                EditPoint::Before(0),
                Edit::Move {
                    from: Alloc::Reg(abi_reg),
                    to: Alloc::Stack(slot),
                },
            ));
            trace.push(TraceEntry {
                vinst_idx: 0,
                vinst_mnemonic: String::from("entry_spill"),
                decision: alloc::format!("x{} -> slot{}", abi_reg, slot),
                register_state: String::new(),
            });
        }
        // If param not in pool and not spilled, it was never used
    }

    // Entry edits go first (they're at Before(0))
    entry_edits.extend(edits);
    let final_edits = entry_edits;

    Ok(AllocOutput {
        allocs,
        inst_alloc_offsets,
        edits: final_edits,
        num_spill_slots: spill.total_slots(),
        trace,
    })
}

/// Generic (non-call) instruction processing.
fn process_generic(
    inst: &VInst,
    inst_idx: usize,
    inst_idx_u16: u16,
    offset: usize,
    vreg_pool: &[VReg],
    pool: &mut RegPool,
    spill: &mut SpillAlloc,
    allocs: &mut [Alloc],
    edits: &mut Vec<(EditPoint, Edit)>,
    trace: &mut AllocTrace,
) {
    let mut operand_idx: usize = 0;

    // Defs (backward: freed)
    inst.for_each_def(vreg_pool, |def_vreg| {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;

        let alloc = if let Some(preg) = pool.home(def_vreg) {
            Alloc::Reg(preg)
        } else if let Some(slot) = spill.has_slot(def_vreg) {
            Alloc::Stack(slot)
        } else {
            Alloc::None
        };

        allocs[alloc_idx] = alloc;
        if let Some(preg) = pool.home(def_vreg) {
            pool.free(preg);
        }
    });

    // Uses (backward: allocated)
    inst.for_each_use(vreg_pool, |use_vreg| {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;

        let alloc = alloc_use(use_vreg, inst_idx, inst_idx_u16, pool, spill, edits, trace);
        allocs[alloc_idx] = alloc;
    });
}

/// Allocate a use operand: reload from spill or allocate fresh, evicting if needed.
fn alloc_use(
    use_vreg: VReg,
    inst_idx: usize,
    inst_idx_u16: u16,
    pool: &mut RegPool,
    spill: &mut SpillAlloc,
    edits: &mut Vec<(EditPoint, Edit)>,
    trace: &mut AllocTrace,
) -> Alloc {
    if let Some(preg) = pool.home(use_vreg) {
        pool.touch(preg);
        Alloc::Reg(preg)
    } else if let Some(slot) = spill.has_slot(use_vreg) {
        let (new_preg, evicted) = pool.alloc(use_vreg);
        edits.push((
            EditPoint::Before(inst_idx_u16),
            Edit::Move {
                from: Alloc::Stack(slot),
                to: Alloc::Reg(new_preg),
            },
        ));
        trace.push(TraceEntry {
            vinst_idx: inst_idx,
            vinst_mnemonic: String::from("reload"),
            decision: alloc::format!("slot{} -> t{}", slot, new_preg),
            register_state: String::new(),
        });
        handle_eviction(
            evicted,
            new_preg,
            inst_idx,
            inst_idx_u16,
            spill,
            edits,
            trace,
        );
        Alloc::Reg(new_preg)
    } else {
        let (new_preg, evicted) = pool.alloc(use_vreg);
        handle_eviction(
            evicted,
            new_preg,
            inst_idx,
            inst_idx_u16,
            spill,
            edits,
            trace,
        );
        trace.push(TraceEntry {
            vinst_idx: inst_idx,
            vinst_mnemonic: String::from("alloc"),
            decision: alloc::format!("v{} -> t{}", use_vreg.0, new_preg),
            register_state: String::new(),
        });
        Alloc::Reg(new_preg)
    }
}

fn handle_eviction(
    evicted: Option<VReg>,
    preg: PReg,
    inst_idx: usize,
    inst_idx_u16: u16,
    spill: &mut SpillAlloc,
    edits: &mut Vec<(EditPoint, Edit)>,
    trace: &mut AllocTrace,
) {
    if let Some(evicted_vreg) = evicted {
        let slot = spill.get_or_assign(evicted_vreg);
        edits.push((
            EditPoint::Before(inst_idx_u16),
            Edit::Move {
                from: Alloc::Reg(preg),
                to: Alloc::Stack(slot),
            },
        ));
        trace.push(TraceEntry {
            vinst_idx: inst_idx,
            vinst_mnemonic: String::from("evict"),
            decision: alloc::format!("t{} -> slot{}", preg, slot),
            register_state: String::new(),
        });
    }
}

/// 3-step call handling algorithm.
///
/// Step 1: Defs — constrain ret vregs to RET_REGS, emit After moves
/// Step 2: Clobber save/restore for caller-saved pool regs (t-regs)
/// Step 3: Uses — constrain arg vregs to ARG_REGS, emit Before moves
///
/// Edit ordering after global reverse:
///   Before(call): saves first, then arg moves
///   After(call):  ret moves first, then restores
fn process_call(
    inst: &VInst,
    inst_idx: usize,
    inst_idx_u16: u16,
    offset: usize,
    vreg_pool: &[VReg],
    pool: &mut RegPool,
    spill: &mut SpillAlloc,
    allocs: &mut [Alloc],
    edits: &mut Vec<(EditPoint, Edit)>,
    trace: &mut AllocTrace,
) {
    let (args_slice, rets_slice) = match inst {
        VInst::Call { args, rets, .. } => (*args, *rets),
        _ => unreachable!(),
    };

    let args = args_slice.vregs(vreg_pool);
    let rets = rets_slice.vregs(vreg_pool);

    // Collect edits in forward order; we'll push in reverse for the backward walk.
    let mut before_saves: Vec<(EditPoint, Edit)> = Vec::new();
    let mut before_arg_moves: Vec<(EditPoint, Edit)> = Vec::new();
    let mut after_ret_moves: Vec<(EditPoint, Edit)> = Vec::new();
    let mut after_restores: Vec<(EditPoint, Edit)> = Vec::new();

    // ── Step 1: Defs (return values) ──
    let mut operand_idx: usize = 0;
    for (i, &ret_vreg) in rets.iter().enumerate() {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;

        if i >= gpr::RET_REGS.len() {
            // Extra rets beyond register return slots — process generically.
            // (sret will handle these in Phase 3.)
            let alloc = if let Some(preg) = pool.home(ret_vreg) {
                Alloc::Reg(preg)
            } else if let Some(slot) = spill.has_slot(ret_vreg) {
                Alloc::Stack(slot)
            } else {
                Alloc::None
            };
            allocs[alloc_idx] = alloc;
            if let Some(preg) = pool.home(ret_vreg) {
                pool.free(preg);
            }
            continue;
        }

        let target = gpr::RET_REGS[i];

        allocs[alloc_idx] = Alloc::Reg(target);

        if let Some(pool_reg) = pool.home(ret_vreg) {
            // Vreg lives later in pool_reg: move ret_reg → pool_reg after call
            after_ret_moves.push((
                EditPoint::After(inst_idx_u16),
                Edit::Move {
                    from: Alloc::Reg(target),
                    to: Alloc::Reg(pool_reg),
                },
            ));
            trace.push(TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("call_ret"),
                decision: alloc::format!("x{} -> x{} (v{})", target, pool_reg, ret_vreg.0),
                register_state: String::new(),
            });
            pool.free(pool_reg);
        } else if let Some(slot) = spill.has_slot(ret_vreg) {
            // Vreg is spilled: move ret_reg → stack after call
            after_ret_moves.push((
                EditPoint::After(inst_idx_u16),
                Edit::Move {
                    from: Alloc::Reg(target),
                    to: Alloc::Stack(slot),
                },
            ));
            trace.push(TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("call_ret"),
                decision: alloc::format!("x{} -> slot{} (v{})", target, slot, ret_vreg.0),
                register_state: String::new(),
            });
        }
        // else: dead def, no move needed
    }

    // (Step 2 — relocate a-reg precolors — removed: params are no longer
    // pre-seeded into a-regs, so there is nothing to relocate.)

    // ── Step 3: Clobber save/restore for caller-saved pool t-regs ──
    for &preg in gpr::CALLER_SAVED_POOL {
        if let Some(vreg) = pool
            .iter_occupied()
            .find(|&(p, _)| p == preg)
            .map(|(_, v)| v)
        {
            let slot = spill.get_or_assign(vreg);
            before_saves.push((
                EditPoint::Before(inst_idx_u16),
                Edit::Move {
                    from: Alloc::Reg(preg),
                    to: Alloc::Stack(slot),
                },
            ));
            after_restores.push((
                EditPoint::After(inst_idx_u16),
                Edit::Move {
                    from: Alloc::Stack(slot),
                    to: Alloc::Reg(preg),
                },
            ));
            trace.push(TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("clobber_save"),
                decision: alloc::format!("x{} -> slot{} (v{})", preg, slot, vreg.0),
                register_state: String::new(),
            });
        }
    }

    // ── Step 4: Uses (arguments) ──
    for (i, &arg_vreg) in args.iter().enumerate() {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;

        if i >= gpr::ARG_REGS.len() {
            // Stack-passed arg: process as normal use (emitter handles)
            let alloc = alloc_use(arg_vreg, inst_idx, inst_idx_u16, pool, spill, edits, trace);
            allocs[alloc_idx] = alloc;
            continue;
        }

        let target = gpr::ARG_REGS[i]; // base=0, sret shift is Phase 3

        if let Some(pool_reg) = pool.home(arg_vreg) {
            pool.touch(pool_reg);
            if pool_reg != target {
                before_arg_moves.push((
                    EditPoint::Before(inst_idx_u16),
                    Edit::Move {
                        from: Alloc::Reg(pool_reg),
                        to: Alloc::Reg(target),
                    },
                ));
            }
            trace.push(TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("call_arg"),
                decision: alloc::format!("v{}: x{} -> x{}", arg_vreg.0, pool_reg, target),
                register_state: String::new(),
            });
        } else if let Some(slot) = spill.has_slot(arg_vreg) {
            before_arg_moves.push((
                EditPoint::Before(inst_idx_u16),
                Edit::Move {
                    from: Alloc::Stack(slot),
                    to: Alloc::Reg(target),
                },
            ));
            trace.push(TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("call_arg"),
                decision: alloc::format!("v{}: slot{} -> x{}", arg_vreg.0, slot, target),
                register_state: String::new(),
            });
        } else {
            // Not yet allocated — allocate to a pool reg for the backward walk
            let (new_preg, evicted) = pool.alloc(arg_vreg);
            if let Some(ev) = evicted {
                let slot = spill.get_or_assign(ev);
                before_saves.push((
                    EditPoint::Before(inst_idx_u16),
                    Edit::Move {
                        from: Alloc::Reg(new_preg),
                        to: Alloc::Stack(slot),
                    },
                ));
                trace.push(TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("evict"),
                    decision: alloc::format!("x{} -> slot{} (v{})", new_preg, slot, ev.0),
                    register_state: String::new(),
                });
            }
            if new_preg != target {
                before_arg_moves.push((
                    EditPoint::Before(inst_idx_u16),
                    Edit::Move {
                        from: Alloc::Reg(new_preg),
                        to: Alloc::Reg(target),
                    },
                ));
            }
            trace.push(TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("call_arg"),
                decision: alloc::format!("v{}: x{} -> x{}", arg_vreg.0, new_preg, target),
                register_state: String::new(),
            });
        }

        allocs[alloc_idx] = Alloc::Reg(target);
    }

    // Push edits in reverse-forward order (global reverse will restore forward order).
    // Desired forward: Before(saves, arg_moves), After(ret_moves, restores)
    // Push order:      After(restores), After(ret_moves), Before(arg_moves), Before(saves)
    for e in after_restores.into_iter().rev() {
        edits.push(e);
    }
    for e in after_ret_moves.into_iter().rev() {
        edits.push(e);
    }
    for e in before_arg_moves.into_iter().rev() {
        edits.push(e);
    }
    for e in before_saves.into_iter().rev() {
        edits.push(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debug::vinst;
    use crate::rv32::abi;
    use lps_shared::{LpsFnSig, LpsType};

    fn make_abi() -> FuncAbi {
        abi::func_abi_rv32(
            &LpsFnSig {
                name: alloc::string::String::from("test"),
                return_type: LpsType::Void,
                parameters: vec![],
            },
            0,
        )
    }

    #[test]
    fn walk_empty() {
        let output = walk_linear(&[], &[], &make_abi()).unwrap();
        assert!(output.allocs.is_empty());
        assert!(output.edits.is_empty());
        assert_eq!(output.num_spill_slots, 0);
    }

    #[test]
    fn walk_simple_iconst() {
        let input = "i0 = IConst32 10\nRet i0";
        let (vinsts, _symbols, pool) = vinst::parse(input).unwrap();
        let output = walk_linear(&vinsts, &pool, &make_abi()).unwrap();

        // Should have 2 instructions
        assert_eq!(output.inst_alloc_offsets.len(), 2);

        // IConst32: 1 def (i0), 0 uses
        // Ret: 0 defs, 1 use (i0)
        // Total: 2 operands
        assert_eq!(output.allocs.len(), 2);

        // Both should be registers (no spill needed)
        assert!(output.allocs.iter().all(|a| a.is_reg()));
    }

    #[test]
    fn walk_binary_add() {
        let input = "i0 = IConst32 10\ni1 = IConst32 20\ni2 = Add32 i0, i1\nRet i2";
        let (vinsts, _symbols, pool) = vinst::parse(input).unwrap();
        let output = walk_linear(&vinsts, &pool, &make_abi()).unwrap();

        // 4 instructions
        assert_eq!(output.inst_alloc_offsets.len(), 4);

        // Should not need spill for this simple case
        assert_eq!(output.num_spill_slots, 0);
    }
}
