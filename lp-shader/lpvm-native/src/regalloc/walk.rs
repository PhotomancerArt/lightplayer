//! Backward walk allocator: [`RegionTree`] dispatch with spill-at-boundary.
//!
//! Walks instructions in reverse order, allocating registers for uses,
//! freeing registers for defs, and recording spill/reload edits.

use crate::abi::{FuncAbi, PReg, RegClass};
use crate::regalloc::classes::VRegClasses;
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

/// Per-instruction operand offsets into the flat `allocs` table (global
/// indices), plus every vreg's register class.
///
/// The classes ride along with the operand count rather than getting a pass of
/// their own: this loop already visits every def, and `for_each_def` is generic
/// over its callback, so a second walk would cost a second monomorphization of
/// a match over the entire instruction set — real flash in a compiler that
/// runs on the device.
pub(crate) fn build_operand_layout(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
) -> (Vec<u16>, usize, VRegClasses) {
    let mut inst_alloc_offsets = Vec::with_capacity(vinsts.len());
    let mut total_operands: usize = 0;
    let mut classes = VRegClasses::new();
    for inst in vinsts {
        inst_alloc_offsets.push(total_operands as u16);
        let mut num_operands: usize = 0;
        let mut def_idx: usize = 0;
        inst.for_each_def(vreg_pool, |def| {
            classes.record_def(def, crate::regalloc::classes::def_class(inst, def_idx));
            def_idx += 1;
            num_operands += 1;
        });
        inst.for_each_use(vreg_pool, |_use| num_operands += 1);
        total_operands += num_operands;
    }
    (inst_alloc_offsets, total_operands, classes)
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
    classes: &VRegClasses,
) -> Vec<Option<PReg>> {
    let max_vreg = vreg_pool.iter().map(|v| v.0).max().unwrap_or(0) as usize;
    let mut passthrough: Vec<Option<PReg>> = vec![None; max_vreg + 1];
    let mut disqualified = vec![false; max_vreg + 1];

    for &(vreg_idx, preg) in func_abi.precolors() {
        let idx = vreg_idx as usize;
        if idx < passthrough.len() {
            passthrough[idx] = Some(preg);
        }
    }

    // The entry value survives in its ABI register only until the first
    // call executes: a call's argument moves and the (caller-saved) callee
    // clobber the argument registers. Args of any call after the first in
    // the stream are disqualified. A back-edge means even the first call
    // can execute twice — its second iteration would read a register the
    // first iteration's callee clobbered — so any loop disqualifies all
    // call args. (Stream order is layout order for structured control
    // flow, so an earlier-stream call can only run after a later-stream
    // one via a back-edge.)
    let mut seen_labels: Vec<crate::vinst::LabelId> = Vec::new();
    let mut has_back_edge = false;
    for inst in vinsts {
        match inst {
            VInst::Label(id, _) => seen_labels.push(*id),
            VInst::Br { target, .. } | VInst::BrIf { target, .. } => {
                if seen_labels.contains(target) {
                    has_back_edge = true;
                }
            }
            _ => {}
        }
    }

    let mut seen_call = false;
    for inst in vinsts {
        match inst {
            VInst::Call {
                args,
                callee_uses_sret,
                caller_passes_sret_ptr,
                caller_sret_vm_abi_swap,
                ..
            } => {
                let clobbered_by_prior_call = seen_call || has_back_edge;
                seen_call = true;
                let call_args = args.vregs(vreg_pool);
                let isa = func_abi.isa();
                for (i, &arg_vreg) in call_args.iter().enumerate() {
                    let idx = arg_vreg.0 as usize;
                    if idx >= passthrough.len() || disqualified[idx] || passthrough[idx].is_none() {
                        continue;
                    }
                    if clobbered_by_prior_call {
                        disqualified[idx] = true;
                        continue;
                    }
                    let entry_reg = passthrough[idx].unwrap();
                    let Some(target) = isa.lpir_call_arg_target(
                        classes.of(arg_vreg),
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
    let (inst_alloc_offsets, total_operands, classes) = build_operand_layout(vinsts, vreg_pool);
    let mut max_vreg_idx = vreg_pool.iter().map(|v| v.0).max().unwrap_or(0) as usize;
    for inst in vinsts {
        inst.for_each_vreg_touching(vreg_pool, |v| {
            max_vreg_idx = max_vreg_idx.max(v.0 as usize);
        });
    }
    let max_vreg_idx = max_vreg_idx + 32;
    let passthrough = build_passthrough_set(vinsts, vreg_pool, func_abi, &classes);
    let mut state = WalkState {
        vinsts,
        vreg_pool,
        func_abi,
        tree,
        inst_alloc_offsets,
        pool,
        classes,
        spill: SpillAlloc::new(max_vreg_idx + 16),
        allocs: vec![Alloc::None; total_operands],
        edits: Vec::new(),
        trace: trace_sink_new(),
        loop_carried: RegSet::new(),
        passthrough,
        call_scratch: CallScratch::default(),
    };
    state.walk_region(root)?;
    state.finish()
}

/// Reusable scratch buffers for [`process_call`], owned by [`WalkState`] and
/// cleared per call. In Q32 saturating mode nearly every float op lowers to a
/// call, so fresh per-call `Vec`s were a measurable allocator-churn source;
/// with reuse the steady state allocates nothing.
#[derive(Default)]
struct CallScratch {
    before_arg_moves: Vec<(EditPoint, Edit)>,
    after_ret_moves: Vec<(EditPoint, Edit)>,
    after_restores: Vec<(EditPoint, Edit)>,
    ret_value_pool_regs: Vec<PReg>,
    clobbered: Vec<(PReg, VReg)>,
    clobbered_pregs: Vec<PReg>,
    reg_pass_args: Vec<(VReg, PReg)>,
    stack_pass_args: Vec<(usize, VReg)>,
}

impl CallScratch {
    fn clear(&mut self) {
        self.before_arg_moves.clear();
        self.after_ret_moves.clear();
        self.after_restores.clear();
        self.ret_value_pool_regs.clear();
        self.clobbered.clear();
        self.clobbered_pregs.clear();
        self.reg_pass_args.clear();
        self.stack_pass_args.clear();
    }
}

struct WalkState<'a> {
    vinsts: &'a [VInst],
    vreg_pool: &'a [VReg],
    func_abi: &'a FuncAbi,
    tree: &'a RegionTree,
    inst_alloc_offsets: Vec<u16>,
    pool: RegPool,
    /// Register class of every vreg — the class every pool and spill-slot
    /// decision below is made for.
    classes: VRegClasses,
    spill: SpillAlloc,
    allocs: Vec<Alloc>,
    edits: Vec<(EditPoint, Edit)>,
    trace: TraceSink,
    /// VRegs that are loop-carried: defs to registers get a store-after-def
    /// edit so the spill slot always has the latest value at sub-boundaries.
    loop_carried: RegSet,
    /// Entry-parameter vregs that stay in their ABI register (never enter pool).
    /// Indexed by vreg index; `Some(preg)` = passthrough to that ABI register.
    passthrough: Vec<Option<PReg>>,
    /// Reused by [`process_call`]; see [`CallScratch`].
    call_scratch: CallScratch,
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
                let count = *child_count as usize;
                // Copy one child id per iteration instead of collecting the
                // range into a Vec (self.tree is borrowed shared, so indexing
                // per-iteration is fine and allocation-free).
                for idx in (0..count).rev() {
                    let child = self.tree.seq_children[s + idx];
                    self.walk_region(child)?;
                    if idx > 0 {
                        if let Some(anchor) = region_first_vinst(self.tree, child) {
                            self.boundary_reload_before(anchor)?;
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
                        self.boundary_reload_before(anchor)?;
                    }
                }
                self.walk_region(*then_body)?;
                // Anchor at then_body's own start (the fallthrough), not
                // head's. Placing reloads inside the fallthrough path prevents
                // them from clobbering the BrIf condition register.
                if let Some(anchor) = region_first_vinst(self.tree, *then_body) {
                    self.boundary_reload_before(anchor)?;
                }
                self.walk_region(*head)?;
                Ok(())
            }
            Region::Block { body, .. } => {
                if *body != REGION_ID_NONE {
                    self.walk_region(*body)?;
                    if let Some(anchor) = region_first_vinst(self.tree, *body) {
                        self.boundary_reload_before(anchor)?;
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
                            self.spill.get_or_assign(vreg, self.classes.of(vreg))?;
                            self.loop_carried.insert(vreg);
                        }
                    }

                    self.walk_region(*body)?;
                    if let Some(anchor) = region_first_vinst(self.tree, *body) {
                        self.boundary_reload_before(anchor)?;
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
                    &self.classes,
                    &mut self.spill,
                    &mut self.allocs,
                    &mut self.edits,
                    &mut self.trace,
                    &self.passthrough,
                    &mut self.call_scratch,
                )?;
            } else {
                process_generic(
                    inst,
                    inst_idx,
                    inst_idx_u16,
                    offset,
                    self.vreg_pool,
                    self.func_abi,
                    &mut self.pool,
                    &self.classes,
                    &mut self.spill,
                    &mut self.allocs,
                    &mut self.edits,
                    &mut self.trace,
                )?;
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
                        if let Some(preg) = self.allocs[def_idx].preg() {
                            if let Some(slot) = self.spill.has_slot(def_vreg) {
                                self.edits.push((
                                    EditPoint::After(inst_idx_u16),
                                    Edit::Move {
                                        from: Alloc::reg(preg),
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
    fn boundary_reload_before(&mut self, anchor: u16) -> Result<(), AllocError> {
        // Snapshot into a fixed buffer (pool holds at most 32 hardware regs)
        // so the loop can mutate pool/spill/edits without a heap allocation
        // per region boundary.
        // 64 = 32 hardware registers per class, two classes — the ceiling on
        // what `iter_occupied` can yield.
        let mut occupied = [(PReg::int(0), VReg(0)); 64];
        let mut n = 0;
        for entry in self.pool.iter_occupied() {
            occupied[n] = entry;
            n += 1;
        }
        for &(preg, vreg) in &occupied[..n] {
            // The slot's class is the register's class: this reload writes
            // back into the same file the value was spilled from.
            let slot = self.spill.get_or_assign(vreg, preg.class)?;
            self.edits.push((
                EditPoint::Before(anchor),
                Edit::Move {
                    from: Alloc::Stack(slot),
                    to: Alloc::reg(preg),
                },
            ));
            self.pool.free(preg);
        }
        Ok(())
    }

    fn finish(mut self) -> Result<AllocOutput, AllocError> {
        self.edits.reverse();
        stable_sort_edits_by_point(&mut self.edits);

        let mut entry_precolors: Vec<(VReg, PReg)> = Vec::new();
        for (vreg_idx, preg) in self.func_abi.precolors() {
            let vreg = VReg(*vreg_idx as u16);
            entry_precolors.push((vreg, *preg));
        }

        // Entry parameter setup, built in four dependency-ordered groups.
        //
        // The incoming ABI parameter registers are *read* by the first two
        // groups and *written* by the last two. On an ISA whose allocatable
        // pool overlaps its parameter registers — Xtensa: pool `a2..a7` +
        // `a10..a15`, callee params `a2..a7` — a naive interleaving lets one
        // parameter's register be overwritten before its own move reads it,
        // and the function then computes on a duplicate. Grouping, plus
        // `sequence_arg_moves` on the reg→reg group, is what keeps "every
        // parameter reaches its home carrying its incoming value" true.
        // rv32 is unaffected either way (pool 18..31 vs params 10..17 are
        // disjoint), so this is a no-op there.
        let mut entry_spills: Vec<(EditPoint, Edit)> = Vec::new();
        let mut pending_entry: Vec<(Alloc, PReg)> = Vec::new();
        let mut slot_inits: Vec<(EditPoint, Edit)> = Vec::new();
        let mut stack_arg_loads: Vec<(EditPoint, Edit)> = Vec::new();

        for (vreg, abi_reg) in entry_precolors {
            if let Some(final_preg) = self.pool.home(vreg) {
                if final_preg != abi_reg {
                    pending_entry.push((Alloc::reg(abi_reg), final_preg));
                }
                TracePush::push_with(&mut self.trace, || TraceEntry {
                    vinst_idx: 0,
                    vinst_mnemonic: String::from("entry_move"),
                    decision: alloc::format!("x{} -> x{}", abi_reg.hw, final_preg.hw),
                    register_state: String::new(),
                });
                // Spill slots can be assigned during the backward walk (e.g. for a later call) before
                // any instruction stores the live value. A `Before(0)` reload would otherwise read
                // garbage; mirror the incoming register into the slot so early reloads match the ABI.
                // Reads the move's *destination*, so it runs after every move has landed.
                if let Some(slot) = self.spill.has_slot(vreg) {
                    slot_inits.push((
                        EditPoint::Before(0),
                        Edit::Move {
                            from: Alloc::reg(final_preg),
                            to: Alloc::Stack(slot),
                        },
                    ));
                    TracePush::push_with(&mut self.trace, || TraceEntry {
                        vinst_idx: 0,
                        vinst_mnemonic: String::from("entry_slot_init"),
                        decision: alloc::format!("x{} -> slot{slot}", final_preg.hw),
                        register_state: String::new(),
                    });
                }
            } else if let Some(slot) = self.spill.has_slot(vreg) {
                // Reads an incoming register and writes memory, so it must run
                // before any move can overwrite that register.
                entry_spills.push((
                    EditPoint::Before(0),
                    Edit::Move {
                        from: Alloc::reg(abi_reg),
                        to: Alloc::Stack(slot),
                    },
                ));
                TracePush::push_with(&mut self.trace, || TraceEntry {
                    vinst_idx: 0,
                    vinst_mnemonic: String::from("entry_spill"),
                    decision: alloc::format!("x{} -> slot{slot}", abi_reg.hw),
                    register_state: String::new(),
                });
            }
        }

        for (vreg_idx, loc) in self.func_abi.param_locs().iter().enumerate() {
            if let crate::abi::classify::ArgLoc::Stack { offset, .. } = loc {
                let vreg = VReg(vreg_idx as u16);
                if let Some(final_preg) = self.pool.home(vreg) {
                    stack_arg_loads.push((
                        EditPoint::Before(0),
                        Edit::LoadIncomingArg {
                            fp_offset: *offset,
                            to: Alloc::reg(final_preg),
                        },
                    ));
                    TracePush::push_with(&mut self.trace, || TraceEntry {
                        vinst_idx: 0,
                        vinst_mnemonic: String::from("entry_load_stack_arg"),
                        decision: alloc::format!("[fp+{offset}] -> x{}", final_preg.hw),
                        register_state: String::new(),
                    });
                } else if let Some(slot) = self.spill.has_slot(vreg) {
                    stack_arg_loads.push((
                        EditPoint::Before(0),
                        Edit::LoadIncomingArg {
                            fp_offset: *offset,
                            to: Alloc::Stack(slot),
                        },
                    ));
                    TracePush::push_with(&mut self.trace, || TraceEntry {
                        vinst_idx: 0,
                        vinst_mnemonic: String::from("entry_load_stack_arg"),
                        decision: alloc::format!("[fp+{offset}] -> slot{slot}"),
                        register_state: String::new(),
                    });
                }
            }
        }

        // Concatenate in dependency order: reads-to-memory, then the
        // hazard-free register shuffle, then writes derived from its results.
        let mut entry_edits: Vec<(EditPoint, Edit)> = entry_spills;
        for (from, to) in
            sequence_arg_moves(pending_entry, self.func_abi.isa().move_cycle_scratch())
        {
            entry_edits.push((EditPoint::Before(0), Edit::Move { from, to }));
        }
        entry_edits.extend(slot_inits);
        entry_edits.extend(stack_arg_loads);
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

fn stable_sort_edits_by_point(edits: &mut [(EditPoint, Edit)]) {
    for i in 1..edits.len() {
        let mut j = i;
        while j > 0 && edits[j].0 < edits[j - 1].0 {
            edits.swap(j, j - 1);
            j -= 1;
        }
    }
}

/// Walk a Linear region backward, producing AllocOutput (whole slice = one Linear root).
pub fn walk_linear(
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &FuncAbi,
) -> Result<AllocOutput, AllocError> {
    walk_linear_with_pool(vinsts, vreg_pool, func_abi, RegPool::for_abi(func_abi))
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
#[allow(
    clippy::too_many_arguments,
    reason = "explicit borrows of WalkState's fields; a struct would re-borrow the whole state"
)]
fn process_generic(
    inst: &VInst,
    inst_idx: usize,
    inst_idx_u16: u16,
    offset: usize,
    vreg_pool: &[VReg],
    func_abi: &FuncAbi,
    pool: &mut RegPool,
    classes: &VRegClasses,
    spill: &mut SpillAlloc,
    allocs: &mut [Alloc],
    edits: &mut Vec<(EditPoint, Edit)>,
    trace: &mut TraceSink,
) -> Result<(), AllocError> {
    // Special case: Mov is a copy. Coalesce src and dst to the same register
    // to eliminate the move at emission time (emitter skips addi rd, rs, 0 when rd==rs).
    if let VInst::Mov { dst, src, .. } = inst {
        let def_idx = offset;
        let use_idx = offset + 1;

        // Def side: determine dst's allocation (already assigned earlier in backward walk)
        let dst_alloc = if let Some(preg) = pool.home(*dst) {
            Alloc::reg(preg)
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
                allocs[use_idx] = alloc_use(
                    *src,
                    classes.of(*src),
                    inst_idx,
                    inst_idx_u16,
                    pool,
                    spill,
                    edits,
                    trace,
                )?;
            } else {
                // The coalesce target is `dst`'s own register, so `src` lands
                // in `dst`'s class — which is the same class, since a `Mov`
                // copies a value rather than converting it.
                let evicted = pool.alloc_fixed(preg, *src);
                if let Some(evicted_vreg) = evicted {
                    let slot = spill.get_or_assign(evicted_vreg, preg.class)?;
                    edits.push((
                        EditPoint::After(inst_idx_u16),
                        Edit::Move {
                            from: Alloc::Stack(slot),
                            to: Alloc::reg(preg),
                        },
                    ));
                    TracePush::push_with(trace, || TraceEntry {
                        vinst_idx: inst_idx,
                        vinst_mnemonic: String::from("coalesce_evict"),
                        decision: alloc::format!(
                            "slot{} -> t{} (v{})",
                            slot,
                            preg.hw,
                            evicted_vreg.0
                        ),
                        register_state: String::new(),
                    });
                }
                // If src was evicted by a call clobber and has a spill slot,
                // the After(reload) expects the value in the slot. Since we're
                // coalescing src back into a register, emit a store-after-def
                // so the slot gets the value too.
                if let Some(slot) = spill.has_slot(*src) {
                    edits.push((
                        EditPoint::After(inst_idx_u16),
                        Edit::Move {
                            from: Alloc::reg(preg),
                            to: Alloc::Stack(slot),
                        },
                    ));
                }
                allocs[use_idx] = Alloc::reg(preg);
                TracePush::push_with(trace, || TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("coalesce"),
                    decision: alloc::format!("v{} -> t{} (shared)", src.0, preg.hw),
                    register_state: String::new(),
                });
            }
            // dst may also have a spill slot (assigned by a later eviction in
            // the backward walk). Store the register to that slot so reloads
            // find the correct value — same logic as process_generic's
            // def_spill_stores, but for the coalesced Mov path.
            if let Some(slot) = spill.has_slot(*dst) {
                edits.push((
                    EditPoint::After(inst_idx_u16),
                    Edit::Move {
                        from: Alloc::reg(preg),
                        to: Alloc::Stack(slot),
                    },
                ));
            }
        } else {
            // Dst is spilled or dead: use normal allocation path for src
            allocs[use_idx] = alloc_use(
                *src,
                classes.of(*src),
                inst_idx,
                inst_idx_u16,
                pool,
                spill,
                edits,
                trace,
            )?;
        }
        return Ok(());
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
            Alloc::reg(preg)
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
                    from: Alloc::reg(preg),
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
    // `for_each_use` is a callback, so a failed allocation is captured here and
    // returned once the walk of this instruction's operands is done.
    let mut alloc_err: Option<AllocError> = None;
    inst.for_each_use(vreg_pool, |use_vreg| {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;

        let class = classes.of(use_vreg);
        let alloc = if is_sret_ret {
            match spill.get_or_assign(use_vreg, class) {
                Ok(slot) => {
                    if let Some(preg) = pool.home(use_vreg) {
                        pool.free(preg);
                    }
                    Alloc::Stack(slot)
                }
                Err(e) => {
                    alloc_err.get_or_insert(e);
                    Alloc::None
                }
            }
        } else {
            match alloc_use(
                use_vreg,
                class,
                inst_idx,
                inst_idx_u16,
                pool,
                spill,
                edits,
                trace,
            ) {
                Ok(a) => a,
                Err(e) => {
                    alloc_err.get_or_insert(e);
                    Alloc::None
                }
            }
        };
        allocs[alloc_idx] = alloc;
    });
    if let Some(e) = alloc_err {
        return Err(e);
    }

    // Pushed after uses so that after global reverse, def stores come
    // before any After(reload) from handle_eviction — ensuring the slot
    // is written before it can be overwritten by an eviction reload to
    // the same physical register.
    edits.extend(def_spill_stores);
    Ok(())
}

/// Allocate a use operand into `class`: reload from spill or allocate fresh,
/// evicting if needed.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit borrows of WalkState's fields; a struct would re-borrow the whole state"
)]
fn alloc_use(
    use_vreg: VReg,
    class: RegClass,
    inst_idx: usize,
    inst_idx_u16: u16,
    pool: &mut RegPool,
    spill: &mut SpillAlloc,
    edits: &mut Vec<(EditPoint, Edit)>,
    trace: &mut TraceSink,
) -> Result<Alloc, AllocError> {
    if let Some(preg) = pool.home(use_vreg) {
        pool.touch(preg);
        Ok(Alloc::reg(preg))
    } else if let Some(slot) = spill.has_slot(use_vreg) {
        let (new_preg, evicted) = pool
            .alloc(use_vreg, class)
            .ok_or(AllocError::OutOfRegisters)?;
        edits.push((
            EditPoint::Before(inst_idx_u16),
            Edit::Move {
                from: Alloc::Stack(slot),
                to: Alloc::reg(new_preg),
            },
        ));
        TracePush::push_with(trace, || TraceEntry {
            vinst_idx: inst_idx,
            vinst_mnemonic: String::from("reload"),
            decision: alloc::format!("slot{slot} -> t{}", new_preg.hw),
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
        )?;
        Ok(Alloc::reg(new_preg))
    } else {
        let (new_preg, evicted) = pool
            .alloc(use_vreg, class)
            .ok_or(AllocError::OutOfRegisters)?;
        handle_eviction(
            evicted,
            new_preg,
            inst_idx,
            inst_idx_u16,
            spill,
            edits,
            trace,
        )?;
        TracePush::push_with(trace, || TraceEntry {
            vinst_idx: inst_idx,
            vinst_mnemonic: String::from("alloc"),
            decision: alloc::format!("v{} -> t{}", use_vreg.0, new_preg.hw),
            register_state: String::new(),
        });
        Ok(Alloc::reg(new_preg))
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
) -> Result<(), AllocError> {
    if let Some(evicted_vreg) = evicted {
        // The evicted value was living in `preg`, so its slot holds a value of
        // `preg`'s class by construction.
        let slot = spill.get_or_assign(evicted_vreg, preg.class)?;
        // Emit a reload-after (regalloc2 style): the evicted vreg's DEF will
        // write directly to its spill slot.  After the current instruction
        // finishes, we reload the spilled value back into the register so it
        // is available for subsequent uses.
        edits.push((
            EditPoint::After(inst_idx_u16),
            Edit::Move {
                from: Alloc::Stack(slot),
                to: Alloc::reg(preg),
            },
        ));
        TracePush::push_with(trace, || TraceEntry {
            vinst_idx: inst_idx,
            vinst_mnemonic: String::from("evict"),
            decision: alloc::format!("slot{slot} -> t{}", preg.hw),
            register_state: String::new(),
        });
    }
    Ok(())
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
#[allow(
    clippy::too_many_arguments,
    reason = "explicit borrows of WalkState's fields; a struct would re-borrow the whole state"
)]
fn process_call(
    func_abi: &FuncAbi,
    inst: &VInst,
    inst_idx: usize,
    inst_idx_u16: u16,
    offset: usize,
    vreg_pool: &[VReg],
    pool: &mut RegPool,
    classes: &VRegClasses,
    spill: &mut SpillAlloc,
    allocs: &mut [Alloc],
    edits: &mut Vec<(EditPoint, Edit)>,
    trace: &mut TraceSink,
    passthrough: &[Option<PReg>],
    scratch: &mut CallScratch,
) -> Result<(), AllocError> {
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
    // All edits go into scratch vectors — nothing is pushed to the global `edits`
    // until the end, so we have full control over forward-order sequencing.
    // The buffers live in `WalkState::call_scratch` and are reused across
    // calls (Q32 code is call-dense; per-call Vecs were measurable churn).
    scratch.clear();
    let CallScratch {
        before_arg_moves,
        after_ret_moves,
        after_restores,
        // Pool registers that receive call return values.  After(call)
        // eviction restores must NOT target these, or they overwrite the
        // return value (regalloc2 avoids this by removing clobbers from
        // available_pregs before operand allocation; we filter at
        // restore-emit time). Direct returns get explicit ret moves; sret
        // returns are loaded by the emitter from the caller-side sret buffer
        // into these same pool registers.
        ret_value_pool_regs,
        clobbered,
        clobbered_pregs,
        reg_pass_args,
        stack_pass_args,
    } = scratch;

    // ── Step 1: Defs (return values) ──
    //
    // The direct return registers are read *simultaneously* in ABI terms, just
    // like the argument registers are written simultaneously: every return
    // value must leave its ABI register carrying the value the callee put
    // there, before any other return's move overwrites it. Emitting the moves
    // in return order is only safe when no return's destination is another
    // return's source — true on rv32, whose return registers (hw 10..11) and
    // allocatable pool (18..31) are disjoint, and false on Xtensa, where the
    // caller-view return bank a10/a11 sits inside its own 12-register pool.
    //
    // Collected here and sequenced below via the same `sequence_arg_moves`
    // used for the argument direction.
    let mut pending_ret_moves: Vec<(Alloc, PReg)> = Vec::new();
    // Write-throughs `Reg(pool_home) -> Stack(slot)`: they read a *destination*
    // of the moves above, so they must run after all of them.
    let mut ret_write_throughs: Vec<(EditPoint, Edit)> = Vec::new();

    let mut operand_idx: usize = 0;
    for (i, &ret_vreg) in rets.iter().enumerate() {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;
        let ret_class = classes.of(ret_vreg);

        if callee_uses_sret || i >= isa.direct_ret_reg_count(ret_class) {
            let alloc = if let Some(preg) = pool.home(ret_vreg) {
                ret_value_pool_regs.push(preg);
                Alloc::reg(preg)
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
            .direct_ret_reg(ret_class, i)
            .ok_or(AllocError::OutOfRegisters)?;

        allocs[alloc_idx] = Alloc::reg(target);

        if let Some(pool_reg) = pool.home(ret_vreg) {
            ret_value_pool_regs.push(pool_reg);
            pending_ret_moves.push((Alloc::reg(target), pool_reg));
            if let Some(slot) = spill.has_slot(ret_vreg) {
                ret_write_throughs.push((
                    EditPoint::After(inst_idx_u16),
                    Edit::Move {
                        from: Alloc::reg(pool_reg),
                        to: Alloc::Stack(slot),
                    },
                ));
            }
            TracePush::push_with(trace, || TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("call_ret"),
                decision: alloc::format!("x{} -> x{} (v{})", target.hw, pool_reg.hw, ret_vreg.0),
                register_state: String::new(),
            });
            pool.free(pool_reg);
        } else if let Some(slot) = spill.has_slot(ret_vreg) {
            after_ret_moves.push((
                EditPoint::After(inst_idx_u16),
                Edit::Move {
                    from: Alloc::reg(target),
                    to: Alloc::Stack(slot),
                },
            ));
            TracePush::push_with(trace, || TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("call_ret"),
                decision: alloc::format!("x{} -> slot{} (v{})", target.hw, slot, ret_vreg.0),
                register_state: String::new(),
            });
        }
    }

    // Order within the After(call) return group:
    //   1. `Reg(ret_reg) -> Stack(slot)` stores pushed by the loop above. They
    //      read a return register, so they must precede any move that writes
    //      one. Stores touch no register, so they cannot disturb the moves.
    //   2. the sequenced reg->reg moves.
    //   3. the write-throughs, which read a move *destination*.
    for (from, to) in sequence_arg_moves(pending_ret_moves, isa.move_cycle_scratch()) {
        after_ret_moves.push((EditPoint::After(inst_idx_u16), Edit::Move { from, to }));
    }
    after_ret_moves.extend(ret_write_throughs);

    // ── Step 2: Evict-then-reload for caller-saved pool t-regs ──
    // regalloc2-style: evict clobbered-reg occupants from the pool and remove
    // the registers from the LRU so they can't be reused during arg allocation
    // (matches regalloc2's remove_clobbers_from_available_pregs). Emit only
    // post-call reloads, no pre-call saves.
    //
    // One call clobbers every class's caller-saved bank at once, so the sweep
    // is over both. `RegClass::Float` contributes nothing while no backend has
    // float registers.
    for class in RegClass::ALL {
        clobbered.extend(isa.caller_saved_pool_hw(class).iter().filter_map(|&hw| {
            let preg = PReg { hw, class };
            pool.iter_occupied()
                .find(|&(p, _)| p == preg)
                .map(|(_, v)| (preg, v))
        }));
    }
    for (preg, vreg) in clobbered.iter() {
        let slot = spill.get_or_assign(*vreg, preg.class)?;
        pool.evict(*preg);
        clobbered_pregs.push(*preg);
        after_restores.push((
            EditPoint::After(inst_idx_u16),
            Edit::Move {
                from: Alloc::Stack(slot),
                to: Alloc::reg(*preg),
            },
        ));
        TracePush::push_with(trace, || TraceEntry {
            vinst_idx: inst_idx,
            vinst_mnemonic: String::from("clobber_evict"),
            decision: alloc::format!("v{} evicted from x{} -> slot{}", vreg.0, preg.hw, slot),
            register_state: String::new(),
        });
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

    // `reg_pass_args`: (vreg, target_arg_reg) for register-pass args — Before
    // moves deferred.
    // `stack_pass_args`: (operand_alloc_index, vreg) for stack-pass args.
    // Stack-pass operands must be assigned after all argument allocation is
    // done: duplicate args can be evicted while preparing later operands, and
    // the emitter needs the final location when it stores to the outgoing
    // stack area.

    // ── Phase A: allocate every arg vreg into the pool ──
    for (i, &arg_vreg) in args.iter().enumerate() {
        let alloc_idx = offset + operand_idx;
        operand_idx += 1;
        let arg_class = classes.of(arg_vreg);

        let target_opt = isa.lpir_call_arg_target(
            arg_class,
            callee_uses_sret,
            caller_passes_sret_ptr,
            caller_sret_vm_abi_swap,
            i,
        );
        let is_reg_pass = target_opt.is_some();
        let trace_target = target_opt.map(|p| p.hw).unwrap_or(0);
        if let Some(target) = target_opt {
            // Pass-through shortcut: vreg stays in its ABI register, no pool needed.
            let is_passthrough = passthrough
                .get(arg_vreg.0 as usize)
                .copied()
                .flatten()
                .is_some_and(|entry_reg| entry_reg == target);
            if is_passthrough {
                allocs[alloc_idx] = Alloc::reg(target);
                TracePush::push_with(trace, || TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("call_arg"),
                    decision: alloc::format!("v{}: x{} (passthrough)", arg_vreg.0, target.hw),
                    register_state: String::new(),
                });
                operand_idx += 0; // already incremented
                continue;
            }

            reg_pass_args.push((arg_vreg, target));
            allocs[alloc_idx] = Alloc::reg(target);
        }

        if let Some(pool_reg) = pool.home(arg_vreg) {
            pool.touch(pool_reg);
        } else if let Some(slot) = spill.has_slot(arg_vreg) {
            let (new_preg, evicted) = pool
                .alloc(arg_vreg, arg_class)
                .ok_or(AllocError::OutOfRegisters)?;
            if let Some(ev) = evicted {
                let ev_slot = spill.get_or_assign(ev, new_preg.class)?;
                if !ret_value_pool_regs.contains(&new_preg) {
                    after_restores.push((
                        EditPoint::After(inst_idx_u16),
                        Edit::Move {
                            from: Alloc::Stack(ev_slot),
                            to: Alloc::reg(new_preg),
                        },
                    ));
                }
                TracePush::push_with(trace, || TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("evict"),
                    decision: alloc::format!("x{} -> slot{} (v{})", new_preg.hw, ev_slot, ev.0),
                    register_state: String::new(),
                });
            }
            before_arg_moves.push((
                EditPoint::Before(inst_idx_u16),
                Edit::Move {
                    from: Alloc::Stack(slot),
                    to: Alloc::reg(new_preg),
                },
            ));
            TracePush::push_with(trace, || TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("reload"),
                decision: alloc::format!("slot{} -> x{} (v{})", slot, new_preg.hw, arg_vreg.0),
                register_state: String::new(),
            });
        } else {
            let (new_preg, evicted) = pool
                .alloc(arg_vreg, arg_class)
                .ok_or(AllocError::OutOfRegisters)?;
            if let Some(ev) = evicted {
                let ev_slot = spill.get_or_assign(ev, new_preg.class)?;
                if !ret_value_pool_regs.contains(&new_preg) {
                    after_restores.push((
                        EditPoint::After(inst_idx_u16),
                        Edit::Move {
                            from: Alloc::Stack(ev_slot),
                            to: Alloc::reg(new_preg),
                        },
                    ));
                }
                TracePush::push_with(trace, || TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("evict"),
                    decision: alloc::format!("x{} -> slot{} (v{})", new_preg.hw, ev_slot, ev.0),
                    register_state: String::new(),
                });
            }
        }

        if !is_reg_pass {
            stack_pass_args.push((alloc_idx, arg_vreg));
        }

        TracePush::push_with(trace, || TraceEntry {
            vinst_idx: inst_idx,
            vinst_mnemonic: String::from("call_arg"),
            decision: if is_reg_pass {
                alloc::format!("v{}: pool -> x{} (deferred)", arg_vreg.0, trace_target)
            } else {
                alloc::format!(
                    "v{}: x{} (stack-pass)",
                    arg_vreg.0,
                    pool.home(arg_vreg).map(|p| p.hw).unwrap_or(0)
                )
            },
            register_state: String::new(),
        });
    }

    // ── Phase B: compute the Before(call) moves for register-pass args ──
    // The pool now reflects the final allocation state after all evictions.
    //
    // These moves happen *simultaneously* in ABI terms: every argument must
    // reach its staging register carrying the value it had before any of them
    // ran. Emitting them in argument order is only safe when no argument's
    // source register is another argument's destination — true on rv32, where
    // the argument registers and the allocatable pool are disjoint sets, and
    // false on Xtensa, where the staging bank IS the caller-saved half of the
    // pool. `sequence_arg_moves` orders them (and breaks cycles) so the
    // simultaneity holds on both.
    let mut pending_moves: Vec<(Alloc, PReg)> = Vec::new();
    for &(arg_vreg, target) in reg_pass_args.iter() {
        if let Some(pool_reg) = pool.home(arg_vreg) {
            if pool_reg != target {
                pending_moves.push((Alloc::reg(pool_reg), target));
            }
            TracePush::push_with(trace, || TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("call_arg_move"),
                decision: alloc::format!("v{}: x{} -> x{}", arg_vreg.0, pool_reg.hw, target.hw),
                register_state: String::new(),
            });
        } else if let Some(slot) = spill.has_slot(arg_vreg) {
            pending_moves.push((Alloc::Stack(slot), target));
            TracePush::push_with(trace, || TraceEntry {
                vinst_idx: inst_idx,
                vinst_mnemonic: String::from("call_arg_move"),
                decision: alloc::format!("v{}: slot{} -> x{}", arg_vreg.0, slot, target.hw),
                register_state: String::new(),
            });
        }
    }
    // ── Phase B1: record final locations for stack-pass args ──
    //
    // A stack-pass arg is written to the outgoing argument area by the *ISA
    // emitter*, inside `emit_call` — i.e. after every staging move computed
    // above has already run. Its home register is therefore live across those
    // moves, and naming it here is only safe if no staging move writes it.
    //
    // That holds on rv32 for the usual reason: the staging targets are the
    // argument registers (hw 10..17) and a home comes from the allocatable
    // pool (18..31), which is disjoint from them. On Xtensa the staging bank
    // a10..a15 *is* the caller-saved half of the pool, so at high arity — 12
    // user arguments is the first — a value that overflows to the stack sits
    // in a register another argument is staged into. The store then reads the
    // staged value and the callee sees a duplicate.
    //
    // Where that collides, park the value in its spill slot before the staging
    // moves run and hand the emitter the slot instead; both emitters already
    // reload a `Stack` outgoing arg through their scratch register.
    let move_scratch = isa.move_cycle_scratch();
    for &(alloc_idx, arg_vreg) in stack_pass_args.iter() {
        let home = pool.home(arg_vreg);
        // `move_scratch` is outside the allocatable pool on both ISAs, so it is
        // never a home; it is in the set because `sequence_arg_moves` writes it
        // when breaking a cycle, and this set means "written before the store".
        let staged_over = home.is_some_and(|r| {
            r == move_scratch || pending_moves.iter().any(|&(_, target)| target == r)
        });
        allocs[alloc_idx] = match home {
            Some(pool_reg) if !staged_over => Alloc::reg(pool_reg),
            Some(pool_reg) => {
                let slot = spill.get_or_assign(arg_vreg, pool_reg.class)?;
                before_arg_moves.push((
                    EditPoint::Before(inst_idx_u16),
                    Edit::Move {
                        from: Alloc::reg(pool_reg),
                        to: Alloc::Stack(slot),
                    },
                ));
                TracePush::push_with(trace, || TraceEntry {
                    vinst_idx: inst_idx,
                    vinst_mnemonic: String::from("call_arg_park"),
                    decision: alloc::format!(
                        "v{}: x{} -> slot{slot} (staged over)",
                        arg_vreg.0,
                        pool_reg.hw
                    ),
                    register_state: String::new(),
                });
                Alloc::Stack(slot)
            }
            None => match spill.has_slot(arg_vreg) {
                Some(slot) => Alloc::Stack(slot),
                None => Alloc::None,
            },
        };
    }

    // The parks are pushed after the Phase-A reloads and before the staging
    // moves, which is the only order that works: a stack-pass arg may itself
    // have been reloaded into its home by one of those reloads.
    for (from, to) in sequence_arg_moves(pending_moves, move_scratch) {
        before_arg_moves.push((EditPoint::Before(inst_idx_u16), Edit::Move { from, to }));
    }

    // Restore clobbered registers to the LRU now that arg allocation is done.
    pool.restore_evicted(clobbered_pregs);

    // Push edits in reverse-forward order (global reverse will restore forward order).
    // Desired forward: Before(arg_moves), After(ret_moves), After(restores)
    // Push order:      After(restores), After(ret_moves), Before(arg_moves)
    //
    // ret_moves come before restores in forward order so that the return value
    // lands in its pool register before any eviction restores run.  Eviction
    // restores that target a ret_move pool register are already filtered out
    // above, but sequencing ret_moves first is an extra safety net.
    for &e in after_restores.iter().rev() {
        edits.push(e);
    }
    for &e in after_ret_moves.iter().rev() {
        edits.push(e);
    }
    for &e in before_arg_moves.iter().rev() {
        edits.push(e);
    }
    Ok(())
}

/// Order a set of simultaneous register moves into a safe sequence.
///
/// Used in both call directions:
///
/// - **arguments** — `from` is wherever the value lives, `to` is the ABI's
///   argument register. Emitting in argument order destroys a value before its
///   consumer reads it and the caller silently passes a duplicate.
/// - **returns** — `from` is the ABI's return register, `to` is the value's
///   pool home. Emitting in return order destroys a return value before it is
///   moved out, and the caller silently reads a duplicate.
///
/// Every `to` is distinct in both cases (distinct ABI registers one way,
/// distinct pool homes the other), but a `to` may also be another move's
/// `from`. This is the standard "parallel move" problem.
///
/// The sequence is built by repeatedly emitting any move whose destination is
/// no longer anyone's source. When only cycles remain, one destination's live
/// value is parked in `scratch` and its readers rewritten to read `scratch`,
/// which breaks the cycle; `scratch` is outside the allocatable pool, and it is
/// reusable across cycles because a parked value is always consumed before the
/// next break.
///
/// On rv32 this is an identity transform in both directions (the argument and
/// return registers are disjoint from the pool, so no destination is ever a
/// source). It exists for Xtensa, whose staging bank `a10..a15` and return
/// bank `a10..a11` both sit inside its own allocatable pool — and as a
/// correctness net for any future ISA that overlaps them.
/// `scratch` must be of the same class as the registers being shuffled — a
/// float value cannot be parked in a GPR. Every move set is single-class today
/// (there are no float argument registers to shuffle), so the caller passes the
/// ISA's integer scratch.
fn sequence_arg_moves(mut pending: Vec<(Alloc, PReg)>, scratch: PReg) -> Vec<(Alloc, Alloc)> {
    fn src_reg(a: &Alloc) -> Option<PReg> {
        a.preg()
    }

    let mut out: Vec<(Alloc, Alloc)> = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let mut progressed = false;
        let mut i = 0;
        while i < pending.len() {
            let (from, to) = pending[i];
            let still_needed = pending
                .iter()
                .enumerate()
                .any(|(j, (f, _))| j != i && src_reg(f) == Some(to));
            if still_needed {
                i += 1;
                continue;
            }
            out.push((from, Alloc::reg(to)));
            pending.remove(i);
            progressed = true;
        }
        if pending.is_empty() {
            break;
        }
        if !progressed {
            // Everything left is in a cycle: park one destination's live value
            // in scratch and point its readers at scratch instead. That move
            // becomes emittable on the next pass.
            let (_, blocked_to) = pending[0];
            out.push((Alloc::reg(blocked_to), Alloc::reg(scratch)));
            for (f, _) in pending.iter_mut() {
                if src_reg(f) == Some(blocked_to) {
                    *f = Alloc::reg(scratch);
                }
            }
        }
    }
    out
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

    /// A function that outruns the spill-slot index space must be **reported**,
    /// not wrapped.
    ///
    /// `SpillAlloc::next_slot` used to be the same `u8` it hands out, so the
    /// 257th slot wrapped to 0. Under `overflow-checks` that is a panic — taken
    /// down the on-device compiler in `fw-esp32v3`, which compiles authored
    /// GLSL. With overflow checks off (the firmware's release profile) it is
    /// worse and quieter: a fresh vreg gets slot 0, two live values share four
    /// bytes of frame, and the shader miscompiles all the way to the strip.
    ///
    /// The shape is the one that found it — hundreds of values live at once in
    /// a single function, which is what a long straight-line `render()` body
    /// lowers to. Every value is defined up front and consumed in definition
    /// order, so each one's live range spans the whole chain.
    #[test]
    fn a_function_past_the_spill_slot_ceiling_reports_instead_of_wrapping() {
        use alloc::string::String;
        use core::fmt::Write as _;

        // Comfortably past the 256-slot ceiling without being so large that a
        // debug-build walk is slow.
        const N: u16 = 400;
        let mut input = String::new();
        for i in 0..N {
            writeln!(input, "i{i} = IConst32 {i}").unwrap();
        }
        let mut acc = 0u16;
        for i in 1..N {
            let dst = N + i;
            writeln!(input, "i{dst} = Add i{acc}, i{i}").unwrap();
            acc = dst;
        }
        writeln!(input, "Ret i{acc}").unwrap();

        let (vinsts, _symbols, pool) = vinst::parse(&input).unwrap();
        let err = walk_linear(&vinsts, &pool, &make_abi())
            .expect_err("400 simultaneously live values must exhaust the slot space");
        assert_eq!(
            err,
            AllocError::TooManySpillSlots {
                max: u32::from(crate::regalloc::spill::SpillAlloc::MAX_SLOTS)
            },
        );
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

    /// FuelCheck is a plain use of its vmctx vreg: the walk must allocate it
    /// to a register like any other use.
    #[test]
    fn walk_fuel_check_use_allocated() {
        let input = "i0 = IConst32 100\nFuelCheck i0, @0, dec\nRet";
        let (vinsts, _symbols, pool) = vinst::parse(input).unwrap();
        let output = walk_linear(&vinsts, &pool, &make_abi()).unwrap();

        // FuelCheck (inst 1) has no defs, one use (i0) at operand 0.
        let alloc = output.operand_alloc(1, 0);
        assert!(
            alloc.is_reg(),
            "FuelCheck vmctx use must be in a register, got {alloc:?}"
        );
    }

    /// Spilled vmctx: with a 2-register pool and enough live values to evict
    /// i0, the FuelCheck use must trigger a reload from the spill slot.
    #[test]
    fn walk_fuel_check_spilled_vmctx_reload() {
        let input = "\
            i0 = IConst32 100\n\
            i1 = IConst32 2\n\
            i2 = IConst32 3\n\
            i3 = Add i1, i2\n\
            FuelCheck i0, @0, dec\n\
            Ret i3";
        let (vinsts, _symbols, pool) = vinst::parse(input).unwrap();
        let output = walk_linear_with_pool(
            &vinsts,
            &pool,
            &make_abi(),
            RegPool::with_capacity(crate::isa::IsaTarget::Rv32imac, 2),
        )
        .unwrap();

        assert!(
            output.num_spill_slots >= 1,
            "expected at least 1 spill slot (i0 evicted)"
        );
        // i0's def must go to its spill slot (evicted during backward walk).
        let i0_def = output.operand_alloc(0, 0);
        assert!(i0_def.is_stack(), "i0 def should be Stack, got {i0_def:?}");
        // FuelCheck's vmctx use (inst 4, operand 0) must still land in a
        // register, fed by a reload edit somewhere between def and use.
        let fuel_use = output.operand_alloc(4, 0);
        assert!(
            fuel_use.is_reg(),
            "FuelCheck vmctx use should be Reg (reloaded), got {fuel_use:?}"
        );
        let has_reload = output.edits.iter().any(|(_, edit)| {
            matches!(
                edit,
                Edit::Move {
                    from: Alloc::Stack(_),
                    to: Alloc::Reg(_)
                }
            )
        });
        assert!(
            has_reload,
            "expected a stack->reg reload edit, got edits: {edits:?}",
            edits = output.edits
        );
    }
    /// Independent moves keep their order and all land.
    #[test]
    fn sequence_arg_moves_passes_through_independent_moves() {
        let out = sequence_arg_moves(
            vec![
                (Alloc::int_reg(20), PReg::int(10)),
                (Alloc::int_reg(21), PReg::int(11)),
            ],
            PReg::int(9),
        );
        assert_eq!(
            out,
            vec![
                (Alloc::int_reg(20), Alloc::int_reg(10)),
                (Alloc::int_reg(21), Alloc::int_reg(11)),
            ]
        );
    }

    /// The defect this function exists for: `a12`'s value is needed by the move
    /// into `a13`, so the write to `a12` must come second. Emitting in argument
    /// order silently passed a duplicate.
    #[test]
    fn sequence_arg_moves_orders_a_chain_before_its_source_is_clobbered() {
        // want: a12 <- a13, a13 <- a12's ORIGINAL value is not required here;
        // the chain is a13 <- a12 and a12 <- a11.
        let out = sequence_arg_moves(
            vec![
                (Alloc::int_reg(11), PReg::int(12)),
                (Alloc::int_reg(12), PReg::int(13)),
            ],
            PReg::int(9),
        );
        assert_eq!(
            out,
            vec![
                // a13 <- a12 first: a12 is still carrying its incoming value.
                (Alloc::int_reg(12), Alloc::int_reg(13)),
                (Alloc::int_reg(11), Alloc::int_reg(12)),
            ],
            "a chain must be emitted from its tail"
        );
    }

    /// A true swap has no safe order, so one value goes through the scratch
    /// register. Checked by simulation rather than by pinning an instruction
    /// sequence, so the test constrains the semantics and not the strategy.
    #[test]
    fn sequence_arg_moves_breaks_a_two_cycle_through_scratch() {
        const SCRATCH: u8 = 9;
        let out = sequence_arg_moves(
            vec![
                (Alloc::int_reg(10), PReg::int(11)),
                (Alloc::int_reg(11), PReg::int(10)),
            ],
            PReg::int(SCRATCH),
        );

        // Simulate: each register starts holding its own id.
        let mut regs: [u8; 32] = core::array::from_fn(|i| i as u8);
        for (from, to) in &out {
            let (Alloc::Reg(f), Alloc::Reg(t)) = (from, to) else {
                panic!("register moves only");
            };
            regs[t.hw() as usize] = regs[f.hw() as usize];
        }
        assert_eq!(regs[11], 10, "a11 must end up with a10's incoming value");
        assert_eq!(regs[10], 11, "a10 must end up with a11's incoming value");
        assert!(
            out.iter()
                .any(|(_, to)| matches!(to, Alloc::Reg(r) if r.hw() == SCRATCH)),
            "a two-cycle cannot be resolved without the scratch register"
        );
    }

    /// Three-cycle, same simulation check — the cycle-breaking must not be
    /// special-cased to pairs.
    #[test]
    fn sequence_arg_moves_breaks_a_three_cycle() {
        let out = sequence_arg_moves(
            vec![
                (Alloc::int_reg(10), PReg::int(11)),
                (Alloc::int_reg(11), PReg::int(12)),
                (Alloc::int_reg(12), PReg::int(10)),
            ],
            PReg::int(9),
        );
        let mut regs: [u8; 32] = core::array::from_fn(|i| i as u8);
        for (from, to) in &out {
            let (Alloc::Reg(f), Alloc::Reg(t)) = (from, to) else {
                panic!("register moves only");
            };
            regs[t.hw() as usize] = regs[f.hw() as usize];
        }
        assert_eq!([regs[11], regs[12], regs[10]], [10, 11, 12]);
    }

    /// Spill-slot sources have no register to clobber, but their destination can
    /// still be another move's source.
    #[test]
    fn sequence_arg_moves_orders_stack_sources_after_their_destination_is_read() {
        let out = sequence_arg_moves(
            vec![
                (Alloc::Stack(0), PReg::int(12)),
                (Alloc::int_reg(12), PReg::int(13)),
            ],
            PReg::int(9),
        );
        assert_eq!(
            out,
            vec![
                (Alloc::int_reg(12), Alloc::int_reg(13)),
                (Alloc::Stack(0), Alloc::int_reg(12)),
            ],
            "the reload into a12 must not run before a12 is read"
        );
    }

    /// Pins the **return** direction's move set, which `process_call` began
    /// routing through this function on 2026-07-30
    /// (`docs/defects/2026-07-30-xtensa-two-value-return-clobber.md`).
    ///
    /// This is a characterization test, not the regression test: the sequencer
    /// was always correct, and the defect was that the return path never called
    /// it. Reverting that fix does not fail this test. The negative-controlled
    /// regression lives in the corpus at
    /// `lpvm/native/perf/call-clobber-correctness.glsl:99`
    /// (`test_interleaved_vec2`), which goes 6/7 → 7/7 across the fix. What
    /// this test buys is that a future refactor of the sequencer cannot break
    /// the return direction silently.
    ///
    /// A two-value return arrives in the caller-view `a10`/`a11`. When the
    /// first return's pool home happens to be `a11`, moving it out first
    /// destroys the second return before anyone reads it. This is the exact
    /// move set observed for `vec2 a = make_vec2(…)` held live across a second
    /// call — the emitted code was:
    ///
    /// ```text
    /// or a11, a10, a10   ; ret0 -> home a11, clobbering ret1
    /// or a12, a11, a11   ; ret1 -> home a12, reading the clobbered value
    /// ```
    ///
    /// so `a.y` silently became `a.x`.
    #[test]
    fn sequence_arg_moves_orders_return_values_out_of_the_return_bank() {
        let out = sequence_arg_moves(
            vec![
                (Alloc::int_reg(10), PReg::int(11)),
                (Alloc::int_reg(11), PReg::int(12)),
            ],
            PReg::int(9),
        );
        assert_eq!(
            out,
            vec![
                // a11 (ret1) must vacate before ret0 is moved on top of it.
                (Alloc::int_reg(11), Alloc::int_reg(12)),
                (Alloc::int_reg(10), Alloc::int_reg(11)),
            ],
            "the second return value must leave a11 before the first overwrites it"
        );
    }
}
