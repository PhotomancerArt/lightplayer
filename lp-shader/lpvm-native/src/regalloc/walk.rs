//! Backward walk allocator: [`RegionTree`] dispatch with spill-at-boundary.
//!
//! Walks instructions in reverse order, allocating registers for uses,
//! freeing registers for defs, and recording spill/reload edits.

use crate::abi::FuncAbi;
use crate::regalloc::pool::RegPool;
use crate::regalloc::spill::SpillAlloc;
use crate::regalloc::trace::TraceEntry;
use crate::regalloc::{
    Alloc, AllocError, AllocOutput, Edit, EditPoint, TracePush, TraceSink, trace_sink_new,
};
use crate::region::{REGION_ID_NONE, Region, RegionId, RegionTree};
use crate::regset::RegSet;
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
        Region::Block { body, .. } => region_first_vinst(tree, *body),
    }
}

/// Identify entry-parameter vregs that are exclusively used as call args at the
/// same ABI position they arrive in. These can stay in their ABI register
/// without ever entering the pool, eliminating entry_move + arg_move overhead.
///
/// Returns a map from vreg index → entry ABI register for eligible vregs.
fn build_passthrough_set(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &FuncAbi,
) -> Vec<Option<u8>> {
    let max_vreg = vreg_pool.iter().map(|v| v.0).max().unwrap_or(0) as usize;
    let mut passthrough: Vec<Option<u8>> = vec![None; max_vreg + 1];
    let mut disqualified = vec![false; max_vreg + 1];

    for &(vreg_idx, preg) in func_abi.precolors() {
        let idx = vreg_idx as usize;
        if idx < passthrough.len() {
            passthrough[idx] = Some(preg.hw);
        }
    }

    for inst in vinsts {
        match inst {
            VInst::Call {
                args,
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                ..
            } => {
                let call_args = args.vregs(vreg_pool);
                let isa = func_abi.isa();
                for (i, &arg_vreg) in call_args.iter().enumerate() {
                    let idx = arg_vreg.0 as usize;
                    if idx >= passthrough.len() || disqualified[idx] || passthrough[idx].is_none() {
                        continue;
                    }
                    let entry_reg = passthrough[idx].unwrap();
                    let Some(target) = isa.lpir_call_arg_target_hw(
                        *callee_uses_sret,
                        *caller_passes_sret_ptr,
                        *caller_sret_vm_abi_swap,
                        i,
                    ) else {
                        disqualified[idx] = true;
                        continue;
                    };
                    if entry_reg != target {
                        disqualified[idx] = true;
                    }
                }
            }
            other => {
                other.for_each_use(vreg_pool, |use_vreg| {
                    let idx = use_vreg.0 as usize;
                    if idx < disqualified.len() {
                        disqualified[idx] = true;
                    }
                });
            }
        }
    }

    for (idx, dq) in disqualified.iter().enumerate() {
        if *dq && idx < passthrough.len() {
            passthrough[idx] = None;
        }
    }
    passthrough
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
    let mut max_vreg_idx = vreg_pool.iter().map(|v| v.0).max().unwrap_or(0) as usize;
    for inst in vinsts {
        inst.for_each_vreg_touching(vreg_pool, |v| {
            max_vreg_idx = max_vreg_idx.max(v.0 as usize);
        });
    }
    let max_vreg_idx = max_vreg_idx + 32;
    let passthrough = build_passthrough_set(vinsts, vreg_pool, func_abi);
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
        passthrough,
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
    /// Entry-parameter vregs that stay in their ABI register (never enter pool).
    /// Indexed by vreg index; `Some(hw)` = passthrough to that ABI register.
    passthrough: Vec<Option<u8>>,
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
                    // Anchor at else_body's own start (the jump target), not
                    // then_body's. Placing these reloads inside the else path
                    // prevents them from executing in the fallthrough path.
                    if let Some(anchor) = region_first_vinst(self.tree, *else_body) {
                        self.boundary_reload_before(anchor);
                    }
                }
                self.walk_region(*then_body)?;
                // Anchor at then_body's own start (the fallthrough), not
                // head's. Placing reloads inside the fallthrough path prevents
                // them from clobbering the BrIf condition register.
                if let Some(anchor) = region_first_vinst(self.tree, *then_body) {
                    self.boundary_reload_before(anchor);
                }
                self.walk_region(*head)?;
                Ok(())
            }
            Region::Block { body, .. } => {
                if *body != REGION_ID_NONE {
                    self.walk_region(*body)?;
                    if let Some(anchor) = region_first_vinst(self.tree, *body) {
                        self.boundary_reload_before(anchor);
                    }
                }
                Ok(())
            }
            Region::Loop { header, body, .. } => {
                if *body != REGION_ID_NONE {
                    // Pre-assign spill slots for loop-carried values so that
                    // defs inside the body write directly to the slot. The
                    // back-edge Br is a no-op; without pre-assignment the
                    // updated value would never reach the spill slot and the
                    // next iteration's header reload would read stale data.
                    let body_live = crate::regalloc::liveness::analyze_liveness(
                        self.tree,
                        *body,
                        self.vinsts,
                        self.vreg_pool,
                    );
                    // Only values *defined* inside the loop need spill-at-boundary / loop_carried
                    // treatment. Parameters (and other loop-invariant inputs) are live into the body
                    // but must not get a spill slot here — reload-before-first-use would read garbage.
                    let defs_in_loop = crate::regalloc::liveness::defs_in_region(
                        self.tree,
                        *body,
                        self.vinsts,
                        self.vreg_pool,
                    )
                    .union(&crate::regalloc::liveness::defs_in_region(
                        self.tree,
                        *header,
                        self.vinsts,
                        self.vreg_pool,
                    ));
                    for vreg in body_live.live_in.iter() {
                        // Parameters (and vmctx) appear as defs in lowered IR (`v = copy v`) inside the
                        // loop body range, but they are not carried across iterations — skip them so we
                        // do not assign spill slots that get reloaded before the entry move has stored.
                        if self.func_abi.precolor_of(vreg.0 as u32).is_some() {
                            continue;
                        }
                        if defs_in_loop.contains(vreg) {
                            self.spill.get_or_assign(vreg);
                            self.loop_carried.insert(vreg);
                        }
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
                    self.func_abi,
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
                    &self.passthrough,
                );
            } else {
                process_generic(
                    inst,
                    inst_idx,
                    inst_idx_u16,
                    offset,
                    self.vreg_pool,
                    self.func_abi,
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
        let occupied: Vec<(u8, VReg)> = self.pool.iter_occupied().collect();
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

        let mut entry_precolors: Vec<(VReg, u8)> = Vec::new();
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
                        decision: alloc::format!("x{abi_reg} -> x{final_preg}"),
                        register_state: String::new(),
                    },
                );
                // Spill slots can be assigned during the backward walk (e.g. for a later call) before
                // any instruction stores the live value. A `Before(0)` reload would otherwise read
                // garbage; mirror the incoming register into the slot so early reloads match the ABI.
                if let Some(slot) = self.spill.has_slot(vreg) {
                    entry_edits.push((
                        EditPoint::Before(0),
                        Edit::Move {
                            from: Alloc::Reg(final_preg),
                            to: Alloc::Stack(slot),
                        },
                    ));
                    TracePush::push(
                        &mut self.trace,
                        TraceEntry {
                            vinst_idx: 0,
                            vinst_mnemonic: String::from("entry_slot_init"),
                            decision: alloc::format!("x{final_preg} -> slot{slot}"),
                            register_state: String::new(),
                        },
                    );
                }
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
                        decision: alloc::format!("x{abi_reg} -> slot{slot}"),
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
                            decision: alloc::format!("[fp+{offset}] -> x{final_preg}"),
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
                            decision: alloc::format!("[fp+{offset}] -> slot{slot}"),
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
    walk_linear_with_pool(vinsts, vreg_pool, func_abi, RegPool::new(func_abi.isa()))
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
    func_abi: &FuncAbi,
    pool: &mut RegPool,
    spill: &mut SpillAlloc,
    allocs: &mut [Alloc],
    edits: &mut Vec<(EditPoint, Edit)>,
    trace: &mut TraceSink,
) {
    // Special case: Mov is a copy. Coalesce src and dst to the same register
    // to eliminate the move at emission time (emitter skips addi rd, rs, 0 when rd==rs).
    if let VInst::Mov { dst, src, .. } = inst {
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
                // If src was evicted by a call clobber and has a spill slot,
                // the After(reload) expects the value in the slot. Since we're
                // coalescing src back into a register, emit a store-after-def
                // so the slot gets the value too.
                if let Some(slot) = spill.has_slot(*src) {
                    edits.push((
                        EditPoint::After(inst_idx_u16),
                        Edit::Move {
                            from: Alloc::Reg(preg),
                            to: Alloc::Stack(slot),
                        },
                    ));
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
            // dst may also have a spill slot (assigned by a later eviction in
            // the backward walk). Store the register to that slot so reloads
            // find the correct value — same logic as process_generic's
            // def_spill_stores, but for the coalesced Mov path.
            if let Some(slot) = spill.has_slot(*dst) {
                edits.push((
                    EditPoint::After(inst_idx_u16),
                    Edit::Move {
                        from: Alloc::Reg(preg),
                        to: Alloc::Stack(slot),
                    },
                ));
            }
        } else {
            // Dst is spilled or dead: use normal allocation path for src
            allocs[use_idx] = alloc_use(*src, inst_idx, inst_idx_u16, pool, spill, edits, trace);
        }
        return;
    }

    let mut operand_idx: usize = 0;
    let mut def_spill_stores: Vec<(EditPoint, Edit)> = Vec::new();

    // Defs (backward: freed)
    inst.for_each_def(vreg_pool, |def_vreg| {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;

        let preg_home = pool.home(def_vreg);
        let slot = spill.has_slot(def_vreg);

        let alloc = if let Some(preg) = preg_home {
            Alloc::Reg(preg)
        } else if let Some(slot) = slot {
            Alloc::Stack(slot)
        } else {
            Alloc::None
        };

        allocs[alloc_idx] = alloc;

        // When a def writes to a register but the vreg also has a spill
        // slot (assigned by a later eviction in the backward walk), store
        // the register value to the slot so that any reload-from-slot
        // (clobber restore, eviction reload, etc.) finds the correct data.
        if let (Some(preg), Some(slot)) = (preg_home, slot) {
            def_spill_stores.push((
                EditPoint::After(inst_idx_u16),
                Edit::Move {
                    from: Alloc::Reg(preg),
                    to: Alloc::Stack(slot),
                },
            ));
        }

        if let Some(preg) = preg_home {
            pool.free(preg);
        }
    });

    // Uses (backward: allocated)
    // Sret Ret: force ALL operands to Alloc::Stack (regalloc2-style Stack constraint).
    // The emitter loads each into TEMP0 and stores to the sret buffer sequentially,
    // so no register conflicts are possible. This eliminates the entire class of
    // Ret operand collisions where later operands evict earlier ones.
    let is_sret_ret = matches!(
        inst,
        VInst::Ret { vals, .. } if func_abi.isa().sret_uses_buffer_for(vals.count as u32)
    );
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

    // Pushed after uses so that after global reverse, def stores come
    // before any After(reload) from handle_eviction — ensuring the slot
    // is written before it can be overwritten by an eviction reload to
    // the same physical register.
    edits.extend(def_spill_stores);
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
                decision: alloc::format!("slot{slot} -> t{new_preg}"),
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
    preg: u8,
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
                decision: alloc::format!("slot{slot} -> t{preg}"),
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
    func_abi: &FuncAbi,
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
    passthrough: &[Option<u8>],
) {
    let isa = func_abi.isa();
    let (args_slice, rets_slice, callee_uses_sret, caller_passes_sret_ptr, caller_sret_vm_abi_swap) =
        match inst {
            VInst::Call {
                args,
                rets,
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                ..
            } => (
                *args,
                *rets,
                *callee_uses_sret,
                *caller_passes_sret_ptr,
                *caller_sret_vm_abi_swap,
            ),
            _ => unreachable!(),
        };

    let args = args_slice.vregs(vreg_pool);
    let rets = rets_slice.vregs(vreg_pool);

    // Collect edits in forward order; we'll push in reverse for the backward walk.
    // All edits go into local vectors — nothing is pushed to the global `edits`
    // until the end, so we have full control over forward-order sequencing.
    let mut before_arg_moves: Vec<(EditPoint, Edit)> = Vec::new();
    let mut after_ret_moves: Vec<(EditPoint, Edit)> = Vec::new();
    let mut after_restores: Vec<(EditPoint, Edit)> = Vec::new();

    // Track pool registers that receive ret_move targets.  After(call)
    // eviction restores must NOT target these, or they overwrite the return
    // value (regalloc2 avoids this by removing clobbers from available_pregs
    // before operand allocation; we filter at restore-emit time).
    let mut ret_move_pool_regs: Vec<u8> = Vec::new();

    // ── Step 1: Defs (return values) ──
    let mut operand_idx: usize = 0;
    for (i, &ret_vreg) in rets.iter().enumerate() {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;

        if callee_uses_sret || i >= isa.direct_ret_reg_count() {
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

        let target = isa
            .direct_ret_reg_hw(i)
            .expect("ret slot within direct_ret_reg_count");

        allocs[alloc_idx] = Alloc::Reg(target);

        if let Some(pool_reg) = pool.home(ret_vreg) {
            ret_move_pool_regs.push(pool_reg);
            after_ret_moves.push((
                EditPoint::After(inst_idx_u16),
                Edit::Move {
                    from: Alloc::Reg(target),
                    to: Alloc::Reg(pool_reg),
                },
            ));
            if let Some(slot) = spill.has_slot(ret_vreg) {
                after_ret_moves.push((
                    EditPoint::After(inst_idx_u16),
                    Edit::Move {
                        from: Alloc::Reg(pool_reg),
                        to: Alloc::Stack(slot),
                    },
                ));
            }
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
    }

    // ── Step 2: Evict-then-reload for caller-saved pool t-regs ──
    // regalloc2-style: evict clobbered-reg occupants from the pool and remove
    // the registers from the LRU so they can't be reused during arg allocation
    // (matches regalloc2's remove_clobbers_from_available_pregs). Emit only
    // post-call reloads, no pre-call saves.
    let clobbered: Vec<(u8, VReg)> = isa
        .caller_saved_pool_hw()
        .iter()
        .filter_map(|&preg| {
            pool.iter_occupied()
                .find(|&(p, _)| p == preg)
                .map(|(_, v)| (preg, v))
        })
        .collect();
    let mut clobbered_pregs: Vec<u8> = Vec::new();
    for (preg, vreg) in &clobbered {
        let slot = spill.get_or_assign(*vreg);
        pool.evict(*preg);
        clobbered_pregs.push(*preg);
        after_restores.push((
            EditPoint::After(inst_idx_u16),
            Edit::Move {
                from: Alloc::Stack(slot),
                to: Alloc::Reg(*preg),
            },
        ));
        TracePush::push(
            trace,
            TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("clobber_evict"),
                decision: alloc::format!("v{} evicted from x{} -> slot{}", vreg.0, preg, slot),
                register_state: String::new(),
            },
        );
    }

    // ── Step 3: Uses (arguments) ──
    //
    // Two-phase allocation (regalloc2-style):
    //   Phase A — ensure every arg vreg has a pool register.  Track
    //             register-pass arg targets but do NOT emit Before moves yet.
    //   Phase B — generate Before(call) moves using each vreg's FINAL
    //             pool/spill location, which reflects all evictions from
    //             phase A (including evictions caused by stack-pass arg
    //             allocation).
    //
    // All eviction restores go into `after_restores` (not the global `edits`
    // vector) so they can be filtered against ret_move_pool_regs and
    // sequenced correctly relative to ret_moves.

    // (vreg, target_arg_reg) for register-pass args — Before moves deferred.
    let mut reg_pass_args: Vec<(VReg, u8)> = Vec::new();

    // ── Phase A: allocate every arg vreg into the pool ──
    for (i, &arg_vreg) in args.iter().enumerate() {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;

        let target_opt = isa.lpir_call_arg_target_hw(
            callee_uses_sret,
            caller_passes_sret_ptr,
            caller_sret_vm_abi_swap,
            i,
        );
        let is_reg_pass = target_opt.is_some();
        let trace_target = target_opt.unwrap_or(0);
        if let Some(target) = target_opt {
            // Pass-through shortcut: vreg stays in its ABI register, no pool needed.
            let is_passthrough = passthrough
                .get(arg_vreg.0 as usize)
                .copied()
                .flatten()
                .is_some_and(|entry_reg| entry_reg == target);
            if is_passthrough {
                allocs[alloc_idx] = Alloc::Reg(target);
                TracePush::push(
                    trace,
                    TraceEntry {
                        vinst_idx: inst_idx,
                        vinst_mnemonic: String::from("call_arg"),
                        decision: alloc::format!("v{}: x{} (passthrough)", arg_vreg.0, target),
                        register_state: String::new(),
                    },
                );
                operand_idx += 0; // already incremented
                continue;
            }

            reg_pass_args.push((arg_vreg, target));
            allocs[alloc_idx] = Alloc::Reg(target);
        }

        if let Some(pool_reg) = pool.home(arg_vreg) {
            pool.touch(pool_reg);
            if !is_reg_pass {
                allocs[alloc_idx] = Alloc::Reg(pool_reg);
            }
        } else if let Some(slot) = spill.has_slot(arg_vreg) {
            let (new_preg, evicted) = pool.alloc(arg_vreg);
            if let Some(ev) = evicted {
                let ev_slot = spill.get_or_assign(ev);
                if !ret_move_pool_regs.contains(&new_preg) {
                    after_restores.push((
                        EditPoint::After(inst_idx_u16),
                        Edit::Move {
                            from: Alloc::Stack(ev_slot),
                            to: Alloc::Reg(new_preg),
                        },
                    ));
                }
                TracePush::push(
                    trace,
                    TraceEntry {
                        vinst_idx: inst_idx,
                        vinst_mnemonic: String::from("evict"),
                        decision: alloc::format!("x{} -> slot{} (v{})", new_preg, ev_slot, ev.0),
                        register_state: String::new(),
                    },
                );
            }
            before_arg_moves.push((
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
                    decision: alloc::format!("slot{} -> x{} (v{})", slot, new_preg, arg_vreg.0),
                    register_state: String::new(),
                },
            );
            if !is_reg_pass {
                allocs[alloc_idx] = Alloc::Reg(new_preg);
            }
        } else {
            let (new_preg, evicted) = pool.alloc(arg_vreg);
            if let Some(ev) = evicted {
                let ev_slot = spill.get_or_assign(ev);
                if !ret_move_pool_regs.contains(&new_preg) {
                    after_restores.push((
                        EditPoint::After(inst_idx_u16),
                        Edit::Move {
                            from: Alloc::Stack(ev_slot),
                            to: Alloc::Reg(new_preg),
                        },
                    ));
                }
                TracePush::push(
                    trace,
                    TraceEntry {
                        vinst_idx: inst_idx,
                        vinst_mnemonic: String::from("evict"),
                        decision: alloc::format!("x{} -> slot{} (v{})", new_preg, ev_slot, ev.0),
                        register_state: String::new(),
                    },
                );
            }
            if !is_reg_pass {
                allocs[alloc_idx] = Alloc::Reg(new_preg);
            }
        }

        TracePush::push(
            trace,
            TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("call_arg"),
                decision: if is_reg_pass {
                    alloc::format!("v{}: pool -> x{} (deferred)", arg_vreg.0, trace_target)
                } else {
                    alloc::format!(
                        "v{}: x{} (stack-pass)",
                        arg_vreg.0,
                        pool.home(arg_vreg).unwrap_or(0)
                    )
                },
                register_state: String::new(),
            },
        );
    }

    // ── Phase B: emit Before(call) moves for register-pass args ──
    // The pool now reflects the final allocation state after all evictions.
    for &(arg_vreg, target) in &reg_pass_args {
        if let Some(pool_reg) = pool.home(arg_vreg) {
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
                    vinst_mnemonic: String::from("call_arg_move"),
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
                    vinst_mnemonic: String::from("call_arg_move"),
                    decision: alloc::format!("v{}: slot{} -> x{}", arg_vreg.0, slot, target),
                    register_state: String::new(),
                },
            );
        }
    }

    // Restore clobbered registers to the LRU now that arg allocation is done.
    pool.restore_evicted(&clobbered_pregs);

    // Push edits in reverse-forward order (global reverse will restore forward order).
    // Desired forward: Before(arg_moves), After(ret_moves), After(restores)
    // Push order:      After(restores), After(ret_moves), Before(arg_moves)
    //
    // ret_moves come before restores in forward order so that the return value
    // lands in its pool register before any eviction restores run.  Eviction
    // restores that target a ret_move pool register are already filtered out
    // above, but sequencing ret_moves first is an extra safety net.
    for e in after_restores.into_iter().rev() {
        edits.push(e);
    }
    for e in after_ret_moves.into_iter().rev() {
        edits.push(e);
    }
    for e in before_arg_moves.into_iter().rev() {
        edits.push(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debug::vinst;
    use crate::regalloc::test::abi_fixtures;

    fn make_abi() -> FuncAbi {
        abi_fixtures::void_func_abi()
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
        let input = "i0 = IConst32 10\ni1 = IConst32 20\ni2 = Add i0, i1\nRet i2";
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
    /// i3 = Add i0, i1 ; v3 = v0+v1  (evicts v2)
    /// i4 = Add i3, i2 ; v4 = v3+v2  (v2 must reload from spill)
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
            i3 = Add i0, i1\n\
            i4 = Add i3, i2\n\
            Ret i4";
        let (vinsts, _symbols, pool) = vinst::parse(input).unwrap();
        let output = walk_linear_with_pool(
            &vinsts,
            &pool,
            &make_abi(),
            RegPool::with_capacity(crate::isa::IsaTarget::Rv32imac, 2),
        )
        .unwrap();

        // v2 must be spilled (only 2 regs, 3 live values at inst 3)
        assert!(
            output.num_spill_slots >= 1,
            "expected at least 1 spill slot"
        );

        // v2's def (inst 2) must go to Stack (because it was evicted)
        let v2_def_alloc = output.operand_alloc(2, 0);
        assert!(
            v2_def_alloc.is_stack(),
            "v2 def should be Stack, got {v2_def_alloc:?}",
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
            "expected After(3) reload edit (stack→reg), got edits: {edits:?}",
            edits = output.edits,
        );

        // v2's use at inst 4 must be Reg (reloaded)
        let v2_use_at_4 = output.operand_alloc(4, 2); // def=0, use0=1, use1=2
        assert!(
            v2_use_at_4.is_reg(),
            "v2 use at inst 4 should be Reg, got {v2_use_at_4:?}",
        );

        // Edits must be sorted
        for w in output.edits.windows(2) {
            assert!(
                w[0].0 <= w[1].0,
                "edits not sorted: {a:?} > {b:?}",
                a = w[0],
                b = w[1],
            );
        }
    }
}
