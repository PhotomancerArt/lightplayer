//! Backward walk allocator for Linear regions.
//!
//! Walks instructions in reverse order, allocating registers for uses,
//! freeing registers for defs, and recording spill/reload edits.

use crate::abi::FuncAbi;
use crate::fa_alloc::pool::RegPool;
use crate::fa_alloc::spill::SpillAlloc;
use crate::fa_alloc::trace::{AllocTrace, TraceEntry};
use crate::fa_alloc::{Alloc, AllocError, AllocOutput, Edit, EditPoint};
use crate::rv32::gpr::PReg;
use crate::vinst::{VInst, VReg};
use alloc::string::String;
use alloc::vec::Vec;

/// Walk a Linear region backward, producing AllocOutput.
pub fn walk_linear(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &FuncAbi,
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

    // Initialize state
    let mut pool = RegPool::new();
    let max_vreg_idx = vreg_pool.iter().map(|v| v.0).max().unwrap_or(0) as usize + 32;
    let mut spill = SpillAlloc::new(max_vreg_idx + 16);
    let mut trace = AllocTrace::new();
    let mut edits: Vec<(EditPoint, Edit)> = Vec::new();

    // Seed entry parameters at their ABI registers
    let mut entry_precolors: Vec<(VReg, PReg)> = Vec::new();
    for (vreg_idx, preg) in func_abi.precolors() {
        let vreg = VReg(*vreg_idx as u16);
        let abi_reg = preg.hw;
        pool.alloc_fixed(abi_reg, vreg);
        entry_precolors.push((vreg, abi_reg));
        trace.push(TraceEntry {
            vinst_idx: 0,
            vinst_mnemonic: String::from("entry"),
            decision: alloc::format!("v{} -> x{}", vreg.0, abi_reg),
            register_state: String::new(),
        });
    }

    // Walk instructions backward
    for (inst_idx, inst) in vinsts.iter().enumerate().rev() {
        let inst_idx_u16 = inst_idx as u16;
        let offset = inst_alloc_offsets[inst_idx] as usize;

        // Process defs first (backward order: defs are freed)
        let mut operand_idx: usize = 0;
        inst.for_each_def(vreg_pool, |def_vreg| {
            let alloc_idx = offset + operand_idx;
            operand_idx += 1;

            // Check where this vreg ended up (from processing earlier in backward walk)
            let alloc = if let Some(preg) = pool.home(def_vreg) {
                Alloc::Reg(preg)
            } else if let Some(slot) = spill.has_slot(def_vreg) {
                Alloc::Stack(slot)
            } else {
                // Vreg was never allocated (dead def) - this is ok
                Alloc::None
            };

            // Record the allocation for this def operand
            allocs[alloc_idx] = alloc;

            // Free the register if it was in one
            if let Some(preg) = pool.home(def_vreg) {
                pool.free(preg);
            }
        });

        // Process uses (backward order: uses are allocated)
        inst.for_each_use(vreg_pool, |use_vreg| {
            let alloc_idx = offset + operand_idx;
            operand_idx += 1;

            // Check current location of this vreg
            let alloc = if let Some(preg) = pool.home(use_vreg) {
                // Already in a register - mark as used (LRU update)
                pool.touch(preg);
                Alloc::Reg(preg)
            } else if let Some(slot) = spill.has_slot(use_vreg) {
                // Currently spilled - need to reload
                let (new_preg, evicted) = pool.alloc(use_vreg);

                // Record reload edit
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

                // If we evicted someone, spill them
                if let Some(evicted_vreg) = evicted {
                    let evict_slot = spill.get_or_assign(evicted_vreg);
                    edits.push((
                        EditPoint::Before(inst_idx_u16),
                        Edit::Move {
                            from: Alloc::Reg(new_preg),
                            to: Alloc::Stack(evict_slot),
                        },
                    ));
                    trace.push(TraceEntry {
                        vinst_idx: inst_idx,
                        vinst_mnemonic: String::from("evict"),
                        decision: alloc::format!("t{} -> slot{}", new_preg, evict_slot),
                        register_state: String::new(),
                    });
                }

                Alloc::Reg(new_preg)
            } else {
                // Not allocated anywhere - allocate fresh register
                let (new_preg, evicted) = pool.alloc(use_vreg);

                // If we evicted someone, spill them
                if let Some(evicted_vreg) = evicted {
                    let evict_slot = spill.get_or_assign(evicted_vreg);
                    edits.push((
                        EditPoint::Before(inst_idx_u16),
                        Edit::Move {
                            from: Alloc::Reg(new_preg),
                            to: Alloc::Stack(evict_slot),
                        },
                    ));
                    trace.push(TraceEntry {
                        vinst_idx: inst_idx,
                        vinst_mnemonic: String::from("evict"),
                        decision: alloc::format!("t{} -> slot{}", new_preg, evict_slot),
                        register_state: String::new(),
                    });
                }

                trace.push(TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("alloc"),
                    decision: alloc::format!("v{} -> t{}", use_vreg.0, new_preg),
                    register_state: String::new(),
                });

                Alloc::Reg(new_preg)
            };

            // Record the allocation for this use operand
            allocs[alloc_idx] = alloc;
        });
    }

    // Reverse edits (recorded in backward order, need forward order)
    edits.reverse();

    // Record entry moves for params that moved from ABI registers
    let mut entry_edits: Vec<(EditPoint, Edit)> = Vec::new();
    for (vreg, abi_reg) in entry_precolors {
        if let Some(final_preg) = pool.home(vreg) {
            if final_preg != abi_reg {
                // Param moved - need entry move
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
                    decision: alloc::format!("x{} -> t{}", abi_reg, final_preg),
                    register_state: String::new(),
                });
            }
        } else if let Some(slot) = spill.has_slot(vreg) {
            // Param spilled directly to stack
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
