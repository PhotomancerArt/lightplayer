//! Linear scan register allocation on a flat [`VInst`] sequence.
//!
//! Uses half-open program points `[start, end)`: uses at instruction `i` extend liveness to
//! `2*i+1`; defs start at `2*i+1` so operands and results at the same instruction do not overlap.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpir::IrFunction;

use super::{Allocation, PhysReg, RegAlloc};
use crate::abi::classify::ArgLoc;
use crate::abi::{FuncAbi, PReg, RegClass};
use crate::error::NativeError;
use crate::isa::rv32::abi::{ARG_REGS, RET_REGS, alloca_base_int, caller_saved_int};
use crate::lower::LoopRegion;
use crate::vinst::{VInst, VReg};

fn abi2_int_preg_to_phys(p: PReg) -> Result<PhysReg, NativeError> {
    match p.class {
        RegClass::Int => Ok(p.hw),
        RegClass::Float => Err(NativeError::UnassignedVReg(p.hw as u32)),
    }
}

fn sorted_allocatable_ints(set: crate::abi::PregSet) -> Vec<PhysReg> {
    let mut v: Vec<PhysReg> = set
        .iter()
        .filter(|p| p.class == RegClass::Int)
        .map(|p| p.hw)
        .collect();
    v.sort_unstable();
    v
}

fn clobber_set_from_abi(abi: &FuncAbi) -> BTreeSet<PhysReg> {
    abi.call_clobbers()
        .iter()
        .filter(|p| p.class == RegClass::Int)
        .map(|p| p.hw)
        .collect()
}

#[derive(Debug, Clone)]
struct Interval {
    vreg: VReg,
    /// Half-open range in program points.
    start: u32,
    end: u32,
    /// ABI-fixed physical register (register params).
    fixed_reg: Option<PhysReg>,
}

/// Event types for allocation tracing/debug output.
#[derive(Debug, Clone)]
enum AllocEvent {
    /// VReg becomes live at this instruction index.
    LiveStart {
        idx: usize,
        vreg: VReg,
        preg: Option<PhysReg>,
    },
    /// VReg expires after this instruction index.
    LiveEnd {
        idx: usize,
        vreg: VReg,
        preg: Option<PhysReg>,
    },
    /// Register assignment at this instruction.
    Assign {
        idx: usize,
        vreg: VReg,
        preg: PhysReg,
    },
    /// Spill decision for vreg.
    Spill { idx: usize, vreg: VReg, slot: usize },
    /// Free-form comment at instruction (reserved for future trace hooks).
    #[allow(dead_code)]
    Comment { idx: usize, msg: String },
}

/// Collected allocation trace for rendering.
#[derive(Debug, Clone, Default)]
struct AllocTrace {
    events: Vec<AllocEvent>,
}

impl AllocTrace {
    fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, event: AllocEvent) {
        self.events.push(event);
    }

    /// Render the trace as a compact columnar view showing liveness.
    /// Columns are reused: when a vreg expires, its column becomes available for new vregs.
    fn render(&self, vinsts: &[VInst]) -> String {
        use alloc::collections::BTreeMap;
        use alloc::collections::BTreeSet;

        // Build live ranges: vreg -> (start_idx, end_idx)
        let mut live_start: BTreeMap<VReg, usize> = BTreeMap::new();
        let mut live_end: BTreeMap<VReg, usize> = BTreeMap::new();
        for event in &self.events {
            match event {
                AllocEvent::LiveStart { idx, vreg, .. } => {
                    live_start.entry(*vreg).or_insert(*idx);
                }
                AllocEvent::LiveEnd { idx, vreg, .. } => {
                    live_end.insert(*vreg, *idx);
                }
                _ => {}
            }
        }

        // Build events by index for quick lookup
        let mut events_at: BTreeMap<usize, Vec<&AllocEvent>> = BTreeMap::new();
        for event in &self.events {
            let idx = match event {
                AllocEvent::LiveStart { idx, .. } => *idx,
                AllocEvent::LiveEnd { idx, .. } => *idx,
                AllocEvent::Assign { idx, .. } => *idx,
                AllocEvent::Spill { idx, .. } => *idx,
                AllocEvent::Comment { idx, .. } => *idx,
            };
            events_at.entry(idx).or_default().push(event);
        }

        // Pre-compute all live sets per instruction
        let mut live_at: Vec<BTreeSet<VReg>> = Vec::new();
        for i in 0..vinsts.len() {
            let mut live_now: BTreeSet<VReg> = BTreeSet::new();
            for (vreg, start) in &live_start {
                if *start <= i {
                    if let Some(end) = live_end.get(vreg) {
                        if *end >= i {
                            live_now.insert(*vreg);
                        }
                    }
                }
            }
            live_at.push(live_now);
        }

        // Compute max concurrent live vregs to determine column count
        let max_concurrent = live_at.iter().map(|s| s.len()).max().unwrap_or(1);

        // Assign columns with reuse: a column may take a new vreg only after the previous
        // occupant's last live row plus one blank row (see docs/design/native/alloc-debug.md).
        let mut vreg_column: BTreeMap<VReg, usize> = BTreeMap::new();
        // First instruction index at which this column may be assigned again.
        let mut col_next_assign_ok: Vec<usize> = Vec::new();

        for (i, live_now) in live_at.iter().enumerate() {
            for vreg in live_now {
                if vreg_column.contains_key(vreg) {
                    continue;
                }
                let last_live = *live_end.get(vreg).unwrap_or(&i);
                let mut assigned = false;
                for col in 0..col_next_assign_ok.len() {
                    let occupied_by_live = live_now
                        .iter()
                        .any(|v| vreg_column.get(v).copied() == Some(col));
                    if occupied_by_live {
                        continue;
                    }
                    if col_next_assign_ok[col] <= i {
                        vreg_column.insert(*vreg, col);
                        col_next_assign_ok[col] = last_live.saturating_add(2);
                        assigned = true;
                        break;
                    }
                }
                if !assigned {
                    let col = col_next_assign_ok.len();
                    vreg_column.insert(*vreg, col);
                    col_next_assign_ok.push(last_live.saturating_add(2));
                }
            }
        }

        // Render each instruction
        let mut lines = Vec::new();
        let max_cols = max_concurrent.max(1);
        let col_width = 3_usize; // "vN " slot width
        let prefix_w = max_cols * (col_width + 1);
        const IDX_W: usize = 6; // "[  0]"
        const MNE_W: usize = 11; // longest: MemcpyWords

        lines.push(format!(
            "{:<prefix_w$}{:>idx_w$}{:>idx_w$}{:<mne_w$} {}",
            "",
            "vinst",
            "lpir",
            "op",
            "inst",
            prefix_w = prefix_w,
            idx_w = IDX_W,
            mne_w = MNE_W,
        ));

        for (i, vinst) in vinsts.iter().enumerate() {
            let live_now = &live_at[i];

            // Build live prefix columns
            let mut prefix_parts = Vec::new();
            for col in 0..max_cols {
                let mut found = None;
                for (vreg, vcol) in &vreg_column {
                    if *vcol == col && live_now.contains(vreg) {
                        found = Some(*vreg);
                        break;
                    }
                }
                if let Some(vreg) = found {
                    prefix_parts.push(format!("v{:<2}", vreg.0));
                } else {
                    prefix_parts.push("   ".to_string());
                }
            }
            let prefix = prefix_parts.join(" ");

            // Collect comments for this instruction
            let mut comments = Vec::new();
            if let Some(events) = events_at.get(&i) {
                for event in events {
                    match event {
                        AllocEvent::LiveStart { vreg, preg, .. } => {
                            if let Some(p) = preg {
                                comments.push(format!("live(v{}), x{}", vreg.0, p));
                            } else {
                                comments.push(format!("live(v{})", vreg.0));
                            }
                        }
                        AllocEvent::LiveEnd { vreg, preg, .. } => {
                            if let Some(p) = preg {
                                comments.push(format!("expire(v{}), free x{}", vreg.0, p));
                            } else {
                                comments.push(format!("expire(v{})", vreg.0));
                            }
                        }
                        AllocEvent::Assign { vreg, preg, .. } => {
                            comments.push(format!("v{} = x{}", vreg.0, preg));
                        }
                        AllocEvent::Spill { vreg, slot, .. } => {
                            comments.push(format!("spill(v{}) [{}]", vreg.0, slot));
                        }
                        AllocEvent::Comment { msg, .. } => {
                            comments.push(msg.clone());
                        }
                    }
                }
            }

            let comment = if comments.is_empty() {
                String::new()
            } else {
                format!("  # {}", comments.join(", "))
            };

            let lpir_cell = match vinst.src_op() {
                Some(o) => format!("[{:>3}]", o),
                None => format!("[{:>3}]", "-"),
            };

            lines.push(format!(
                "{:<prefix_w$}{:>idx_w$}{:>idx_w$}{:<mne_w$} {}{}",
                prefix,
                format!("[{:>3}]", i),
                lpir_cell,
                vinst.mnemonic(),
                vinst.format_alloc_trace_detail(),
                comment,
                prefix_w = prefix_w,
                idx_w = IDX_W,
                mne_w = MNE_W,
            ));
        }

        lines.join("\n")
    }
}

fn collect_ret_uses(vinsts: &[VInst]) -> BTreeSet<VReg> {
    let mut s = BTreeSet::new();
    for inst in vinsts {
        if let VInst::Ret { vals, .. } = inst {
            for v in vals {
                s.insert(*v);
            }
        }
    }
    s
}

/// Forward scan: first def and last use per vreg in doubled program-point space.
/// Uses at instruction `i` → `[2*i, 2*i+1)`, defs → `[2*i+1, 2*i+2)`.
/// Two-pass: defs first (to set first_def), then uses (to extend last_use).
fn forward_scan(n: usize, vinsts: &[VInst]) -> (Vec<u32>, Vec<u32>) {
    let mut first_def = alloc::vec![u32::MAX; n];
    let mut last_use = alloc::vec![0u32; n];

    // Pass 1: defs set first_def and their own endpoint
    for (i, inst) in vinsts.iter().enumerate() {
        let i = i as u32;
        for d in inst.defs() {
            let vi = d.0 as usize;
            if vi < n {
                first_def[vi] = first_def[vi].min(2 * i + 1);
                last_use[vi] = last_use[vi].max(2 * i + 2);
            }
        }
    }

    // Pass 2: uses extend last_use. No first_def guard — parameter vregs are
    // implicitly live at entry and have no def in the vinst stream.
    for (i, inst) in vinsts.iter().enumerate() {
        let i = i as u32;
        for u in inst.uses() {
            let vi = u.0 as usize;
            if vi < n {
                last_use[vi] = last_use[vi].max(2 * i + 1);
            }
        }
    }

    (first_def, last_use)
}

/// Extend intervals for loop-carried values. A vreg is loop-carried in loop `[H, B]` if it has
/// a use at some instruction `u` and a def at some instruction `d` where `H <= u,d <= B` and
/// `u < d` (the use reads a value from the previous iteration). Such vregs must span the entire
/// loop.
///
/// Process innermost loops first (sort by region size) so that extensions propagate outward.
fn extend_for_loops(intervals: &mut [Interval], vinsts: &[VInst], loops: &[LoopRegion]) {
    if loops.is_empty() {
        return;
    }
    let mut sorted: Vec<&LoopRegion> = loops.iter().collect();
    sorted.sort_by_key(|l| l.backedge_idx - l.header_idx);

    for lr in sorted {
        let h = lr.header_idx;
        let b = lr.backedge_idx;
        let h_pp = 2 * h as u32;
        let b_pp = 2 * b as u32 + 2;

        for iv in intervals.iter_mut() {
            let vi = iv.vreg.0 as usize;
            if iv.start > b_pp || iv.end < h_pp {
                continue;
            }

            let mut first_use_in_loop = u32::MAX;
            let mut first_def_in_loop = u32::MAX;
            for i in h..=b {
                for u in vinsts[i].uses() {
                    if u.0 as usize == vi {
                        first_use_in_loop = first_use_in_loop.min(i as u32);
                    }
                }
                for d in vinsts[i].defs() {
                    if d.0 as usize == vi {
                        first_def_in_loop = first_def_in_loop.min(i as u32);
                    }
                }
            }

            let is_loop_carried = first_use_in_loop < first_def_in_loop;
            if is_loop_carried {
                iv.start = iv.start.min(h_pp);
                iv.end = iv.end.max(b_pp);
            }
        }
    }
}

/// Build live intervals for body vregs (indices >= param slots).
fn build_intervals(func: &IrFunction, vinsts: &[VInst]) -> Vec<Interval> {
    let max_vreg = vinsts
        .iter()
        .flat_map(|v| v.defs().chain(v.uses()))
        .map(|v| v.0 as usize)
        .max()
        .unwrap_or(0);
    let n = (func.vreg_types.len()).max(max_vreg + 1);

    let (first_def, last_use) = forward_scan(n, vinsts);

    let slots = func.total_param_slots() as usize;
    let mut out = Vec::new();
    for idx in slots..n {
        let start = first_def[idx];
        let end = last_use[idx];
        if start == u32::MAX || end == 0 {
            continue;
        }
        if start >= end {
            continue;
        }
        out.push(Interval {
            vreg: VReg(idx as u32),
            start,
            end,
            fixed_reg: None,
        });
    }
    out
}

/// Build live intervals for param vregs (indices 0..slots).
fn param_intervals(
    slots: usize,
    param_locs: &[ArgLoc],
    vinsts: &[VInst],
) -> Result<Vec<Interval>, NativeError> {
    let max_vreg = vinsts
        .iter()
        .flat_map(|v| v.defs().chain(v.uses()))
        .map(|v| v.0 as usize)
        .max()
        .unwrap_or(0);
    let n = slots.max(max_vreg + 1);
    let (_, last_use) = forward_scan(n, vinsts);

    let mut out = Vec::with_capacity(slots);
    for idx in 0..slots {
        let mut last = last_use[idx];
        if matches!(param_locs[idx], ArgLoc::Stack { .. }) && last == 0 {
            last = 1;
        }
        if last == 0 {
            continue;
        }
        let fixed = match param_locs[idx] {
            ArgLoc::Reg(p) => Some(abi2_int_preg_to_phys(p)?),
            ArgLoc::Stack { .. } => None,
        };
        out.push(Interval {
            vreg: VReg(idx as u32),
            start: 0,
            end: last,
            fixed_reg: fixed,
        });
    }
    Ok(out)
}

#[derive(Clone, Debug)]
struct Active {
    end: u32,
    vreg: VReg,
    preg: PhysReg,
}

pub struct LinearScan;

impl LinearScan {
    pub const fn new() -> Self {
        Self
    }

    pub fn allocate_with_func_abi(
        &self,
        func: &IrFunction,
        vinsts: &[VInst],
        abi: &FuncAbi,
        loop_regions: &[LoopRegion],
        alloc_trace: bool,
    ) -> Result<Allocation, NativeError> {
        let max_vreg = vinsts
            .iter()
            .flat_map(|v| v.defs().chain(v.uses()))
            .map(|v| v.0 as usize)
            .max()
            .unwrap_or(0);
        let n = (func.vreg_types.len()).max(max_vreg + 1);
        let slots = func.total_param_slots() as usize;
        let param_locs = abi.param_locs();

        if slots > param_locs.len() {
            return Err(NativeError::TooManyArgs(slots));
        }

        let mut vreg_to_phys: Vec<Option<PhysReg>> = alloc::vec![None; n];
        let mut spill_slots: Vec<VReg> = Vec::new();
        let mut incoming_stack_params: Vec<(VReg, i32)> = Vec::new();

        let alloca_list = sorted_allocatable_ints(abi.allocatable());

        for i in 0..slots {
            match param_locs[i] {
                ArgLoc::Reg(p) => {
                    vreg_to_phys[i] = Some(abi2_int_preg_to_phys(p)?);
                }
                ArgLoc::Stack { offset, .. } => {
                    incoming_stack_params.push((VReg(i as u32), offset));
                }
            }
        }

        let mut intervals = param_intervals(slots, param_locs, vinsts)?;
        intervals.extend(build_intervals(func, vinsts));
        extend_for_loops(&mut intervals, vinsts, loop_regions);

        let ret_uses = collect_ret_uses(vinsts);
        intervals.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then_with(|| {
                    let ar = ret_uses.contains(&a.vreg);
                    let br = ret_uses.contains(&b.vreg);
                    br.cmp(&ar)
                })
                .then_with(|| b.end.cmp(&a.end))
                .then_with(|| a.vreg.0.cmp(&b.vreg.0))
        });

        let mut active: Vec<Active> = Vec::new();
        let mut trace = AllocTrace::new();

        for iv in intervals {
            // Convert program points to instruction indices
            let start_idx = (iv.start / 2) as usize;

            // Expire old intervals
            let expired: Vec<Active> = active
                .iter()
                .filter(|a| a.end <= iv.start)
                .cloned()
                .collect();
            active.retain(|a| a.end > iv.start);

            // Trace: live end for expired (at their end instruction)
            for a in &expired {
                let exp_idx = ((a.end - 1) / 2) as usize;
                trace.push(AllocEvent::LiveEnd {
                    idx: exp_idx,
                    vreg: a.vreg,
                    preg: vreg_to_phys[a.vreg.0 as usize],
                });
            }

            // Trace: live start for new interval
            trace.push(AllocEvent::LiveStart {
                idx: start_idx,
                vreg: iv.vreg,
                preg: iv.fixed_reg.or_else(|| vreg_to_phys[iv.vreg.0 as usize]),
            });

            if let Some(preg) = iv.fixed_reg {
                vreg_to_phys[iv.vreg.0 as usize] = Some(preg);
                trace.push(AllocEvent::Assign {
                    idx: start_idx,
                    vreg: iv.vreg,
                    preg,
                });
                active.push(Active {
                    end: iv.end,
                    vreg: iv.vreg,
                    preg,
                });
                active.sort_by_key(|a| (a.end, a.vreg.0));
                continue;
            }

            let used: BTreeSet<PhysReg> = active.iter().map(|a| a.preg).collect();
            let free = alloca_list.iter().copied().find(|p| !used.contains(p));

            if let Some(preg) = free {
                vreg_to_phys[iv.vreg.0 as usize] = Some(preg);
                trace.push(AllocEvent::Assign {
                    idx: start_idx,
                    vreg: iv.vreg,
                    preg,
                });
                active.push(Active {
                    end: iv.end,
                    vreg: iv.vreg,
                    preg,
                });
                active.sort_by_key(|a| (a.end, a.vreg.0));
                continue;
            }

            // Spill path
            let mut victim_v = iv.vreg;
            let mut victim_end = iv.end;
            for a in &active {
                if a.end > victim_end {
                    victim_end = a.end;
                    victim_v = a.vreg;
                }
            }

            if victim_v == iv.vreg {
                let slot = spill_slots.len();
                spill_slots.push(iv.vreg);
                vreg_to_phys[iv.vreg.0 as usize] = None;
                trace.push(AllocEvent::Spill {
                    idx: start_idx,
                    vreg: iv.vreg,
                    slot,
                });
            } else {
                let preg = vreg_to_phys[victim_v.0 as usize]
                    .ok_or_else(|| NativeError::UnassignedVReg(victim_v.0))?;
                vreg_to_phys[victim_v.0 as usize] = None;
                spill_slots.push(victim_v);
                let victim_idx = ((victim_end - 1) / 2) as usize;
                trace.push(AllocEvent::Spill {
                    idx: victim_idx,
                    vreg: victim_v,
                    slot: spill_slots.len() - 1,
                });
                active.retain(|a| a.vreg != victim_v);
                vreg_to_phys[iv.vreg.0 as usize] = Some(preg);
                trace.push(AllocEvent::Assign {
                    idx: start_idx,
                    vreg: iv.vreg,
                    preg,
                });
                active.push(Active {
                    end: iv.end,
                    vreg: iv.vreg,
                    preg,
                });
                active.sort_by_key(|a| (a.end, a.vreg.0));
            }
        }

        // Trace: any remaining active intervals expire at end
        for a in &active {
            let end_idx = ((a.end - 1) / 2) as usize;
            trace.push(AllocEvent::LiveEnd {
                idx: end_idx,
                vreg: a.vreg,
                preg: vreg_to_phys[a.vreg.0 as usize],
            });
        }

        // Render trace when explicitly enabled (stderr; off in production paths)
        if alloc_trace {
            extern crate std;
            std::eprintln!("=== Allocation trace for {} ===\n", func.name);
            std::eprintln!("{}", trace.render(vinsts));
            std::eprintln!("\n=== Register map ===");
            for (i, p) in vreg_to_phys.iter().enumerate() {
                match p {
                    Some(r) => std::eprintln!("  v{} -> x{}", i, r),
                    None => std::eprintln!("  v{} -> [spill]", i),
                }
            }
            if !spill_slots.is_empty() {
                std::eprintln!("\n=== Spill slots ===");
                for (i, v) in spill_slots.iter().enumerate() {
                    std::eprintln!("  slot {}: v{}", i, v.0);
                }
            }
        }

        for inst in vinsts {
            for v in inst.uses() {
                let vi = v.0 as usize;
                if vi >= n {
                    return Err(NativeError::UnassignedVReg(v.0));
                }
                if vreg_to_phys[vi].is_none() && !spill_slots.contains(&v) {
                    return Err(NativeError::UnassignedVReg(v.0));
                }
            }
        }

        let mut clobbered: BTreeSet<PhysReg> = BTreeSet::new();
        for inst in vinsts {
            if inst.is_call() {
                clobbered.extend(clobber_set_from_abi(abi));
            }
        }

        Ok(Allocation {
            vreg_to_phys,
            clobbered,
            spill_slots,
            incoming_stack_params,
        })
    }
}

impl Default for LinearScan {
    fn default() -> Self {
        Self::new()
    }
}

impl RegAlloc for LinearScan {
    fn allocate(
        &self,
        func: &IrFunction,
        vinsts: &[VInst],
        arg_reg_offset: usize,
    ) -> Result<Allocation, NativeError> {
        let return_method = crate::abi::classify::ReturnMethod::Direct {
            locs: RET_REGS[..2]
                .iter()
                .map(|r| crate::abi::classify::ArgLoc::Reg(*r))
                .collect(),
        };
        let allocatable = alloca_base_int();
        let precolors: Vec<(u32, PReg)> = (0..func.total_param_slots() as usize)
            .enumerate()
            .map(|(i, vreg)| (vreg as u32, ARG_REGS[arg_reg_offset + i]))
            .collect();
        let caller_saved = caller_saved_int();
        let callee_saved = crate::isa::rv32::abi::callee_saved_int();

        let abi = FuncAbi::new_raw(
            precolors
                .iter()
                .map(|(_, p)| crate::abi::classify::ArgLoc::Reg(*p))
                .collect(),
            return_method,
            allocatable,
            precolors,
            caller_saved,
            callee_saved,
        );

        self.allocate_with_func_abi(func, vinsts, &abi, &[], false)
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use lpir::VReg;

    use super::*;
    use crate::vinst::SymbolRef;

    #[test]
    fn linear_scan_assigns_small_function_without_spills() {
        let f = IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: vec![lpir::IrType::I32; 4],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let vinsts = vec![
            VInst::IConst32 {
                dst: VReg(1),
                val: 1,
                src_op: None,
            },
            VInst::IConst32 {
                dst: VReg(2),
                val: 2,
                src_op: None,
            },
            VInst::Add32 {
                dst: VReg(3),
                src1: VReg(1),
                src2: VReg(2),
                src_op: None,
            },
        ];
        let l = LinearScan::new().allocate(&f, &vinsts, 0).expect("linear");
        assert_eq!(l.spill_count(), 0);
        for i in 1..=3 {
            assert!(l.vreg_to_phys[i].is_some(), "vreg {i} unassigned");
        }
    }

    #[test]
    fn call_clobbers_like_greedy() {
        let f = IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: vec![],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let vinsts = vec![VInst::Call {
            target: SymbolRef {
                name: String::from("g"),
            },
            args: Vec::new(),
            rets: Vec::new(),
            callee_uses_sret: false,
            src_op: None,
        }];
        let g = crate::regalloc::GreedyAlloc::new()
            .allocate(&f, &vinsts, 0)
            .expect("greedy");
        let l = LinearScan::new().allocate(&f, &vinsts, 0).expect("linear");
        assert_eq!(g.clobbered, l.clobbered);
    }
}
