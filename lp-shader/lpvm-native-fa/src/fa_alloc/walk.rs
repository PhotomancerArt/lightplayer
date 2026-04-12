//! Backward walk allocator with real register allocation decisions.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::region::{REGION_ID_NONE, Region, RegionId, RegionTree};
use crate::rv32::gpr::{self, ALLOC_POOL, FP_REG, PReg};
use crate::rv32::inst::PInst;
use crate::vinst::{VInst, VReg};

use super::spill::SpillAlloc;
use super::trace::{AllocTrace, TraceEntry};

/// FP-relative byte offset for spill slot `slot`. Slots 0,1,… live below the
/// saved RA (at FP-4) and saved FP (at FP-8), so the first usable spill is at
/// FP-12.
fn spill_fp_offset(slot: u8) -> i32 {
    -((slot as i32 + 1) * 4 + 8)
}

/// Allocation error types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocError {
    UnsupportedControlFlow,
    UnsupportedCall,
    TooManyArgs,
    UnsupportedSret,
}

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocError::UnsupportedControlFlow => write!(f, "branches/jumps not supported"),
            AllocError::UnsupportedCall => write!(f, "calls not supported"),
            AllocError::TooManyArgs => write!(f, "call with >8 args not supported"),
            AllocError::UnsupportedSret => write!(f, "sret calls not yet supported"),
        }
    }
}

/// Physical register pool with LRU eviction.
pub struct RegPool {
    /// Which vreg occupies each PReg (None = free).
    preg_vreg: [Option<VReg>; 32],
    /// LRU order: index 0 = least recently used. Only allocatable regs.
    lru: Vec<PReg>,
}

impl RegPool {
    pub fn new() -> Self {
        let lru: Vec<PReg> = ALLOC_POOL.iter().copied().collect();
        Self {
            preg_vreg: [None; 32],
            lru,
        }
    }

    /// Find the PReg currently holding this vreg, if any.
    pub fn home(&self, vreg: VReg) -> Option<PReg> {
        for (i, v) in self.preg_vreg.iter().enumerate() {
            if *v == Some(vreg) {
                return Some(i as PReg);
            }
        }
        None
    }

    /// Allocate a free register for vreg. Returns the PReg and any evicted vreg.
    /// If no free reg, evicts the LRU and returns (evicted_vreg, preg).
    pub fn alloc(&mut self, vreg: VReg) -> (PReg, Option<VReg>) {
        // Try to find a free allocatable reg (prefer LRU order)
        for (i, &preg) in self.lru.iter().enumerate() {
            if self.preg_vreg[preg as usize].is_none() {
                self.preg_vreg[preg as usize] = Some(vreg);
                // Move to end (most recently used)
                self.lru.remove(i);
                self.lru.push(preg);
                return (preg, None);
            }
        }
        // Evict LRU (index 0)
        let victim_preg = self.lru.remove(0);
        let victim_vreg = self.preg_vreg[victim_preg as usize];
        self.preg_vreg[victim_preg as usize] = Some(vreg);
        self.lru.push(victim_preg);
        (victim_preg, victim_vreg)
    }

    /// Allocate a specific PReg for vreg. Evicts current occupant if any.
    /// Returns the evicted vreg (if any).
    pub fn alloc_fixed(&mut self, preg: PReg, vreg: VReg) -> Option<VReg> {
        let evicted = self.preg_vreg[preg as usize];
        self.preg_vreg[preg as usize] = Some(vreg);
        self.touch(preg);
        evicted
    }

    /// Free a PReg (vreg is no longer in a register).
    pub fn free(&mut self, preg: PReg) {
        self.preg_vreg[preg as usize] = None;
    }

    /// Mark PReg as most recently used.
    pub fn touch(&mut self, preg: PReg) {
        if let Some(pos) = self.lru.iter().position(|&p| p == preg) {
            self.lru.remove(pos);
            self.lru.push(preg);
        }
    }

    /// Count occupied allocatable registers.
    pub fn occupied_count(&self) -> usize {
        ALLOC_POOL
            .iter()
            .filter(|&&p| self.preg_vreg[p as usize].is_some())
            .count()
    }

    /// Iterate over occupied (preg, vreg) pairs for allocatable registers.
    pub fn iter_occupied(&self) -> impl Iterator<Item = (PReg, VReg)> + '_ {
        ALLOC_POOL
            .iter()
            .copied()
            .filter_map(|p| self.preg_vreg[p as usize].map(|v| (p, v)))
    }

    /// Get a snapshot of current occupied (preg, vreg) pairs.
    /// Used for saving register state at region boundaries.
    pub fn snapshot_occupied(&self) -> Vec<(PReg, VReg)> {
        self.iter_occupied().collect()
    }

    /// Clear allocatable registers only (preserves precolored mappings).
    pub fn clear(&mut self) {
        for p in ALLOC_POOL.iter() {
            self.preg_vreg[*p as usize] = None;
        }
        self.lru.clear();
        self.lru.extend(ALLOC_POOL.iter().copied());
    }

    /// Clear ALL registers including precolored ones outside ALLOC_POOL.
    pub fn clear_all(&mut self) {
        self.preg_vreg = [None; 32];
        self.lru.clear();
        self.lru.extend(ALLOC_POOL.iter().copied());
    }

    /// Iterate ALL occupied registers, including precolored ones
    /// outside ALLOC_POOL (e.g. a0 for vmctx).
    pub fn iter_all_occupied(&self) -> impl Iterator<Item = (PReg, VReg)> + '_ {
        self.preg_vreg
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.map(|vreg| (i as PReg, vreg)))
    }

    /// Seed the pool with vreg assignments from saved state.
    /// Clears existing state first, then populates with saved assignments.
    pub fn seed(&mut self, assignments: &[(PReg, VReg)]) {
        self.clear();
        for &(preg, vreg) in assignments {
            self.preg_vreg[preg as usize] = Some(vreg);
            self.touch(preg);
        }
    }
}

/// State threaded through the backward walk.
pub struct WalkState<'a> {
    pub pool: RegPool,
    pub spill: SpillAlloc,
    pub trace: AllocTrace,
    pub pinsts: Vec<PInst>,
    pub symbols: &'a crate::vinst::ModuleSymbols,
}

impl<'a> WalkState<'a> {
    pub fn new(num_vregs: usize, symbols: &'a crate::vinst::ModuleSymbols) -> Self {
        Self {
            pool: RegPool::new(),
            spill: SpillAlloc::new(num_vregs),
            trace: AllocTrace::new(),
            pinsts: Vec::new(),
            symbols,
        }
    }

    /// Flush all occupied registers to spill slots.
    /// Emits Lw instructions (reloads in forward order) for each occupied vreg.
    /// Returns the saved (preg, vreg) assignments for later seeding.
    pub fn flush_to_slots(&mut self) -> Vec<(PReg, VReg)> {
        let occupied = self.pool.snapshot_occupied();
        let saved = occupied.clone();

        for (preg, vreg) in occupied {
            let slot = self.spill.get_or_assign(vreg);
            let offset = spill_fp_offset(slot);
            self.pinsts.push(crate::rv32::inst::PInst::Lw {
                dst: preg,
                base: crate::rv32::gpr::FP_REG,
                offset,
            });
        }

        self.pool.clear();
        saved
    }

    /// Emit Sw (spill) for all occupied registers.
    /// These become exit spills in forward order.
    /// Pool is NOT cleared — backward walk needs vregs registered so defs can free them.
    pub fn emit_exit_spills(&mut self) {
        let occupied: Vec<_> = self.pool.iter_occupied().collect();
        for (preg, vreg) in occupied {
            let slot = self.spill.get_or_assign(vreg);
            let offset = spill_fp_offset(slot);
            self.pinsts.push(crate::rv32::inst::PInst::Sw {
                src: preg,
                base: crate::rv32::gpr::FP_REG,
                offset,
            });
        }
    }

    /// Seed pool with vreg assignments from saved state.
    pub fn seed_pool(&mut self, saved: &[(PReg, VReg)]) {
        self.pool.seed(saved);
    }

    /// Seed pool with a full RegSet of vregs, allocating fresh registers.
    /// Used for loop boundaries where the set of live vregs is larger than
    /// what any single predecessor provides.
    pub fn seed_pool_from_regset(&mut self, vregs: &crate::regset::RegSet) {
        self.pool.clear_all();
        for vreg in vregs.iter() {
            self.spill.get_or_assign(vreg);
            let (_preg, _evicted) = self.pool.alloc(vreg);
        }
    }
}

/// Walk a region backward with real register allocation.
/// Returns error for unsupported control flow (IfThenElse/Loop/Call).
pub fn walk_region(
    state: &mut WalkState<'_>,
    tree: &RegionTree,
    region_id: RegionId,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,
) -> Result<(), AllocError> {
    if region_id == REGION_ID_NONE {
        return Ok(());
    }

    let region = &tree.nodes[region_id as usize];
    match region {
        Region::Linear { start, end } => {
            for i in (*start..*end).rev() {
                process_inst(state, i as usize, &vinsts[i as usize], vreg_pool, func_abi)?;
            }
            Ok(())
        }
        Region::Seq {
            children_start,
            child_count,
        } => {
            let start = *children_start as usize;
            let end = start + *child_count as usize;
            for &child_id in tree.seq_children[start..end].iter().rev() {
                walk_region(state, tree, child_id, vinsts, vreg_pool, func_abi)?;
            }
            Ok(())
        }
        Region::IfThenElse {
            head,
            then_body,
            else_body,
            else_label,
            merge_label,
        } => walk_ite(
            state,
            tree,
            *head,
            *then_body,
            *else_body,
            *else_label,
            *merge_label,
            vinsts,
            vreg_pool,
            func_abi,
        ),
        Region::Loop {
            header,
            body,
            header_label,
            exit_label,
        } => walk_loop(
            state,
            tree,
            *header,
            *body,
            *header_label,
            *exit_label,
            vinsts,
            vreg_pool,
            func_abi,
        ),
    }
}

/// Walk an IfThenElse region backward with spill-at-boundary.
///
/// Forward execution order after reversal:
///   [head] [Sw head exit] [Lw then entry] [then] [Sw then exit] [J merge]
///   [Label else] [Lw else entry] [else] [Sw else exit]
///   [Label merge] [Lw merge] [rest...]
fn walk_ite(
    state: &mut WalkState<'_>,
    tree: &RegionTree,
    head: RegionId,
    then_body: RegionId,
    else_body: RegionId,
    else_label: crate::vinst::LabelId,
    merge_label: crate::vinst::LabelId,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,
) -> Result<(), AllocError> {
    use crate::region::REGION_ID_NONE;
    use crate::rv32::inst::PInst;

    // At this point, 'state.pool' has vregs live after the if/else (in "rest").
    // 1. Flush to spill slots for the merge boundary.
    //    This emits Lw instructions which become reloads at merge in forward order.
    let merge_live = state.flush_to_slots();

    // 2. Emit merge label (appears at merge point in forward).
    state.pinsts.push(PInst::Label { id: merge_label });

    // 3. Process else body (if non-empty).
    //    In forward: else_label, Lw entry, else comp, Sw exit.
    //    In backward push: Sw exit, else comp, Lw entry, else_label.
    if else_body != REGION_ID_NONE {
        // Seed pool with merge_live state (vregs in regs for else uses).
        state.seed_pool(&merge_live);
        // Emit Sw at else exit (values go to slots before merge).
        state.emit_exit_spills();
        // Walk else computation.
        walk_region(state, tree, else_body, vinsts, vreg_pool, func_abi)?;
        // Flush to slots at else entry (emits Lw, clears pool).
        let _else_entry_live = state.flush_to_slots();
    }

    // 4. Emit else label.
    //    For empty else, this is the same as merge_label (handled by walker).
    if else_body != REGION_ID_NONE {
        state.pinsts.push(PInst::Label { id: else_label });
    }

    // 5. Emit J to merge (at end of then in forward).
    state.pinsts.push(PInst::J {
        target: merge_label,
    });

    // 6. Process then body.
    //    In forward: Lw entry, then comp, Sw exit, J merge.
    //    In backward push: J merge, Sw exit, then comp, Lw entry.
    state.seed_pool(&merge_live);
    state.emit_exit_spills();
    walk_region(state, tree, then_body, vinsts, vreg_pool, func_abi)?;
    let _then_entry_live = state.flush_to_slots();

    // 7. Process head.
    //    In forward: head comp, BrIf, Sw exit.
    //    In backward push: Sw exit, BrIf, head comp.
    state.seed_pool(&merge_live);
    state.emit_exit_spills();
    walk_region(state, tree, head, vinsts, vreg_pool, func_abi)?;
    // After head walk, pool has state at head entry.
    // The caller continues walking from here.

    Ok(())
}

/// Walk a Loop region backward with spill-at-boundary.
///
/// Forward execution order after reversal:
///   [header_label] [Lw header entry] [header] [Sw header exit]
///   [Lw body entry] [body] [Sw body exit] [J header]
///   [exit_label] [Lw post-loop] [rest...]
fn walk_loop(
    state: &mut WalkState<'_>,
    tree: &RegionTree,
    header: RegionId,
    body: RegionId,
    header_label: crate::vinst::LabelId,
    exit_label: crate::vinst::LabelId,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,
) -> Result<(), AllocError> {
    use crate::fa_alloc::liveness::analyze_liveness;
    use crate::region::REGION_ID_NONE;
    use crate::rv32::inst::PInst;

    // Compute the full set of vregs that must be spilled at every loop boundary.
    // This is the union of:
    //   - vregs live after the loop (post_loop)
    //   - vregs used in the header (live_in of header)
    //   - vregs used in the body (live_in of body)
    // Without this, loop-carried values (like a loop counter) that aren't live
    // after the loop would never be spilled at the body exit, causing the header
    // to always reload stale initial values.
    let header_liveness = analyze_liveness(tree, header, vinsts, vreg_pool);
    let body_liveness = analyze_liveness(tree, body, vinsts, vreg_pool);
    let mut loop_boundary = header_liveness.live_in.union(&body_liveness.live_in);
    // Include precolored registers (like a0→v0 for vmctx) in the loop
    // boundary set.  iter_occupied only returns ALLOC_POOL; we need all regs.
    for (_preg, vreg) in state.pool.iter_all_occupied() {
        loop_boundary.insert(vreg);
    }

    // 1. Flush to spill slots for the post-loop boundary.
    let _post_loop_live = state.flush_to_slots();

    // 2. Emit exit label.
    state.pinsts.push(PInst::Label { id: exit_label });

    // 3. Emit back-edge J to header.
    state.pinsts.push(PInst::J {
        target: header_label,
    });

    // 4. Process body.
    if body != REGION_ID_NONE {
        state.seed_pool_from_regset(&loop_boundary);
        state.emit_exit_spills();
        walk_region(state, tree, body, vinsts, vreg_pool, func_abi)?;
        let _body_entry_live = state.flush_to_slots();
    }

    // 5. Process header.
    if header != REGION_ID_NONE {
        state.seed_pool_from_regset(&loop_boundary);
        state.emit_exit_spills();
        walk_region(state, tree, header, vinsts, vreg_pool, func_abi)?;
        let _header_entry_live = state.flush_to_slots();
    }

    // 6. Emit header label.
    state.pinsts.push(PInst::Label { id: header_label });

    // 7. Pre-loop entry bridge: seed pool with loop_boundary vregs and emit
    //    stores so the pre-loop code's register assignments flow into the
    //    loop's spill-slot convention.
    state.seed_pool_from_regset(&loop_boundary);
    state.emit_exit_spills();

    Ok(())
}

/// Process a single instruction in the backward walk.
fn process_inst(
    state: &mut WalkState<'_>,
    idx: usize,
    vinst: &VInst,
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,
) -> Result<(), AllocError> {
    // Handle branch and control flow instructions first
    match vinst {
        VInst::Label(id, _) => {
            state
                .pinsts
                .push(crate::rv32::inst::PInst::Label { id: *id });
            return Ok(());
        }
        VInst::BrIf {
            cond,
            target,
            invert,
            ..
        } => {
            let mut decision = String::new();
            let cond_vregs = vec![*cond];
            let cond_pregs = resolve_uses(state, &cond_vregs, &mut decision)?;
            let cond_preg = cond_pregs[0];

            if *invert {
                state.pinsts.push(crate::rv32::inst::PInst::Beq {
                    src1: cond_preg,
                    src2: 0,
                    target: *target,
                });
            } else {
                state.pinsts.push(crate::rv32::inst::PInst::Bne {
                    src1: cond_preg,
                    src2: 0,
                    target: *target,
                });
            }

            let state_str = format_pool_state(&state.pool);
            state.trace.push(TraceEntry {
                vinst_idx: idx,
                vinst_mnemonic: "BrIf".into(),
                decision,
                register_state: state_str,
            });
            return Ok(());
        }
        VInst::Br { target, .. } => {
            state
                .pinsts
                .push(crate::rv32::inst::PInst::J { target: *target });

            let state_str = format_pool_state(&state.pool);
            state.trace.push(TraceEntry {
                vinst_idx: idx,
                vinst_mnemonic: "Br".into(),
                decision: String::new(),
                register_state: state_str,
            });
            return Ok(());
        }
        VInst::Call { .. } => return process_call(state, idx, vinst, vreg_pool, func_abi),
        _ => {}
    }

    let mut decision = String::new();

    // 1. Defs: in backward walk, this is where the value dies — free its reg
    // For defs not currently in a register (dead values), allocate temporarily
    let mut def_pregs = Vec::new();
    vinst.for_each_def(vreg_pool, |d| {
        if let Some(preg) = state.pool.home(d) {
            state.pool.free(preg);
            def_pregs.push((d, preg));
        } else {
            // Dead value: not currently in a register (no uses)
            // Allocate a register temporarily for emitting the instruction
            let (preg, evicted) = state.pool.alloc(d);
            if let Some(ev) = evicted {
                // Spill the evicted vreg
                let ev_slot = state.spill.get_or_assign(ev);
                let offset = spill_fp_offset(ev_slot);
                state.pinsts.push(PInst::Sw {
                    src: preg,
                    base: FP_REG,
                    offset,
                });
            }
            def_pregs.push((d, preg));
            // Will be freed after emission
        }
    });

    // 2. Uses: in backward walk, this is where the value is born — ensure in reg
    let mut use_vregs = Vec::new();
    vinst.for_each_use(vreg_pool, |u| {
        use_vregs.push(u);
    });
    let resolved_uses = resolve_uses(state, &use_vregs, &mut decision)?;

    // 3. Emit PInst
    // emit_vinst returns forward-order sequences. Since we're building a
    // backward list (reversed at the end), push in reverse so multi-instruction
    // sequences end up in correct forward order after the final reversal.
    let emitted = emit_vinst(vinst, &def_pregs, &resolved_uses)?;
    state.pinsts.extend(emitted.into_iter().rev());

    // Free registers allocated for dead values
    for (vreg, preg) in &def_pregs {
        if state.pool.home(*vreg).is_some() {
            state.pool.free(*preg);
        }
    }

    // 4. Record trace
    let state_str = format_pool_state(&state.pool);
    state.trace.push(TraceEntry {
        vinst_idx: idx,
        vinst_mnemonic: vinst.mnemonic().into(),
        decision,
        register_state: state_str,
    });

    Ok(())
}

/// Process a Call instruction in the backward walk.
fn process_call(
    state: &mut WalkState<'_>,
    idx: usize,
    vinst: &VInst,
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,
) -> Result<(), AllocError> {
    use crate::rv32::abi::{ARG_REGS, RET_REGS};
    use crate::rv32::inst::SymbolRef;
    use alloc::format;

    let (target, args, rets, callee_uses_sret) = match vinst {
        VInst::Call {
            target,
            args,
            rets,
            callee_uses_sret,
            ..
        } => (*target, *args, *rets, *callee_uses_sret),
        _ => unreachable!(),
    };

    if args.count as usize > ARG_REGS.len() {
        return Err(AllocError::TooManyArgs);
    }

    // For sret: args are shifted by one (a1, a2, ... instead of a0, a1, ...)
    let effective_arg_count = if callee_uses_sret {
        args.count as usize + 1
    } else {
        args.count as usize
    };
    if effective_arg_count > ARG_REGS.len() {
        return Err(AllocError::TooManyArgs);
    }

    let mut decision = String::new();

    // Step 1: Process defs (return values) — free their registers
    // Track which returns were originally live (in the pool)
    let ret_vregs: Vec<VReg> = rets.vregs(vreg_pool).to_vec();
    let mut ret_pregs = Vec::new();
    let mut ret_was_live = Vec::new();
    for (i, &rv) in ret_vregs.iter().enumerate() {
        if let Some(preg) = state.pool.home(rv) {
            state.pool.free(preg);
            ret_pregs.push((rv, preg));
            ret_was_live.push(true);
        } else {
            let ret_preg = RET_REGS[i].hw;
            ret_pregs.push((rv, ret_preg));
            ret_was_live.push(false);
        }
    }

    // Step 2: Emit reloads for live vregs in caller-saved regs (post-call)
    let clobber = func_abi.call_clobbers();
    let mut clobbered_vregs: Vec<(VReg, PReg)> = Vec::new();
    for (preg, vreg) in state.pool.iter_occupied().collect::<Vec<_>>() {
        let abi_preg = crate::abi::PReg::int(preg);
        if clobber.contains(abi_preg) {
            let slot = state.spill.get_or_assign(vreg);
            let offset = spill_fp_offset(slot);
            state.pinsts.push(PInst::Lw {
                dst: preg,
                base: FP_REG,
                offset,
            });
            clobbered_vregs.push((vreg, preg));
            decision.push_str(&format!(" reload v{}←[fp{}]", vreg.0, spill_fp_offset(slot)));
        }
    }

    // Step 3: Emit reloads from sret buffer (post-call in forward order)
    // For sret: load return values from the sret buffer into their assigned regs
    if callee_uses_sret {
        for (i, &(rv, preg)) in ret_pregs.iter().enumerate() {
            if ret_was_live[i] {
                // Load from sret buffer at offset i*4
                // TODO: get actual sret buffer offset from func_abi
                // For now, use a placeholder offset
                let sret_offset = -((func_abi.sret_word_count().unwrap_or(4) * 4 + 16) as i32);
                let offset = sret_offset + (i as i32 * 4);
                state.pinsts.push(PInst::Lw {
                    dst: preg,
                    base: FP_REG,
                    offset,
                });
                decision.push_str(&format!(" sret_load v{}←[sret+{}]", rv.0, i * 4));
            }
        }
    }

    // Step 4+5: Return value moves, then Call
    // Backward walk: push order is reversed. We want forward order:
    //   Call → Mv(ret_reg → assigned_reg)
    // So in backward list we push: Mv first, then Call
    // (last pushed = first after reversal)
    if !callee_uses_sret {
        for (i, &(rv, preg)) in ret_pregs.iter().enumerate() {
            let ret_reg = RET_REGS[i].hw;
            if preg != ret_reg {
                state.pinsts.push(PInst::Mv {
                    dst: preg,
                    src: ret_reg,
                });
            }
            if ret_was_live[i] {
                state.pool.alloc_fixed(preg, rv);
            }
        }
    } else {
        for (i, &(rv, preg)) in ret_pregs.iter().enumerate() {
            if ret_was_live[i] {
                state.pool.alloc_fixed(preg, rv);
            }
        }
    }

    let sym_name = String::from(state.symbols.name(target));
    state.pinsts.push(PInst::Call {
        target: SymbolRef { name: sym_name },
    });

    // Step 6: Resolve args and move to ARG_REGS
    // For sret: args start at a1 (index 1) instead of a0 (index 0)
    let arg_start_idx = if callee_uses_sret { 1 } else { 0 };
    let arg_vregs: Vec<VReg> = args.vregs(vreg_pool).to_vec();
    for (i, &av) in arg_vregs.iter().enumerate() {
        let arg_reg = ARG_REGS[arg_start_idx + i].hw;
        let src = if let Some(p) = state.pool.home(av) {
            state.pool.touch(p);
            p
        } else if let Some(slot) = state.spill.has_slot(av) {
            let (p, evicted) = state.pool.alloc(av);
            if let Some(ev) = evicted {
                let ev_slot = state.spill.get_or_assign(ev);
                let offset = spill_fp_offset(ev_slot);
                state.pinsts.push(PInst::Sw {
                    src: p,
                    base: FP_REG,
                    offset,
                });
            }
            let offset = spill_fp_offset(slot);
            state.pinsts.push(PInst::Lw {
                dst: p,
                base: FP_REG,
                offset,
            });
            p
        } else {
            let (p, evicted) = state.pool.alloc(av);
            if let Some(ev) = evicted {
                let ev_slot = state.spill.get_or_assign(ev);
                let offset = spill_fp_offset(ev_slot);
                state.pinsts.push(PInst::Sw {
                    src: p,
                    base: FP_REG,
                    offset,
                });
            }
            p
        };
        if src != arg_reg {
            decision.push_str(&format!(
                " mv v{}: {}→{}",
                av.0,
                gpr::reg_name(src),
                gpr::reg_name(arg_reg)
            ));
            state.pinsts.push(PInst::Mv { dst: arg_reg, src });
        } else {
            decision.push_str(&format!(" v{} already in {}", av.0, gpr::reg_name(arg_reg)));
        }
    }

    // Step 7: For sret, set up a0 with sret buffer pointer
    // In forward: addi a0, fp, sret_offset (pre-call)
    // In backward: this is done after args, before clobber spills
    if callee_uses_sret {
        // TODO: get actual sret buffer offset from func_abi/frame
        // For now, use a placeholder offset
        let sret_offset = -((func_abi.sret_word_count().unwrap_or(4) * 4 + 16) as i32);
        state.pinsts.push(crate::rv32::inst::PInst::Addi {
            dst: 10, // a0
            src: FP_REG,
            imm: sret_offset,
        });
        decision.push_str(&format!(" sret_ptr a0=fp[{}]", sret_offset));
    }

    // Step 8: Spill clobbered vregs (pre-call in execution)
    for &(vreg, preg) in &clobbered_vregs {
        let slot = state.spill.get_or_assign(vreg);
        let offset = spill_fp_offset(slot);
        state.pinsts.push(PInst::Sw {
            src: preg,
            base: FP_REG,
            offset,
        });
        state.pool.free(preg);
        decision.push_str(&format!(" spill v{}→[fp{}]", vreg.0, spill_fp_offset(slot)));
    }

    // Record trace
    let state_str = format_pool_state(&state.pool);
    state.trace.push(TraceEntry {
        vinst_idx: idx,
        vinst_mnemonic: "Call".into(),
        decision,
        register_state: state_str,
    });

    Ok(())
}

/// Resolve use-vregs to PRegs, handling allocation, reload, and spill.
fn resolve_uses(
    state: &mut WalkState,
    use_vregs: &[VReg],
    decision: &mut String,
) -> Result<Vec<PReg>, AllocError> {
    use alloc::format;

    let mut resolved = Vec::with_capacity(use_vregs.len());
    for &vreg in use_vregs {
        let preg = if let Some(p) = state.pool.home(vreg) {
            // Already in a register
            state.pool.touch(p);
            p
        } else if let Some(slot) = state.spill.has_slot(vreg) {
            // Spilled — reload
            let (p, evicted) = state.pool.alloc(vreg);
            if let Some(ev) = evicted {
                // Spill the evicted vreg
                let ev_slot = state.spill.get_or_assign(ev);
                let offset = spill_fp_offset(ev_slot);
                state.pinsts.push(PInst::Sw {
                    src: p,
                    base: FP_REG,
                    offset,
                });
                decision.push_str(&format!(" spill v{} to [fp{}]", ev.0, spill_fp_offset(ev_slot)));
            }
            let offset = spill_fp_offset(slot);
            state.pinsts.push(PInst::Lw {
                dst: p,
                base: crate::rv32::gpr::FP_REG,
                offset,
            });
            decision.push_str(&format!(" reload v{}→{}", vreg.0, gpr::reg_name(p)));
            p
        } else {
            // First time seeing this vreg — allocate
            let (p, evicted) = state.pool.alloc(vreg);
            if let Some(ev) = evicted {
                // Spill the evicted vreg
                let ev_slot = state.spill.get_or_assign(ev);
                let offset = spill_fp_offset(ev_slot);
                state.pinsts.push(PInst::Sw {
                    src: p,
                    base: FP_REG,
                    offset,
                });
                decision.push_str(&format!(" spill v{} to [fp{}]", ev.0, spill_fp_offset(ev_slot)));
            }
            decision.push_str(&format!(" v{}→{}", vreg.0, gpr::reg_name(p)));
            p
        };
        resolved.push(preg);
    }
    Ok(resolved)
}

fn format_pool_state(pool: &RegPool) -> String {
    use alloc::format;
    let occupied = pool.occupied_count();
    let mut parts = Vec::new();
    // Show vreg->preg mappings for all 32 registers
    for i in 0..32u16 {
        if let Some(preg) = pool.home(crate::vinst::VReg(i)) {
            parts.push(format!("v{}→{}", i, gpr::reg_name(preg)));
        }
    }
    if parts.is_empty() {
        format!("{}/{} used, empty", occupied, ALLOC_POOL.len())
    } else {
        format!(
            "{}/{} used: {}",
            occupied,
            ALLOC_POOL.len(),
            parts.join(", ")
        )
    }
}

/// Emit physical instructions for a VInst.
/// Returns the PInst sequence for this instruction.
fn emit_vinst(
    vinst: &VInst,
    def_pregs: &[(VReg, PReg)],
    use_pregs: &[PReg],
) -> Result<Vec<PInst>, AllocError> {
    use crate::rv32::gpr::SCRATCH;
    use crate::vinst::IcmpCond;

    // Helper to get the def PReg (first def)
    let dst = || def_pregs[0].1;
    // Helpers to get use PRegs
    let src1 = || use_pregs[0];
    let src2 = || use_pregs[1];

    match vinst {
        // Arithmetic: dst = op(src1, src2)
        VInst::Add32 { .. } => Ok(vec![PInst::Add {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::Sub32 { .. } => Ok(vec![PInst::Sub {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::Mul32 { .. } => Ok(vec![PInst::Mul {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::And32 { .. } => Ok(vec![PInst::And {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::Or32 { .. } => Ok(vec![PInst::Or {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::Xor32 { .. } => Ok(vec![PInst::Xor {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::Shl32 { .. } => Ok(vec![PInst::Sll {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::ShrS32 { .. } => Ok(vec![PInst::Sra {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::ShrU32 { .. } => Ok(vec![PInst::Srl {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::DivS32 { .. } => Ok(vec![PInst::Div {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::DivU32 { .. } => Ok(vec![PInst::Divu {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::RemS32 { .. } => Ok(vec![PInst::Rem {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),
        VInst::RemU32 { .. } => Ok(vec![PInst::Remu {
            dst: dst(),
            src1: src1(),
            src2: src2(),
        }]),

        // Unary: dst = op(src)
        VInst::Neg32 { .. } => Ok(vec![PInst::Neg {
            dst: dst(),
            src: src1(),
        }]),
        VInst::Bnot32 { .. } => Ok(vec![PInst::Not {
            dst: dst(),
            src: src1(),
        }]),
        VInst::Mov32 { .. } => {
            if dst() != src1() {
                Ok(vec![PInst::Mv {
                    dst: dst(),
                    src: src1(),
                }])
            } else {
                Ok(vec![])
            }
        }

        // Immediate
        VInst::IConst32 { val, .. } => Ok(vec![PInst::Li {
            dst: dst(),
            imm: *val,
        }]),

        // Memory
        VInst::Load32 { offset, .. } => Ok(vec![PInst::Lw {
            dst: dst(),
            base: src1(),
            offset: *offset,
        }]),
        VInst::Store32 { offset, .. } => {
            // Store: src=use[0], base=use[1]
            Ok(vec![PInst::Sw {
                src: src1(),
                base: src2(),
                offset: *offset,
            }])
        }
        VInst::SlotAddr { slot, .. } => Ok(vec![PInst::SlotAddr {
            dst: dst(),
            slot: *slot,
        }]),
        VInst::MemcpyWords { size, .. } => Ok(vec![PInst::MemcpyWords {
            dst: src1(),
            src: src2(),
            size: *size,
        }]),

        // Compare — multi-instruction sequences using SCRATCH
        VInst::Icmp32 { cond, .. } => {
            let (dst_p, l, r) = (dst(), src1(), src2());
            match cond {
                IcmpCond::Eq => Ok(vec![
                    PInst::Xor {
                        dst: SCRATCH,
                        src1: l,
                        src2: r,
                    },
                    PInst::Seqz {
                        dst: dst_p,
                        src: SCRATCH,
                    },
                ]),
                IcmpCond::Ne => Ok(vec![
                    PInst::Xor {
                        dst: SCRATCH,
                        src1: l,
                        src2: r,
                    },
                    PInst::Snez {
                        dst: dst_p,
                        src: SCRATCH,
                    },
                ]),
                IcmpCond::LtS => Ok(vec![PInst::Slt {
                    dst: dst_p,
                    src1: l,
                    src2: r,
                }]),
                IcmpCond::LeS => Ok(vec![
                    PInst::Slt {
                        dst: SCRATCH,
                        src1: r,
                        src2: l,
                    },
                    PInst::Seqz {
                        dst: dst_p,
                        src: SCRATCH,
                    },
                ]),
                IcmpCond::GtS => Ok(vec![PInst::Slt {
                    dst: dst_p,
                    src1: r,
                    src2: l,
                }]),
                IcmpCond::GeS => Ok(vec![
                    PInst::Slt {
                        dst: SCRATCH,
                        src1: l,
                        src2: r,
                    },
                    PInst::Seqz {
                        dst: dst_p,
                        src: SCRATCH,
                    },
                ]),
                IcmpCond::LtU => Ok(vec![PInst::Sltu {
                    dst: dst_p,
                    src1: l,
                    src2: r,
                }]),
                IcmpCond::LeU => Ok(vec![
                    PInst::Sltu {
                        dst: SCRATCH,
                        src1: r,
                        src2: l,
                    },
                    PInst::Seqz {
                        dst: dst_p,
                        src: SCRATCH,
                    },
                ]),
                IcmpCond::GtU => Ok(vec![PInst::Sltu {
                    dst: dst_p,
                    src1: r,
                    src2: l,
                }]),
                IcmpCond::GeU => Ok(vec![
                    PInst::Sltu {
                        dst: SCRATCH,
                        src1: l,
                        src2: r,
                    },
                    PInst::Seqz {
                        dst: dst_p,
                        src: SCRATCH,
                    },
                ]),
            }
        }

        VInst::IeqImm32 { imm, .. } => {
            let (dst_p, s) = (dst(), src1());
            Ok(vec![
                PInst::Li {
                    dst: SCRATCH,
                    imm: *imm,
                },
                PInst::Xor {
                    dst: SCRATCH,
                    src1: s,
                    src2: SCRATCH,
                },
                PInst::Seqz {
                    dst: dst_p,
                    src: SCRATCH,
                },
            ])
        }

        VInst::Label(..) => Ok(vec![]),

        // Select32: dst = cond ? if_true : if_false
        // cond is 0 or 1 from Icmp/IeqImm; negate to bitmask (0 or 0xFFFFFFFF)
        // Uses: [cond, if_true, if_false]
        VInst::Select32 { .. } => {
            let (dst_p, cond_p, true_p, false_p) =
                (dst(), use_pregs[0], use_pregs[1], use_pregs[2]);
            Ok(vec![
                PInst::Neg {
                    dst: SCRATCH,
                    src: cond_p,
                }, // 0→0, 1→0xFFFFFFFF
                PInst::Sub {
                    dst: dst_p,
                    src1: true_p,
                    src2: false_p,
                },
                PInst::And {
                    dst: dst_p,
                    src1: dst_p,
                    src2: SCRATCH,
                },
                PInst::Add {
                    dst: dst_p,
                    src1: dst_p,
                    src2: false_p,
                },
            ])
        }

        VInst::Ret { .. } => {
            let mut out = Vec::new();
            for (k, &src) in use_pregs.iter().enumerate() {
                let dst_ret = crate::rv32::gpr::RET_REGS[k];
                if src != dst_ret {
                    out.push(PInst::Mv { dst: dst_ret, src });
                }
            }
            out.push(PInst::Ret);
            Ok(out)
        }

        // Handled elsewhere or rejected in process_inst
        _ => Err(AllocError::UnsupportedControlFlow),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::{Region, RegionTree};
    use crate::rv32::gpr::ALLOC_POOL;
    use crate::vinst::{ModuleSymbols, SRC_OP_NONE, VInst, VReg};
    use alloc::string::String;
    use alloc::vec::Vec;

    fn test_abi() -> crate::abi::FuncAbi {
        crate::rv32::abi::func_abi_rv32(
            &lps_shared::LpsFnSig {
                name: String::from("test"),
                return_type: lps_shared::LpsType::Void,
                parameters: vec![],
            },
            0,
        )
    }

    #[test]
    fn regpool_alloc_free() {
        let mut pool = RegPool::new();
        let (p1, evicted) = pool.alloc(VReg(0));
        assert!(evicted.is_none());
        assert_eq!(pool.home(VReg(0)), Some(p1));

        pool.free(p1);
        assert_eq!(pool.home(VReg(0)), None);
    }

    #[test]
    fn regpool_evicts_lru() {
        let mut pool = RegPool::new();
        let n = ALLOC_POOL.len();
        // Fill all allocatable regs
        for i in 0..n {
            let (_, evicted) = pool.alloc(VReg(i as u16));
            assert!(evicted.is_none());
        }
        // Next alloc should evict
        let (_, evicted) = pool.alloc(VReg(n as u16));
        assert!(evicted.is_some());
    }

    #[test]
    fn regpool_alloc_fixed() {
        let mut pool = RegPool::new();
        let (p, _) = pool.alloc(VReg(0));
        // Force vreg 1 into p, evicting vreg 0
        let evicted = pool.alloc_fixed(p, VReg(1));
        assert_eq!(evicted, Some(VReg(0)));
        assert_eq!(pool.home(VReg(1)), Some(p));
        assert_eq!(pool.home(VReg(0)), None);
    }

    #[test]
    fn regpool_touch_moves_to_mru() {
        let mut pool = RegPool::new();
        let n = ALLOC_POOL.len();

        // Fill all allocatable regs
        for i in 0..n {
            let (_, evicted) = pool.alloc(VReg(i as u16));
            assert!(evicted.is_none());
        }

        // Touch the first allocated reg (now LRU) to move it to MRU
        let first_preg = ALLOC_POOL[0];
        pool.touch(first_preg);

        // Next eviction should be the second allocated reg (now LRU)
        let (_, evicted) = pool.alloc(VReg(n as u16));
        // VReg(1) was allocated to ALLOC_POOL[1]
        assert_eq!(evicted, Some(VReg(1)));
    }

    #[test]
    fn walk_region_allocates_simple() {
        let vinsts = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 1,
                src_op: SRC_OP_NONE,
            },
            VInst::IConst32 {
                dst: VReg(1),
                val: 2,
                src_op: SRC_OP_NONE,
            },
            VInst::Add32 {
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ];
        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 3 });
        tree.root = root;
        let symbols = ModuleSymbols::default();
        let abi = test_abi();

        let mut state = WalkState::new(4, &symbols);
        walk_region(&mut state, &tree, root, &vinsts, &[], &abi).unwrap();

        assert_eq!(state.trace.entries.len(), 3);
        assert!(!state.trace.entries[0].decision.contains("STUB"));
        assert!(state.trace.entries[0].register_state.contains("used"));
    }

    #[test]
    fn walk_region_handles_loop_control_flow() {
        // Loop now works
        let vinsts = vec![VInst::IConst32 {
            dst: VReg(0),
            val: 1,
            src_op: SRC_OP_NONE,
        }];
        let mut tree = RegionTree::new();
        let header = tree.push(Region::Linear { start: 0, end: 1 });
        let body = tree.push(Region::Linear { start: 0, end: 0 });
        let root = tree.push(Region::Loop {
            header,
            body,
            header_label: 0,
            exit_label: 1,
        });
        tree.root = root;
        let symbols = ModuleSymbols::default();
        let abi = test_abi();

        let mut state = WalkState::new(4, &symbols);
        let result = walk_region(&mut state, &tree, root, &vinsts, &[], &abi);
        assert!(result.is_ok());
    }

    #[test]
    fn walk_region_handles_spill() {
        let n = ALLOC_POOL.len() + 2;
        let mut vinsts: Vec<VInst> = Vec::new();

        for i in 0..n {
            vinsts.push(VInst::IConst32 {
                dst: VReg(i as u16),
                val: i as i32,
                src_op: SRC_OP_NONE,
            });
        }

        for i in n..(n + ALLOC_POOL.len()) {
            vinsts.push(VInst::Add32 {
                dst: VReg(i as u16),
                src1: VReg(((i - n) % n) as u16),
                src2: VReg(((i - n + 1) % n) as u16),
                src_op: SRC_OP_NONE,
            });
        }

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear {
            start: 0,
            end: vinsts.len() as u16,
        });
        tree.root = root;
        let symbols = ModuleSymbols::default();
        let abi = test_abi();

        let mut state = WalkState::new(vinsts.len(), &symbols);
        walk_region(&mut state, &tree, root, &vinsts, &[], &abi).unwrap();

        assert_eq!(state.trace.entries.len(), vinsts.len());
        let has_alloc = state.trace.entries.iter().any(|e| e.decision.contains('→'));
        assert!(has_alloc, "Expected allocation decisions in trace");
    }

    #[test]
    fn process_call_dead_return_not_allocated() {
        // Call with a dead return value (ret v0 is not used anywhere)
        // The dead return should not occupy a register after the call
        use crate::vinst::{SymbolId, VRegSlice};

        // vreg_pool: [v0, v1] where v0 is the dead return, v1 is the arg
        let vreg_pool = vec![VReg(0), VReg(1)];

        let vinsts = vec![
            // IConst defines v1, which is used as the call argument
            VInst::IConst32 {
                dst: VReg(1),
                val: 42,
                src_op: SRC_OP_NONE,
            },
            // Call returns v0 (dead), takes v1 as arg (vreg_pool index 1)
            VInst::Call {
                target: SymbolId(0),
                args: VRegSlice { start: 1, count: 1 },
                rets: VRegSlice { start: 0, count: 1 }, // v0 is the return
                callee_uses_sret: false,
                src_op: SRC_OP_NONE,
            },
        ];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 2 });
        tree.root = root;

        let mut symbols = ModuleSymbols::default();
        symbols.intern("helper");
        let abi = test_abi();

        let mut state = WalkState::new(4, &symbols);
        walk_region(&mut state, &tree, root, &vinsts, &vreg_pool, &abi).unwrap();

        // After processing:
        // - v0 (dead return) should NOT be in the pool
        // - v1 (arg) was freed after use, so also not in pool
        // The key point: v0 was never allocated even though it was a return
        assert!(
            state.pool.home(VReg(0)).is_none(),
            "Dead return value v0 should not occupy a register after call"
        );

        // Count occupied registers to ensure dead return didn't waste a slot
        let occupied = state.pool.occupied_count();
        assert_eq!(
            occupied, 0,
            "No registers should be occupied after call with dead return"
        );
    }

    #[test]
    fn process_call_live_return_and_args() {
        // v0 = IConst(1); v1 = IConst(2); v2 = Call(helper, args=[v0,v1]); v3 = Add(v2, v0)
        // v2 is a live return (used by Add), v0 is live across the call
        use crate::vinst::{SymbolId, VRegSlice};

        let vreg_pool = vec![VReg(0), VReg(1), VReg(2)];
        let vinsts = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 1,
                src_op: SRC_OP_NONE,
            },
            VInst::IConst32 {
                dst: VReg(1),
                val: 2,
                src_op: SRC_OP_NONE,
            },
            VInst::Call {
                target: SymbolId(0),
                args: VRegSlice { start: 0, count: 2 }, // v0, v1
                rets: VRegSlice { start: 2, count: 1 }, // v2
                callee_uses_sret: false,
                src_op: SRC_OP_NONE,
            },
            VInst::Add32 {
                dst: VReg(3),
                src1: VReg(2),
                src2: VReg(0),
                src_op: SRC_OP_NONE,
            },
        ];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 4 });
        tree.root = root;
        let mut symbols = ModuleSymbols::default();
        symbols.intern("helper");
        let abi = test_abi();

        let mut state = WalkState::new(8, &symbols);
        walk_region(&mut state, &tree, root, &vinsts, &vreg_pool, &abi).unwrap();
        state.pinsts.reverse();

        // Should contain a call instruction
        assert!(state.pinsts.iter().any(|p| matches!(p, PInst::Call { .. })));
        // Should contain Mv instructions for arg placement (a0, a1)
        let has_mv = state.pinsts.iter().any(|p| matches!(p, PInst::Mv { .. }));
        assert!(has_mv, "Expected Mv for arg or ret placement");
        // v0 is live across the call (used in Add), so it should be spilled/reloaded.
        // ALLOC_POOL starts with t0,t1,t2 (caller-saved), so v0 WILL get clobbered.
        let has_sw = state.pinsts.iter().any(|p| matches!(p, PInst::Sw { .. }));
        let has_lw = state.pinsts.iter().any(|p| matches!(p, PInst::Lw { .. }));
        assert!(has_sw, "Expected Sw for caller-saved spill around call");
        assert!(has_lw, "Expected Lw for caller-saved reload around call");
    }

    #[test]
    fn process_call_no_args_no_returns() {
        use crate::vinst::{SymbolId, VRegSlice};

        let vinsts = vec![VInst::Call {
            target: SymbolId(0),
            args: VRegSlice { start: 0, count: 0 },
            rets: VRegSlice { start: 0, count: 0 },
            callee_uses_sret: false,
            src_op: SRC_OP_NONE,
        }];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 1 });
        tree.root = root;
        let mut symbols = ModuleSymbols::default();
        symbols.intern("void_fn");
        let abi = test_abi();

        let mut state = WalkState::new(1, &symbols);
        walk_region(&mut state, &tree, root, &vinsts, &[], &abi).unwrap();
        state.pinsts.reverse();

        // Just a call instruction, no moves needed
        assert_eq!(
            state
                .pinsts
                .iter()
                .filter(|p| matches!(p, PInst::Call { .. }))
                .count(),
            1,
        );
        assert_eq!(state.pool.occupied_count(), 0);
    }

    #[test]
    fn process_call_sret_handled() {
        // Sret calls are now supported
        use crate::vinst::{SymbolId, VRegSlice};

        let vinsts = vec![VInst::Call {
            target: SymbolId(0),
            args: VRegSlice { start: 0, count: 0 },
            rets: VRegSlice { start: 0, count: 0 },
            callee_uses_sret: true,
            src_op: SRC_OP_NONE,
        }];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 1 });
        tree.root = root;
        let mut symbols = ModuleSymbols::default();
        symbols.intern("sret_fn");
        let abi = test_abi();

        let mut state = WalkState::new(1, &symbols);
        let result = walk_region(&mut state, &tree, root, &vinsts, &[], &abi);
        assert!(result.is_ok());
    }

    #[test]
    fn process_call_too_many_args_rejected() {
        use crate::vinst::{SymbolId, VRegSlice};

        // 9 args — exceeds the 8-register limit
        let vreg_pool: Vec<VReg> = (0..9).map(|i| VReg(i)).collect();
        let vinsts = vec![VInst::Call {
            target: SymbolId(0),
            args: VRegSlice { start: 0, count: 9 },
            rets: VRegSlice { start: 0, count: 0 },
            callee_uses_sret: false,
            src_op: SRC_OP_NONE,
        }];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 1 });
        tree.root = root;
        let mut symbols = ModuleSymbols::default();
        symbols.intern("many_args");
        let abi = test_abi();

        let mut state = WalkState::new(10, &symbols);
        let result = walk_region(&mut state, &tree, root, &vinsts, &vreg_pool, &abi);
        assert!(matches!(result, Err(AllocError::TooManyArgs)));
    }

    #[test]
    fn process_call_clobber_spill_reload() {
        // v0 = IConst(10); Call(helper); v1 = Mov(v0)
        // v0 is live across the call, and ALLOC_POOL[0] = t0 (caller-saved).
        // So v0 must be spilled before and reloaded after the call.
        use crate::vinst::{SymbolId, VRegSlice};

        let vinsts = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 10,
                src_op: SRC_OP_NONE,
            },
            VInst::Call {
                target: SymbolId(0),
                args: VRegSlice { start: 0, count: 0 },
                rets: VRegSlice { start: 0, count: 0 },
                callee_uses_sret: false,
                src_op: SRC_OP_NONE,
            },
            VInst::Mov32 {
                dst: VReg(1),
                src: VReg(0),
                src_op: SRC_OP_NONE,
            },
        ];

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 3 });
        tree.root = root;
        let mut symbols = ModuleSymbols::default();
        symbols.intern("helper");
        let abi = test_abi();

        let mut state = WalkState::new(4, &symbols);
        walk_region(&mut state, &tree, root, &vinsts, &[], &abi).unwrap();
        state.pinsts.reverse();

        // v0 allocated to ALLOC_POOL[0] = t0 (caller-saved).
        // Must have Sw (spill) before call and Lw (reload) after call.
        let call_idx = state
            .pinsts
            .iter()
            .position(|p| matches!(p, PInst::Call { .. }))
            .unwrap();

        // There should be a Sw before the call
        let has_spill_before = state.pinsts[..call_idx]
            .iter()
            .any(|p| matches!(p, PInst::Sw { .. }));
        assert!(
            has_spill_before,
            "Expected spill (Sw) before call in pinsts: {:?}",
            &state.pinsts
        );

        // There should be a Lw after the call
        let has_reload_after = state.pinsts[call_idx + 1..]
            .iter()
            .any(|p| matches!(p, PInst::Lw { .. }));
        assert!(
            has_reload_after,
            "Expected reload (Lw) after call in pinsts: {:?}",
            &state.pinsts
        );

        // The trace for Call should mention spill and reload
        let call_trace = state
            .trace
            .entries
            .iter()
            .find(|e| e.vinst_mnemonic == "Call")
            .unwrap();
        assert!(
            call_trace.decision.contains("spill"),
            "Call trace should mention spill: {}",
            call_trace.decision
        );
        assert!(
            call_trace.decision.contains("reload"),
            "Call trace should mention reload: {}",
            call_trace.decision
        );
    }
}
