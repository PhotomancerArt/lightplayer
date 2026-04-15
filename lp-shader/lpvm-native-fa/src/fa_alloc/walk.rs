//! Backward walk allocator: [`RegionTree`] dispatch with spill-at-boundary.
//!
//! Walks instructions in reverse order, allocating registers for uses,
//! freeing registers for defs, and recording spill/reload edits.

use crate::abi::FuncAbi;
use crate::fa_alloc::pool::RegPool;
use crate::fa_alloc::spill::SpillAlloc;
use crate::fa_alloc::trace::TraceEntry;
use crate::fa_alloc::{
    Alloc, AllocError, AllocOutput, Edit, EditPoint, TracePush, TraceSink, trace_sink_new,
};
use crate::region::{REGION_ID_NONE, Region, RegionId, RegionTree};
use crate::regset::RegSet;
use crate::rv32::gpr::{self, PReg};
use crate::vinst::{VInst, VReg};
use alloc::string::String;
use alloc::vec::Vec;

/// Per-instruction operand offsets into the flat `allocs` table (global indices).
pub(crate) fn build_operand_layout(vinsts: &[VInst], vreg_pool: &[VReg]) -> (Vec<u16>, usize) {
    let mut inst_alloc_offsets = Vec::with_capacity(vinsts.len());
    let mut total_operands: usize = 0;
    for inst in vinsts {
        inst_alloc_offsets.push(total_operands as u16);
        let mut num_operands: usize = 0;
        inst.for_each_def(vreg_pool, |_def| num_operands += 1);
        inst.for_each_use(vreg_pool, |_use| num_operands += 1);
        total_operands += num_operands;
    }
    (inst_alloc_offsets, total_operands)
}

/// First VInst index in `vinsts` covered by this region (for boundary edit anchors).
fn region_first_vinst(tree: &RegionTree, id: RegionId) -> Option<u16> {
    if id == REGION_ID_NONE {
        return None;
    }
    match &tree.nodes[id as usize] {
        Region::Linear { start, end } => {
            if start < end {
                Some(*start)
            } else {
                None
            }
        }
        Region::Seq {
            children_start,
            child_count,
        } => {
            let s = *children_start as usize;
            let e = s + *child_count as usize;
            tree.seq_children[s..e]
                .iter()
                .find_map(|&c| region_first_vinst(tree, c))
        }
        Region::IfThenElse { head, .. } => region_first_vinst(tree, *head),
        Region::Loop { header, body, .. } => {
            region_first_vinst(tree, *header).or_else(|| region_first_vinst(tree, *body))
        }
    }
}

/// Register allocation over the full `vinsts` slice using a region tree root.
pub fn allocate_from_tree(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    tree: &RegionTree,
    root: RegionId,
    func_abi: &FuncAbi,
    pool: RegPool,
) -> Result<AllocOutput, AllocError> {
    let (inst_alloc_offsets, total_operands) = build_operand_layout(vinsts, vreg_pool);
    let max_vreg_idx = vreg_pool.iter().map(|v| v.0).max().unwrap_or(0) as usize + 32;
    let mut state = WalkState {
        vinsts,
        vreg_pool,
        func_abi,
        tree,
        inst_alloc_offsets,
        pool,
        spill: SpillAlloc::new(max_vreg_idx + 16),
        allocs: vec![Alloc::None; total_operands],
        edits: Vec::new(),
        trace: trace_sink_new(),
        loop_carried: RegSet::new(),
    };
    state.walk_region(root)?;
    state.finish()
}

struct WalkState<'a> {
    vinsts: &'a [VInst],
    vreg_pool: &'a [VReg],
    func_abi: &'a FuncAbi,
    tree: &'a RegionTree,
    inst_alloc_offsets: Vec<u16>,
    pool: RegPool,
    spill: SpillAlloc,
    allocs: Vec<Alloc>,
    edits: Vec<(EditPoint, Edit)>,
    trace: TraceSink,
    /// VRegs that are loop-carried: defs to registers get a store-after-def
    /// edit so the spill slot always has the latest value at sub-boundaries.
    loop_carried: RegSet,
}

impl<'a> WalkState<'a> {
    fn walk_region(&mut self, id: RegionId) -> Result<(), AllocError> {
        if id == REGION_ID_NONE {
            return Ok(());
        }
        match &self.tree.nodes[id as usize] {
            Region::Linear { start, end } => self.walk_linear_range(*start as usize, *end as usize),
            Region::Seq {
                children_start,
                child_count,
            } => {
                let s = *children_start as usize;
                let e = s + *child_count as usize;
                let children: Vec<RegionId> = self.tree.seq_children[s..e].to_vec();
                for idx in (0..children.len()).rev() {
                    let child = children[idx];
                    self.walk_region(child)?;
                    if idx > 0 {
                        if let Some(anchor) = region_first_vinst(self.tree, child) {
                            self.boundary_reload_before(anchor);
                        }
                    }
                }
                Ok(())
            }
            Region::IfThenElse {
                head,
                then_body,
                else_body,
                ..
            } => {
                if *else_body != REGION_ID_NONE {
                    self.walk_region(*else_body)?;
                    if let Some(anchor) = region_first_vinst(self.tree, *then_body) {
                        self.boundary_reload_before(anchor);
                    }
                }
                self.walk_region(*then_body)?;
                if let Some(anchor) = region_first_vinst(self.tree, *head) {
                    self.boundary_reload_before(anchor);
                }
                self.walk_region(*head)?;
                Ok(())
            }
            Region::Loop { header, body, .. } => {
                if *body != REGION_ID_NONE {
                    // Pre-assign spill slots for loop-carried values so that
                    // defs inside the body write directly to the slot. The
                    // back-edge Br is a no-op; without pre-assignment the
                    // updated value would never reach the spill slot and the
                    // next iteration's header reload would read stale data.
                    let body_live = crate::fa_alloc::liveness::analyze_liveness(
                        self.tree,
                        *body,
                        self.vinsts,
                        self.vreg_pool,
                    );
                    for vreg in body_live.live_in.iter() {
                        self.spill.get_or_assign(vreg);
                        self.loop_carried.insert(vreg);
                    }

                    self.walk_region(*body)?;
                    if let Some(anchor) = region_first_vinst(self.tree, *body) {
                        self.boundary_reload_before(anchor);
                    }
                }
                self.walk_region(*header)?;
                Ok(())
            }
        }
    }

    fn walk_linear_range(&mut self, start: usize, end: usize) -> Result<(), AllocError> {
        for inst_idx in (start..end).rev() {
            let inst = &self.vinsts[inst_idx];
            let inst_idx_u16 = inst_idx as u16;
            let offset = self.inst_alloc_offsets[inst_idx] as usize;

            if inst.is_call() {
                process_call(
                    inst,
                    inst_idx,
                    inst_idx_u16,
                    offset,
                    self.vreg_pool,
                    &mut self.pool,
                    &mut self.spill,
                    &mut self.allocs,
                    &mut self.edits,
                    &mut self.trace,
                );
            } else {
                process_generic(
                    inst,
                    inst_idx,
                    inst_idx_u16,
                    offset,
                    self.vreg_pool,
                    &mut self.pool,
                    &mut self.spill,
                    &mut self.allocs,
                    &mut self.edits,
                    &mut self.trace,
                );
            }

            // For loop-carried defs allocated to a register, insert a store
            // so the spill slot always holds the latest value. Without this,
            // values modified inside a loop body but not used in a later
            // sub-region (e.g. the continuing block) would never reach the
            // slot, and the next iteration's header reload would read stale
            // data.
            if !self.loop_carried.is_empty() {
                let mut def_idx = offset;
                self.vinsts[inst_idx].for_each_def(self.vreg_pool, |def_vreg| {
                    if self.loop_carried.contains(def_vreg) {
                        if let Alloc::Reg(preg) = self.allocs[def_idx] {
                            if let Some(slot) = self.spill.has_slot(def_vreg) {
                                self.edits.push((
                                    EditPoint::After(inst_idx_u16),
                                    Edit::Move {
                                        from: Alloc::Reg(preg),
                                        to: Alloc::Stack(slot),
                                    },
                                ));
                            }
                        }
                    }
                    def_idx += 1;
                });
            }
        }
        Ok(())
    }

    /// At a region boundary, ensure every pool-resident value has a spill slot
    /// and insert a RELOAD edit (slot → reg) before `anchor`. The preceding
    /// region's backward walk will see the spill slot and direct its def there;
    /// the reload fills the register expected by the following region.
    fn boundary_reload_before(&mut self, anchor: u16) {
        let occupied: Vec<(PReg, VReg)> = self.pool.iter_occupied().collect();
        for (preg, vreg) in occupied {
            let slot = self.spill.get_or_assign(vreg);
            self.edits.push((
                EditPoint::Before(anchor),
                Edit::Move {
                    from: Alloc::Stack(slot),
                    to: Alloc::Reg(preg),
                },
            ));
            self.pool.free(preg);
        }
    }

    fn finish(mut self) -> Result<AllocOutput, AllocError> {
        self.edits.reverse();
        self.edits.sort_by_key(|(pt, _)| *pt);

        let mut entry_precolors: Vec<(VReg, PReg)> = Vec::new();
        for (vreg_idx, preg) in self.func_abi.precolors() {
            let vreg = VReg(*vreg_idx as u16);
            entry_precolors.push((vreg, preg.hw));
        }

        let mut entry_edits: Vec<(EditPoint, Edit)> = Vec::new();
        for (vreg, abi_reg) in entry_precolors {
            if let Some(final_preg) = self.pool.home(vreg) {
                entry_edits.push((
                    EditPoint::Before(0),
                    Edit::Move {
                        from: Alloc::Reg(abi_reg),
                        to: Alloc::Reg(final_preg),
                    },
                ));
                TracePush::push(
                    &mut self.trace,
                    TraceEntry {
                        vinst_idx: 0,
                        vinst_mnemonic: String::from("entry_move"),
                        decision: alloc::format!("x{} -> x{}", abi_reg, final_preg),
                        register_state: String::new(),
                    },
                );
            } else if let Some(slot) = self.spill.has_slot(vreg) {
                entry_edits.push((
                    EditPoint::Before(0),
                    Edit::Move {
                        from: Alloc::Reg(abi_reg),
                        to: Alloc::Stack(slot),
                    },
                ));
                TracePush::push(
                    &mut self.trace,
                    TraceEntry {
                        vinst_idx: 0,
                        vinst_mnemonic: String::from("entry_spill"),
                        decision: alloc::format!("x{} -> slot{}", abi_reg, slot),
                        register_state: String::new(),
                    },
                );
            }
        }

        for (vreg_idx, loc) in self.func_abi.param_locs().iter().enumerate() {
            if let crate::abi::classify::ArgLoc::Stack { offset, .. } = loc {
                let vreg = VReg(vreg_idx as u16);
                if let Some(final_preg) = self.pool.home(vreg) {
                    entry_edits.push((
                        EditPoint::Before(0),
                        Edit::LoadIncomingArg {
                            fp_offset: *offset,
                            to: Alloc::Reg(final_preg),
                        },
                    ));
                    TracePush::push(
                        &mut self.trace,
                        TraceEntry {
                            vinst_idx: 0,
                            vinst_mnemonic: String::from("entry_load_stack_arg"),
                            decision: alloc::format!("[fp+{}] -> x{}", offset, final_preg),
                            register_state: String::new(),
                        },
                    );
                } else if let Some(slot) = self.spill.has_slot(vreg) {
                    entry_edits.push((
                        EditPoint::Before(0),
                        Edit::LoadIncomingArg {
                            fp_offset: *offset,
                            to: Alloc::Stack(slot),
                        },
                    ));
                    TracePush::push(
                        &mut self.trace,
                        TraceEntry {
                            vinst_idx: 0,
                            vinst_mnemonic: String::from("entry_load_stack_arg"),
                            decision: alloc::format!("[fp+{}] -> slot{}", offset, slot),
                            register_state: String::new(),
                        },
                    );
                }
            }
        }

        entry_edits.extend(self.edits);
        Ok(AllocOutput {
            allocs: self.allocs,
            inst_alloc_offsets: self.inst_alloc_offsets,
            edits: entry_edits,
            num_spill_slots: self.spill.total_slots(),
            trace: self.trace,
        })
    }
}

/// Walk a Linear region backward, producing AllocOutput (whole slice = one Linear root).
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
    pool: RegPool,
) -> Result<AllocOutput, AllocError> {
    let mut tree = RegionTree::new();
    let root = if vinsts.is_empty() {
        REGION_ID_NONE
    } else {
        tree.push(Region::Linear {
            start: 0,
            end: vinsts.len() as u16,
        })
    };
    tree.root = root;
    allocate_from_tree(vinsts, vreg_pool, &tree, root, func_abi, pool)
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
    trace: &mut TraceSink,
) {
    // Special case: Mov32 is a copy. Coalesce src and dst to the same register
    // to eliminate the move at emission time (emitter skips addi rd, rs, 0 when rd==rs).
    if let VInst::Mov32 { dst, src, .. } = inst {
        let def_idx = offset;
        let use_idx = offset + 1;

        // Def side: determine dst's allocation (already assigned earlier in backward walk)
        let dst_alloc = if let Some(preg) = pool.home(*dst) {
            Alloc::Reg(preg)
        } else if let Some(slot) = spill.has_slot(*dst) {
            Alloc::Stack(slot)
        } else {
            Alloc::None
        };
        allocs[def_idx] = dst_alloc;

        // If dst is in a register, free it and try to coalesce with src.
        // Only coalesce when src does NOT already have a home register --
        // if src is already live in another register (from later uses processed
        // earlier in the backward walk), forcing it into dst's register would
        // create duplicate pool entries and corrupt allocation state.
        if let Some(preg) = pool.home(*dst) {
            let src_has_home = pool.home(*src).is_some();
            pool.free(preg);
            if src_has_home {
                allocs[use_idx] =
                    alloc_use(*src, inst_idx, inst_idx_u16, pool, spill, edits, trace);
            } else {
                let evicted = pool.alloc_fixed(preg, *src);
                if let Some(evicted_vreg) = evicted {
                    let slot = spill.get_or_assign(evicted_vreg);
                    edits.push((
                        EditPoint::After(inst_idx_u16),
                        Edit::Move {
                            from: Alloc::Stack(slot),
                            to: Alloc::Reg(preg),
                        },
                    ));
                    TracePush::push(
                        trace,
                        TraceEntry {
                            vinst_idx: inst_idx,
                            vinst_mnemonic: String::from("coalesce_evict"),
                            decision: alloc::format!(
                                "slot{} -> t{} (v{})",
                                slot,
                                preg,
                                evicted_vreg.0
                            ),
                            register_state: String::new(),
                        },
                    );
                }
                allocs[use_idx] = Alloc::Reg(preg);
                TracePush::push(
                    trace,
                    TraceEntry {
                        vinst_idx: inst_idx,
                        vinst_mnemonic: String::from("coalesce"),
                        decision: alloc::format!("v{} -> t{} (shared)", src.0, preg),
                        register_state: String::new(),
                    },
                );
            }
        } else {
            // Dst is spilled or dead: use normal allocation path for src
            allocs[use_idx] = alloc_use(*src, inst_idx, inst_idx_u16, pool, spill, edits, trace);
        }
        return;
    }

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
    // Sret Ret: force ALL operands to Alloc::Stack (regalloc2-style Stack constraint).
    // The emitter loads each into TEMP0 and stores to the sret buffer sequentially,
    // so no register conflicts are possible. This eliminates the entire class of
    // Ret operand collisions where later operands evict earlier ones.
    let is_sret_ret = matches!(inst, VInst::Ret { vals, .. } if (vals.count as usize) > crate::rv32::abi::SRET_SCALAR_THRESHOLD);
    inst.for_each_use(vreg_pool, |use_vreg| {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;

        let alloc = if is_sret_ret {
            let slot = spill.get_or_assign(use_vreg);
            if let Some(preg) = pool.home(use_vreg) {
                pool.free(preg);
            }
            Alloc::Stack(slot)
        } else {
            alloc_use(use_vreg, inst_idx, inst_idx_u16, pool, spill, edits, trace)
        };
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
    trace: &mut TraceSink,
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
        TracePush::push(
            trace,
            TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("reload"),
                decision: alloc::format!("slot{} -> t{}", slot, new_preg),
                register_state: String::new(),
            },
        );
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
        TracePush::push(
            trace,
            TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("alloc"),
                decision: alloc::format!("v{} -> t{}", use_vreg.0, new_preg),
                register_state: String::new(),
            },
        );
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
    trace: &mut TraceSink,
) {
    if let Some(evicted_vreg) = evicted {
        let slot = spill.get_or_assign(evicted_vreg);
        // Emit a reload-after (regalloc2 style): the evicted vreg's DEF will
        // write directly to its spill slot.  After the current instruction
        // finishes, we reload the spilled value back into the register so it
        // is available for subsequent uses.
        edits.push((
            EditPoint::After(inst_idx_u16),
            Edit::Move {
                from: Alloc::Stack(slot),
                to: Alloc::Reg(preg),
            },
        ));
        TracePush::push(
            trace,
            TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("evict"),
                decision: alloc::format!("slot{} -> t{}", slot, preg),
                register_state: String::new(),
            },
        );
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
    trace: &mut TraceSink,
) {
    let (args_slice, rets_slice, callee_uses_sret) = match inst {
        VInst::Call {
            args,
            rets,
            callee_uses_sret,
            ..
        } => (*args, *rets, *callee_uses_sret),
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

        if callee_uses_sret || i >= gpr::RET_REGS.len() {
            // Sret call: all rets come from the sret buffer (emitter loads them).
            // Non-sret: extra rets beyond register return slots.
            // In both cases process generically — no RET_REG constraint.
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
            TracePush::push(
                trace,
                TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("call_ret"),
                    decision: alloc::format!("x{} -> x{} (v{})", target, pool_reg, ret_vreg.0),
                    register_state: String::new(),
                },
            );
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
            TracePush::push(
                trace,
                TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("call_ret"),
                    decision: alloc::format!("x{} -> slot{} (v{})", target, slot, ret_vreg.0),
                    register_state: String::new(),
                },
            );
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
            TracePush::push(
                trace,
                TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("clobber_save"),
                    decision: alloc::format!("x{} -> slot{} (v{})", preg, slot, vreg.0),
                    register_state: String::new(),
                },
            );
        }
    }

    // ── Step 4: Uses (arguments) ──
    // Evictions during arg allocation: (preg, spill_slot).
    // The evicted vreg's value is already in its spill slot from its def, so no
    // save is needed. But we must restore the value after the call so that
    // forward-time instructions see the correct register contents.
    let arg_base = if callee_uses_sret { 1 } else { 0 };
    let mut arg_evictions: Vec<(PReg, u8)> = Vec::new();
    for (i, &arg_vreg) in args.iter().enumerate() {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;

        if arg_base + i >= gpr::ARG_REGS.len() {
            // Stack-passed arg: process as normal use (emitter handles)
            let alloc = alloc_use(arg_vreg, inst_idx, inst_idx_u16, pool, spill, edits, trace);
            allocs[alloc_idx] = alloc;
            continue;
        }

        let target = gpr::ARG_REGS[arg_base + i];

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
            TracePush::push(
                trace,
                TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("call_arg"),
                    decision: alloc::format!("v{}: x{} -> x{}", arg_vreg.0, pool_reg, target),
                    register_state: String::new(),
                },
            );
        } else if let Some(slot) = spill.has_slot(arg_vreg) {
            before_arg_moves.push((
                EditPoint::Before(inst_idx_u16),
                Edit::Move {
                    from: Alloc::Stack(slot),
                    to: Alloc::Reg(target),
                },
            ));
            TracePush::push(
                trace,
                TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("call_arg"),
                    decision: alloc::format!("v{}: slot{} -> x{}", arg_vreg.0, slot, target),
                    register_state: String::new(),
                },
            );
        } else {
            // Not yet allocated — allocate to a pool reg for the backward walk
            let (new_preg, evicted) = pool.alloc(arg_vreg);
            if let Some(ev) = evicted {
                let slot = spill.get_or_assign(ev);
                arg_evictions.push((new_preg, slot));
                TracePush::push(
                    trace,
                    TraceEntry {
                        vinst_idx: inst_idx,
                        vinst_mnemonic: String::from("evict"),
                        decision: alloc::format!("x{} -> slot{} (v{})", new_preg, slot, ev.0),
                        register_state: String::new(),
                    },
                );
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
            TracePush::push(
                trace,
                TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("call_arg"),
                    decision: alloc::format!("v{}: x{} -> x{}", arg_vreg.0, new_preg, target),
                    register_state: String::new(),
                },
            );
        }

        allocs[alloc_idx] = Alloc::Reg(target);
    }

    // Fix up evictions during arg processing. The evicted vreg's value is
    // already in its spill slot (from the def), so no save is needed.  But
    // after the call the register must be restored to its pre-eviction value so
    // that forward-time instructions see the correct contents.
    //
    // For caller-saved regs: a clobber save/restore pair already exists.
    //   - Remove the SAVE (it would overwrite the slot with the wrong value).
    //   - Keep the RESTORE (it reloads the correct value from the spill slot).
    //
    // For callee-saved regs: no clobber pair exists, but the call doesn't
    //   clobber the register either — the register retains the NEW arg value
    //   after the call. We must add an explicit RESTORE.
    for &(preg, slot) in &arg_evictions {
        if gpr::is_caller_saved_pool(preg) {
            before_saves.retain(
                |(_, e)| !matches!(e, Edit::Move { from: Alloc::Reg(r), .. } if *r == preg),
            );
        } else {
            after_restores.push((
                EditPoint::After(inst_idx_u16),
                Edit::Move {
                    from: Alloc::Stack(slot),
                    to: Alloc::Reg(preg),
                },
            ));
        }
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

    /// Pool size 2, three live values → one must spill.
    ///
    /// ```text
    /// i0 = IConst32 1   ; v0
    /// i1 = IConst32 2   ; v1
    /// i2 = IConst32 3   ; v2
    /// i3 = Add32 i0, i1 ; v3 = v0+v1  (evicts v2)
    /// i4 = Add32 i3, i2 ; v4 = v3+v2  (v2 must reload from spill)
    /// Ret i4
    /// ```
    ///
    /// Correct result: (1+2)+(3) = 6.
    /// Before the fix, the eviction emitted a save-before instead of a
    /// reload-after, which stored the wrong register contents to the spill
    /// slot.
    #[test]
    fn walk_spill_pool2_eviction_reload() {
        let input = "\
            i0 = IConst32 1\n\
            i1 = IConst32 2\n\
            i2 = IConst32 3\n\
            i3 = Add32 i0, i1\n\
            i4 = Add32 i3, i2\n\
            Ret i4";
        let (vinsts, _symbols, pool) = vinst::parse(input).unwrap();
        let output =
            walk_linear_with_pool(&vinsts, &pool, &make_abi(), RegPool::with_capacity(2)).unwrap();

        // v2 must be spilled (only 2 regs, 3 live values at inst 3)
        assert!(
            output.num_spill_slots >= 1,
            "expected at least 1 spill slot"
        );

        // v2's def (inst 2) must go to Stack (because it was evicted)
        let v2_def_alloc = output.operand_alloc(2, 0);
        assert!(
            v2_def_alloc.is_stack(),
            "v2 def should be Stack, got {:?}",
            v2_def_alloc
        );

        // There must be an After(3) reload edit: Stack → Reg
        let has_after3_reload = output.edits.iter().any(|(pt, edit)| {
            *pt == EditPoint::After(3)
                && matches!(
                    edit,
                    Edit::Move {
                        from: Alloc::Stack(_),
                        to: Alloc::Reg(_)
                    }
                )
        });
        assert!(
            has_after3_reload,
            "expected After(3) reload edit (stack→reg), got edits: {:?}",
            output.edits
        );

        // v2's use at inst 4 must be Reg (reloaded)
        let v2_use_at_4 = output.operand_alloc(4, 2); // def=0, use0=1, use1=2
        assert!(
            v2_use_at_4.is_reg(),
            "v2 use at inst 4 should be Reg, got {:?}",
            v2_use_at_4
        );

        // Edits must be sorted
        for w in output.edits.windows(2) {
            assert!(
                w[0].0 <= w[1].0,
                "edits not sorted: {:?} > {:?}",
                w[0],
                w[1]
            );
        }
    }
}
