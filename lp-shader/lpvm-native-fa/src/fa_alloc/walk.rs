//! Backward walk allocator with real register allocation decisions.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::region::{Region, RegionId, RegionTree, REGION_ID_NONE};
use crate::rv32::gpr::{self, PReg, ALLOC_POOL, FP_REG};
use crate::rv32::inst::PInst;
use crate::vinst::{VInst, VReg};

use super::spill::SpillAlloc;
use super::trace::{AllocTrace, TraceEntry};

/// Allocation error types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocError {
    UnsupportedControlFlow,
    UnsupportedCall,
    UnsupportedSelect,
}

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocError::UnsupportedControlFlow => write!(f, "branches/jumps not supported"),
            AllocError::UnsupportedCall => write!(f, "calls not supported"),
            AllocError::UnsupportedSelect => write!(f, "Select32 not supported"),
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
        ALLOC_POOL.iter().filter(|&&p| self.preg_vreg[p as usize].is_some()).count()
    }

    /// Iterate over occupied (preg, vreg) pairs for allocatable registers.
    pub fn iter_occupied(&self) -> impl Iterator<Item = (PReg, VReg)> + '_ {
        ALLOC_POOL.iter().copied().filter_map(|p| {
            self.preg_vreg[p as usize].map(|v| (p, v))
        })
    }
}

/// State threaded through the backward walk.
pub struct WalkState {
    pub pool: RegPool,
    pub spill: SpillAlloc,
    pub trace: AllocTrace,
    pub pinsts: Vec<PInst>,
}

impl WalkState {
    pub fn new(num_vregs: usize) -> Self {
        Self {
            pool: RegPool::new(),
            spill: SpillAlloc::new(num_vregs),
            trace: AllocTrace::new(),
            pinsts: Vec::new(),
        }
    }
}

/// Walk a region backward with real register allocation.
/// Returns error for unsupported control flow (IfThenElse/Loop/Call).
pub fn walk_region(
    state: &mut WalkState,
    tree: &RegionTree,
    region_id: RegionId,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
) -> Result<(), AllocError> {
    if region_id == REGION_ID_NONE {
        return Ok(());
    }

    let region = &tree.nodes[region_id as usize];
    match region {
        Region::Linear { start, end } => {
            for i in (*start..*end).rev() {
                process_inst(state, i as usize, &vinsts[i as usize], vreg_pool)?;
            }
            Ok(())
        }
        Region::Seq { children_start, child_count } => {
            let start = *children_start as usize;
            let end = start + *child_count as usize;
            // Walk children in reverse (backward)
            for &child_id in tree.seq_children[start..end].iter().rev() {
                walk_region(state, tree, child_id, vinsts, vreg_pool)?;
            }
            Ok(())
        }
        Region::IfThenElse { .. } => Err(AllocError::UnsupportedControlFlow),
        Region::Loop { .. } => Err(AllocError::UnsupportedControlFlow),
    }
}

/// Process a single instruction in the backward walk.
fn process_inst(
    state: &mut WalkState,
    idx: usize,
    vinst: &VInst,
    vreg_pool: &[VReg],
) -> Result<(), AllocError> {
    // Skip labels
    if matches!(vinst, VInst::Label(..)) {
        return Ok(());
    }

    // Reject unsupported instructions
    match vinst {
        VInst::Call { .. } => return Err(AllocError::UnsupportedCall),
        VInst::Select32 { .. } => return Err(AllocError::UnsupportedSelect),
        VInst::Br { .. } | VInst::BrIf { .. } => {
            return Err(AllocError::UnsupportedControlFlow)
        }
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
                let offset = -((ev_slot as i32 + 1) * 4);
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
    let emitted = emit_vinst(vinst, &def_pregs, &resolved_uses)?;
    state.pinsts.extend(emitted);

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
                let offset = -((ev_slot as i32 + 1) * 4);
                state.pinsts.push(PInst::Sw {
                    src: p,
                    base: FP_REG,
                    offset,
                });
                decision.push_str(&format!(" spill v{} to [fp-{}]", ev.0, (ev_slot + 1) * 4));
            }
            let offset = -((slot as i32 + 1) * 4);
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
                let offset = -((ev_slot as i32 + 1) * 4);
                state.pinsts.push(PInst::Sw {
                    src: p,
                    base: FP_REG,
                    offset,
                });
                decision.push_str(&format!(" spill v{} to [fp-{}]", ev.0, (ev_slot + 1) * 4));
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
    format!("{}/{} used", occupied, ALLOC_POOL.len())
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
        VInst::Add32 { .. } => Ok(vec![PInst::Add { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::Sub32 { .. } => Ok(vec![PInst::Sub { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::Mul32 { .. } => Ok(vec![PInst::Mul { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::And32 { .. } => Ok(vec![PInst::And { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::Or32 { .. } => Ok(vec![PInst::Or { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::Xor32 { .. } => Ok(vec![PInst::Xor { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::Shl32 { .. } => Ok(vec![PInst::Sll { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::ShrS32 { .. } => Ok(vec![PInst::Sra { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::ShrU32 { .. } => Ok(vec![PInst::Srl { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::DivS32 { .. } => Ok(vec![PInst::Div { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::DivU32 { .. } => Ok(vec![PInst::Divu { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::RemS32 { .. } => Ok(vec![PInst::Rem { dst: dst(), src1: src1(), src2: src2() }]),
        VInst::RemU32 { .. } => Ok(vec![PInst::Remu { dst: dst(), src1: src1(), src2: src2() }]),

        // Unary: dst = op(src)
        VInst::Neg32 { .. } => Ok(vec![PInst::Neg { dst: dst(), src: src1() }]),
        VInst::Bnot32 { .. } => Ok(vec![PInst::Not { dst: dst(), src: src1() }]),
        VInst::Mov32 { .. } => {
            if dst() != src1() {
                Ok(vec![PInst::Mv { dst: dst(), src: src1() }])
            } else {
                Ok(vec![])
            }
        }

        // Immediate
        VInst::IConst32 { val, .. } => Ok(vec![PInst::Li { dst: dst(), imm: *val }]),

        // Memory
        VInst::Load32 { offset, .. } => {
            Ok(vec![PInst::Lw { dst: dst(), base: src1(), offset: *offset }])
        }
        VInst::Store32 { offset, .. } => {
            // Store: src=use[0], base=use[1]
            Ok(vec![PInst::Sw { src: src1(), base: src2(), offset: *offset }])
        }
        VInst::SlotAddr { slot, .. } => {
            Ok(vec![PInst::SlotAddr { dst: dst(), slot: *slot }])
        }
        VInst::MemcpyWords { size, .. } => {
            Ok(vec![PInst::MemcpyWords { dst: src1(), src: src2(), size: *size }])
        }

        // Compare — multi-instruction sequences using SCRATCH
        VInst::Icmp32 { cond, .. } => {
            let (dst_p, l, r) = (dst(), src1(), src2());
            match cond {
                IcmpCond::Eq => Ok(vec![
                    PInst::Xor { dst: SCRATCH, src1: l, src2: r },
                    PInst::Seqz { dst: dst_p, src: SCRATCH },
                ]),
                IcmpCond::Ne => Ok(vec![
                    PInst::Xor { dst: SCRATCH, src1: l, src2: r },
                    PInst::Snez { dst: dst_p, src: SCRATCH },
                ]),
                IcmpCond::LtS => Ok(vec![PInst::Slt { dst: dst_p, src1: l, src2: r }]),
                IcmpCond::LeS => Ok(vec![
                    PInst::Slt { dst: SCRATCH, src1: r, src2: l },
                    PInst::Seqz { dst: dst_p, src: SCRATCH },
                ]),
                IcmpCond::GtS => Ok(vec![PInst::Slt { dst: dst_p, src1: r, src2: l }]),
                IcmpCond::GeS => Ok(vec![
                    PInst::Slt { dst: SCRATCH, src1: l, src2: r },
                    PInst::Seqz { dst: dst_p, src: SCRATCH },
                ]),
                IcmpCond::LtU => Ok(vec![PInst::Sltu { dst: dst_p, src1: l, src2: r }]),
                IcmpCond::LeU => Ok(vec![
                    PInst::Sltu { dst: SCRATCH, src1: r, src2: l },
                    PInst::Seqz { dst: dst_p, src: SCRATCH },
                ]),
                IcmpCond::GtU => Ok(vec![PInst::Sltu { dst: dst_p, src1: r, src2: l }]),
                IcmpCond::GeU => Ok(vec![
                    PInst::Sltu { dst: SCRATCH, src1: l, src2: r },
                    PInst::Seqz { dst: dst_p, src: SCRATCH },
                ]),
            }
        }

        VInst::IeqImm32 { imm, .. } => {
            let (dst_p, s) = (dst(), src1());
            Ok(vec![
                PInst::Li { dst: SCRATCH, imm: *imm },
                PInst::Xor { dst: SCRATCH, src1: s, src2: SCRATCH },
                PInst::Seqz { dst: dst_p, src: SCRATCH },
            ])
        }

        VInst::Label(..) => Ok(vec![]),

        VInst::Ret { .. } => {
            let mut out = Vec::new();
            // Move return values to RET_REGS if not already there
            // use_pregs contains the resolved PRegs for each return value
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
    use crate::vinst::{VInst, VReg, SRC_OP_NONE};
    use super::super::trace::AllocTrace;
    use crate::rv32::gpr::ALLOC_POOL;
    use alloc::vec::Vec;

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
        // v0 = IConst32 1; v1 = IConst32 2; v2 = Add v0, v1
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: SRC_OP_NONE },
            VInst::IConst32 { dst: VReg(1), val: 2, src_op: SRC_OP_NONE },
            VInst::Add32 { dst: VReg(2), src1: VReg(0), src2: VReg(1), src_op: SRC_OP_NONE },
        ];
        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: 3 });
        tree.root = root;

        let mut state = WalkState::new(4);
        walk_region(&mut state, &tree, root, &vinsts, &[]).unwrap();

        // Should have trace entries for 3 instructions
        assert_eq!(state.trace.entries.len(), 3);
        // Trace should show real decisions (not "STUB")
        assert!(!state.trace.entries[0].decision.contains("STUB"));
        // Pool state should be tracked
        assert!(state.trace.entries[0].register_state.contains("used"));
    }

    #[test]
    fn walk_region_rejects_control_flow() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 1, src_op: SRC_OP_NONE },
        ];
        let mut tree = RegionTree::new();
        let header = tree.push(Region::Linear { start: 0, end: 1 });
        let body = tree.push(Region::Linear { start: 0, end: 0 });
        let root = tree.push(Region::Loop { header, body });
        tree.root = root;

        let mut state = WalkState::new(4);
        let result = walk_region(&mut state, &tree, root, &vinsts, &[]);
        assert!(matches!(result, Err(AllocError::UnsupportedControlFlow)));
    }

    #[test]
    fn walk_region_handles_spill() {
        // Create more live values than allocatable registers to force spill
        // Chain: v0=1; v1=2; v2=Add(v0,v1); v3=Add(v2,v0); v4=Add(v3,v1); ...
        // This creates a long chain where many vregs are live simultaneously
        let n = ALLOC_POOL.len() + 2;
        let mut vinsts: Vec<VInst> = Vec::new();

        // Create constants
        for i in 0..n {
            vinsts.push(VInst::IConst32 { dst: VReg(i as u16), val: i as i32, src_op: SRC_OP_NONE });
        }

        // Create a use chain that keeps all values live
        // Each Add uses two previous values, defining a new one
        for i in n..(n + ALLOC_POOL.len()) {
            vinsts.push(VInst::Add32 {
                dst: VReg(i as u16),
                src1: VReg(((i - n) % n) as u16),
                src2: VReg(((i - n + 1) % n) as u16),
                src_op: SRC_OP_NONE,
            });
        }

        let mut tree = RegionTree::new();
        let root = tree.push(Region::Linear { start: 0, end: vinsts.len() as u16 });
        tree.root = root;

        let mut state = WalkState::new(vinsts.len());
        walk_region(&mut state, &tree, root, &vinsts, &[]).unwrap();

        // The trace should have entries for all instructions
        assert_eq!(state.trace.entries.len(), vinsts.len());
        // Should have some decisions recorded for uses
        let has_alloc = state.trace.entries.iter().any(|e| e.decision.contains('→'));
        assert!(has_alloc, "Expected allocation decisions in trace");
    }
}
