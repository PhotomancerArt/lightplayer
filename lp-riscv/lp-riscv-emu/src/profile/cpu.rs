//! Call-graph style CPU cycle attribution (callgrind-like semantics).

use ::alloc::format;
use ::alloc::string::String;
use ::alloc::vec::Vec;
use std::any::Any;
use std::collections::HashMap;

use crate::emu::cycle_model::InstClass;

use super::{Collector, FinishCtx, GateAction, PcSymbolizer};

const RAM_START: u32 = 0x8000_0000;

/// Synthetic program counter for the logical root (before any call).
pub const ROOT_PC: u32 = 0;

/// One activation record on the shadow stack.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Address of the call instruction (`JAL` / `JALR`) that entered this frame.
    ///
    /// This is the PC of the branch instruction itself (not the containing function's
    /// entry). Symbolizers resolve it via interval lookup, matching callgrind.
    pub caller_pc: u32,
    /// Entry address for this frame (callee), i.e. `target_pc` of the call.
    pub callee_pc: u32,
    /// [`CpuCollector::total_cycles_attributed`] immediately **before** the instruction
    /// that pushed this frame (call / tail) added its cost. Inclusive time for the callee
    /// therefore includes that instruction's cycles (see [`CpuCollector::on_instruction_inner`]).
    pub cycles_at_entry: u64,
    /// Guest SP immediately after the call/tail instruction entered this frame.
    pub entry_sp: u32,
    /// Lowest guest SP observed while executing this frame itself.
    pub min_sp: u32,
}

#[derive(Default, Clone, Debug)]
pub struct FuncStats {
    pub self_cycles: u64,
    pub inclusive_cycles: u64,
    pub calls_in: u64,
    pub calls_out: u64,
}

#[derive(Default, Clone, Debug)]
pub struct CallEdge {
    pub count: u64,
    pub inclusive_cycles: u64,
}

#[derive(Clone, Debug)]
pub struct StackSample {
    pub used_bytes: u32,
    pub pc: u32,
    pub function_pc: u32,
    pub sp: u32,
    pub stack_top: u32,
    pub callstack: Vec<StackFrameSample>,
}

#[derive(Clone, Debug)]
pub struct StackFrameSample {
    pub function_pc: u32,
    pub entry_sp: u32,
    pub min_sp: u32,
    pub self_bytes: u32,
    pub cumulative_bytes: u32,
}

/// Aggregates per-function and per-edge cycle stats while a profile gate is active.
pub struct CpuCollector {
    shadow_stack: Vec<Frame>,
    pub func_stats: HashMap<u32, FuncStats>,
    pub call_edges: HashMap<(u32, u32), CallEdge>,
    pub max_stack: Option<StackSample>,
    pub max_stack_by_func: HashMap<u32, StackSample>,
    pub max_frame_by_func: HashMap<u32, StackFrameSample>,
    active: bool,
    pub total_cycles_attributed: u64,
    pub cycle_model_label: &'static str,
    profiled_instructions: u64,
}

impl CpuCollector {
    pub fn new(cycle_model_label: &'static str) -> Self {
        Self {
            shadow_stack: Vec::with_capacity(64),
            func_stats: HashMap::new(),
            call_edges: HashMap::new(),
            max_stack: None,
            max_stack_by_func: HashMap::new(),
            max_frame_by_func: HashMap::new(),
            active: false,
            total_cycles_attributed: 0,
            cycle_model_label,
            profiled_instructions: 0,
        }
    }

    fn current_pc(&self) -> u32 {
        self.shadow_stack
            .last()
            .map(|f| f.callee_pc)
            .unwrap_or(ROOT_PC)
    }

    fn push_frame(&mut self, caller_pc: u32, callee_pc: u32, cycles_at_entry: u64, sp: u32) {
        self.shadow_stack.push(Frame {
            caller_pc,
            callee_pc,
            cycles_at_entry,
            entry_sp: sp,
            min_sp: sp,
        });
        self.func_stats.entry(callee_pc).or_default().calls_in += 1;
        self.func_stats.entry(caller_pc).or_default().calls_out += 1;
    }

    fn pop_frame(&mut self) {
        let Some(top) = self.shadow_stack.pop() else {
            return;
        };
        let inclusive = self
            .total_cycles_attributed
            .saturating_sub(top.cycles_at_entry);
        let stats = self.func_stats.entry(top.callee_pc).or_default();
        stats.inclusive_cycles += inclusive;
        let edge = self
            .call_edges
            .entry((top.caller_pc, top.callee_pc))
            .or_default();
        edge.count += 1;
        edge.inclusive_cycles += inclusive;
    }

    fn update_current_frame_sp(&mut self, sp: u32, stack_top: u32) {
        if let Some(frame) = self.shadow_stack.last_mut() {
            if frame.min_sp == 0 || sp < frame.min_sp {
                frame.min_sp = sp;
            }
            let min_sp = if frame.min_sp == 0 {
                frame.entry_sp
            } else {
                frame.min_sp
            };
            let sample = StackFrameSample {
                function_pc: frame.callee_pc,
                entry_sp: frame.entry_sp,
                min_sp,
                self_bytes: frame.entry_sp.saturating_sub(min_sp),
                cumulative_bytes: stack_top.saturating_sub(min_sp),
            };
            let max_frame = self
                .max_frame_by_func
                .entry(frame.callee_pc)
                .or_insert_with(|| sample.clone());
            if sample.self_bytes > max_frame.self_bytes {
                *max_frame = sample;
            }
        }
    }

    fn stack_frame_samples(&self, stack_top: u32) -> Vec<StackFrameSample> {
        self.shadow_stack
            .iter()
            .map(|frame| {
                let min_sp = if frame.min_sp == 0 {
                    frame.entry_sp
                } else {
                    frame.min_sp
                };
                StackFrameSample {
                    function_pc: frame.callee_pc,
                    entry_sp: frame.entry_sp,
                    min_sp,
                    self_bytes: frame.entry_sp.saturating_sub(min_sp),
                    cumulative_bytes: stack_top.saturating_sub(min_sp),
                }
            })
            .collect()
    }

    fn record_stack_sample(&mut self, pc: u32, sp: u32, stack_top: u32) {
        if !self.active || sp < RAM_START || sp > stack_top {
            return;
        }

        self.update_current_frame_sp(sp, stack_top);
        let used_bytes = stack_top.saturating_sub(sp);
        let function_pc = self.current_pc();
        let sample = StackSample {
            used_bytes,
            pc,
            function_pc,
            sp,
            stack_top,
            callstack: self.stack_frame_samples(stack_top),
        };

        if self
            .max_stack
            .as_ref()
            .is_none_or(|max| sample.used_bytes > max.used_bytes)
        {
            self.max_stack = Some(sample.clone());
        }

        let func_sample = self
            .max_stack_by_func
            .entry(function_pc)
            .or_insert_with(|| sample.clone());
        if sample.used_bytes > func_sample.used_bytes {
            *func_sample = sample;
        }
    }

    /// Per-instruction accounting (callgrind-style):
    ///
    /// Every instruction credits `self_cycles` for [`CpuCollector::current_pc`] — the
    /// function on top of the shadow stack **before** any push/pop for this instruction.
    /// Each instruction is attributed to exactly one function: the one whose code it
    /// executes in. Call instructions run in the caller; returns run in the callee.
    ///
    /// `total_cycles_attributed` is incremented by this instruction's cost before stack
    /// mutation. For [`InstClass::JalCall`], [`InstClass::JalrCall`], [`InstClass::JalTail`],
    /// and [`InstClass::JalrIndirect`], the new frame's `cycles_at_entry` is a snapshot of
    /// `total_cycles_attributed` **before** that instruction's cost is added, so
    /// `pop_frame`'s inclusive interval includes the entering instruction's cycles in the
    /// callee's `inclusive_cycles`.
    fn on_instruction_inner(
        &mut self,
        pc: u32,
        target_pc: u32,
        class: InstClass,
        cycles: u32,
        sp: u32,
    ) {
        if !self.active {
            return;
        }

        self.profiled_instructions += 1;
        let cycles = cycles as u64;

        match class {
            InstClass::JalCall | InstClass::JalrCall => {
                let cycles_at_entry_for_callee = self.total_cycles_attributed;
                self.total_cycles_attributed += cycles;
                let stat_pc = self.current_pc();
                self.func_stats.entry(stat_pc).or_default().self_cycles += cycles;
                self.push_frame(pc, target_pc, cycles_at_entry_for_callee, sp);
            }
            InstClass::JalrReturn => {
                self.total_cycles_attributed += cycles;
                let stat_pc = self.current_pc();
                self.func_stats.entry(stat_pc).or_default().self_cycles += cycles;
                self.pop_frame();
            }
            InstClass::JalTail | InstClass::JalrIndirect => {
                let cycles_at_entry_for_callee = self.total_cycles_attributed;
                self.total_cycles_attributed += cycles;
                let stat_pc = self.current_pc();
                self.func_stats.entry(stat_pc).or_default().self_cycles += cycles;
                self.pop_frame();
                self.push_frame(pc, target_pc, cycles_at_entry_for_callee, sp);
            }
            _ => {
                self.total_cycles_attributed += cycles;
                let stat_pc = self.current_pc();
                self.func_stats.entry(stat_pc).or_default().self_cycles += cycles;
            }
        }
    }
}

impl Collector for CpuCollector {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn name(&self) -> &'static str {
        "cpu"
    }

    fn report_title(&self) -> &'static str {
        "CPU summary"
    }

    fn meta_json(&self) -> serde_json::Value {
        serde_json::json!({
            "cycle_model": self.cycle_model_label,
        })
    }

    fn on_gate_action(&mut self, action: GateAction) {
        match action {
            GateAction::Enable => self.active = true,
            GateAction::Disable => self.active = false,
            _ => {}
        }
    }

    fn on_instruction(&mut self, pc: u32, target_pc: u32, class: InstClass, cycles: u32) {
        self.on_instruction_inner(pc, target_pc, class, cycles, 0);
    }

    fn on_instruction_with_state(
        &mut self,
        pc: u32,
        target_pc: u32,
        class: InstClass,
        cycles: u32,
        sp: u32,
        stack_top: u32,
    ) {
        self.record_stack_sample(pc, sp, stack_top);
        self.on_instruction_inner(pc, target_pc, class, cycles, sp);
    }

    fn finish(&mut self, _ctx: &FinishCtx<'_>) -> std::io::Result<()> {
        Ok(())
    }

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        self.write_cpu_summary_text(w, None)
    }

    fn report_section_symbolized(
        &self,
        w: &mut dyn std::fmt::Write,
        sym: Option<&dyn PcSymbolizer>,
    ) -> std::fmt::Result {
        self.write_cpu_summary_text(w, sym)
    }

    fn event_count(&self) -> u64 {
        self.profiled_instructions
    }
}

fn percent_of_total(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        100.0 * (part as f64) / (total as f64)
    }
}

fn format_pc_for_report(pc: u32, sym: Option<&dyn PcSymbolizer>) -> String {
    match sym {
        Some(s) => s.symbolize(pc).into_owned(),
        None => format!("0x{pc:08x}"),
    }
}

fn format_stack_sample(sample: &StackSample, sym: Option<&dyn PcSymbolizer>) -> String {
    format!(
        "{} bytes at {} in {} (sp=0x{:08x}, stack_top=0x{:08x})",
        sample.used_bytes,
        format_pc_for_report(sample.pc, sym),
        format_pc_for_report(sample.function_pc, sym),
        sample.sp,
        sample.stack_top,
    )
}

/// Sum per-PC stats into buckets keyed by containing symbol start (`entry_lo_for_pc`).
///
/// Summed `inclusive_cycles` can over-count real functions that were split into multiple
/// shadow-stack "entries" inside one symbol: each fragment carried a full inclusive slice.
fn collapse_func_stats_by_symbol(
    func_stats: &HashMap<u32, FuncStats>,
    sym: &dyn PcSymbolizer,
) -> HashMap<u32, (u64, u64)> {
    let mut out: HashMap<u32, (u64, u64)> = HashMap::new();
    for (&pc, stats) in func_stats {
        let canon = sym.entry_lo_for_pc(pc);
        let e = out.entry(canon).or_insert((0, 0));
        e.0 += stats.self_cycles;
        e.1 += stats.inclusive_cycles;
    }
    out
}

fn stack_frame_rows_by_symbol(
    frames: &HashMap<u32, StackFrameSample>,
    sym: Option<&dyn PcSymbolizer>,
) -> Vec<StackFrameSample> {
    let mut rows: Vec<StackFrameSample> = frames.values().cloned().collect();
    if let Some(s) = sym {
        let mut collapsed: HashMap<u32, StackFrameSample> = HashMap::new();
        for frame in rows {
            let canon = s.entry_lo_for_pc(frame.function_pc);
            let entry = collapsed.entry(canon).or_insert_with(|| {
                let mut frame = frame.clone();
                frame.function_pc = canon;
                frame
            });
            if frame.self_bytes > entry.self_bytes {
                let mut frame = frame;
                frame.function_pc = canon;
                *entry = frame;
            }
        }
        rows = collapsed.into_values().collect();
    }
    rows.sort_by_key(|frame| {
        (
            std::cmp::Reverse(frame.self_bytes),
            std::cmp::Reverse(frame.cumulative_bytes),
            frame.function_pc,
        )
    });
    rows
}

fn call_count_rows_by_symbol(
    func_stats: &HashMap<u32, FuncStats>,
    frames: &HashMap<u32, StackFrameSample>,
    sym: Option<&dyn PcSymbolizer>,
) -> Vec<(u32, u64, u32)> {
    let mut rows: Vec<(u32, u64, u32)> = Vec::new();
    if let Some(s) = sym {
        let mut collapsed: HashMap<u32, (u64, u32)> = HashMap::new();
        for (&pc, stats) in func_stats {
            let canon = s.entry_lo_for_pc(pc);
            collapsed.entry(canon).or_insert((0, 0)).0 += stats.calls_in;
        }
        for (&pc, frame) in frames {
            let canon = s.entry_lo_for_pc(pc);
            let entry = collapsed.entry(canon).or_insert((0, 0));
            entry.1 = entry.1.max(frame.self_bytes);
        }
        rows.extend(
            collapsed
                .into_iter()
                .map(|(pc, (calls, frame_bytes))| (pc, calls, frame_bytes)),
        );
    } else {
        rows.extend(func_stats.iter().map(|(&pc, stats)| {
            let frame_bytes = frames.get(&pc).map_or(0, |frame| frame.self_bytes);
            (pc, stats.calls_in, frame_bytes)
        }));
    }
    rows.sort_by_key(|(pc, calls, frame_bytes)| {
        (
            std::cmp::Reverse(*calls),
            std::cmp::Reverse(*frame_bytes),
            *pc,
        )
    });
    rows
}

impl CpuCollector {
    fn write_cpu_summary_text(
        &self,
        w: &mut dyn std::fmt::Write,
        sym: Option<&dyn PcSymbolizer>,
    ) -> std::fmt::Result {
        writeln!(w, "cycle_model={}", self.cycle_model_label)?;
        writeln!(
            w,
            "total_attributed_cycles={}",
            self.total_cycles_attributed
        )?;
        writeln!(w, "profiled_instructions={}", self.profiled_instructions)?;
        writeln!(w)?;

        writeln!(w, "Stack high water:")?;
        match &self.max_stack {
            Some(sample) => {
                writeln!(w, "  {}", format_stack_sample(sample, sym))?;
                if !sample.callstack.is_empty() {
                    writeln!(w, "  Stack frames at high water (leaf first):")?;
                    writeln!(w, "     frame     total  function")?;
                    for frame in sample.callstack.iter().rev() {
                        writeln!(
                            w,
                            "    {:>6}  {:>8}  {}",
                            frame.self_bytes,
                            frame.cumulative_bytes,
                            format_pc_for_report(frame.function_pc, sym),
                        )?;
                    }
                }
            }
            None => writeln!(w, "  no stack samples")?,
        }
        writeln!(w)?;

        writeln!(w, "Top 20 largest observed frames:")?;
        writeln!(w, "     frame     total  function")?;
        for frame in stack_frame_rows_by_symbol(&self.max_frame_by_func, sym)
            .into_iter()
            .take(20)
        {
            writeln!(
                w,
                "  {:>8}  {:>8}  {}",
                frame.self_bytes,
                frame.cumulative_bytes,
                format_pc_for_report(frame.function_pc, sym),
            )?;
        }
        writeln!(w)?;

        writeln!(w, "Top 20 most-called frames:")?;
        writeln!(w, "     calls     frame  function")?;
        for (pc, calls, frame_bytes) in
            call_count_rows_by_symbol(&self.func_stats, &self.max_frame_by_func, sym)
                .into_iter()
                .filter(|(_, calls, _)| *calls > 0)
                .take(20)
        {
            writeln!(
                w,
                "  {:>8}  {:>8}  {}",
                calls,
                frame_bytes,
                format_pc_for_report(pc, sym),
            )?;
        }
        writeln!(w)?;

        writeln!(w, "Top 20 by max observed stack:")?;
        let mut stack_rows: Vec<StackSample> = self.max_stack_by_func.values().cloned().collect();
        stack_rows.sort_by_key(|sample| std::cmp::Reverse(sample.used_bytes));
        if let Some(s) = sym {
            let mut collapsed: HashMap<u32, StackSample> = HashMap::new();
            for sample in stack_rows {
                let canon = s.entry_lo_for_pc(sample.function_pc);
                let entry = collapsed.entry(canon).or_insert_with(|| {
                    let mut sample = sample.clone();
                    sample.function_pc = canon;
                    sample
                });
                if sample.used_bytes > entry.used_bytes {
                    let mut sample = sample;
                    sample.function_pc = canon;
                    *entry = sample;
                }
            }
            stack_rows = collapsed.into_values().collect();
            stack_rows.sort_by_key(|sample| std::cmp::Reverse(sample.used_bytes));
        }
        for sample in stack_rows.into_iter().take(20) {
            writeln!(
                w,
                "  {:>8}  {}",
                sample.used_bytes,
                format_pc_for_report(sample.function_pc, sym),
            )?;
        }
        writeln!(w)?;

        let aggregated: Option<HashMap<u32, (u64, u64)>> =
            sym.map(|s| collapse_func_stats_by_symbol(&self.func_stats, s));

        writeln!(w, "Top 20 by self cycles:")?;
        if let Some(agg) = &aggregated {
            let mut rows: Vec<(u32, u64, u64)> = agg
                .iter()
                .map(|(&canon, &(self_c, incl_c))| (canon, self_c, incl_c))
                .collect();
            rows.sort_by_key(|(_, self_c, _)| std::cmp::Reverse(*self_c));
            for (canon, self_c, _) in rows.into_iter().take(20) {
                writeln!(
                    w,
                    "  {:>12}  {:>5.1}%  {}",
                    self_c,
                    percent_of_total(self_c, self.total_cycles_attributed),
                    format_pc_for_report(canon, sym),
                )?;
            }
        } else {
            let mut by_self: Vec<_> = self.func_stats.iter().collect();
            by_self.sort_by_key(|(_, s)| std::cmp::Reverse(s.self_cycles));
            for (pc, stats) in by_self.iter().take(20) {
                writeln!(
                    w,
                    "  {:>12}  {:>5.1}%  {}",
                    stats.self_cycles,
                    percent_of_total(stats.self_cycles, self.total_cycles_attributed),
                    format_pc_for_report(**pc, sym),
                )?;
            }
        }
        writeln!(w)?;

        writeln!(w, "Top 20 by inclusive cycles:")?;
        if let Some(agg) = &aggregated {
            let mut rows: Vec<(u32, u64, u64)> = agg
                .iter()
                .map(|(&canon, &(self_c, incl_c))| (canon, self_c, incl_c))
                .collect();
            rows.sort_by_key(|(_, _, incl_c)| std::cmp::Reverse(*incl_c));
            for (canon, _, incl_c) in rows.into_iter().take(20) {
                writeln!(
                    w,
                    "  {:>12}  {:>5.1}%  {}",
                    incl_c,
                    percent_of_total(incl_c, self.total_cycles_attributed),
                    format_pc_for_report(canon, sym),
                )?;
            }
        } else {
            let mut by_incl: Vec<_> = self.func_stats.iter().collect();
            by_incl.sort_by_key(|(_, s)| std::cmp::Reverse(s.inclusive_cycles));
            for (pc, stats) in by_incl.iter().take(20) {
                writeln!(
                    w,
                    "  {:>12}  {:>5.1}%  {}",
                    stats.inclusive_cycles,
                    percent_of_total(stats.inclusive_cycles, self.total_cycles_attributed),
                    format_pc_for_report(**pc, sym),
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::GateAction;
    use super::super::PcSymbolizer;
    use super::*;
    use std::borrow::Cow;

    fn extract_top_self_block(report: &str) -> &str {
        let start = report.find("Top 20 by self cycles:").expect("self header")
            + "Top 20 by self cycles:".len();
        let rest = &report[start..];
        let end = rest
            .find("Top 20 by inclusive cycles:")
            .expect("inclusive header");
        &rest[..end]
    }

    struct IntervalSym {
        lo: u32,
        hi: u32,
        name: &'static str,
    }

    impl PcSymbolizer for IntervalSym {
        fn symbolize(&self, pc: u32) -> Cow<'_, str> {
            if (self.lo..self.hi).contains(&pc) {
                Cow::Borrowed(self.name)
            } else {
                Cow::Owned(format!("0x{pc:08x}"))
            }
        }

        fn entry_lo_for_pc(&self, pc: u32) -> u32 {
            if (self.lo..self.hi).contains(&pc) {
                self.lo
            } else {
                pc
            }
        }
    }

    #[test]
    fn gate_disabled_no_attribution() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1);
        cpu.on_instruction(0x1004, 0x1008, InstClass::Alu, 1);
        assert_eq!(cpu.total_cycles_attributed, 0);
        assert!(cpu.func_stats.is_empty());
    }

    #[test]
    fn simple_call_return() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);

        for _ in 0..5 {
            cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x1010, 0x1014, InstClass::JalCall, 2);
        for _ in 0..10 {
            cpu.on_instruction(0x1014, 0x1018, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x1024, 0x1010, InstClass::JalrReturn, 3);

        assert_eq!(cpu.total_cycles_attributed, 5 + 2 + 10 + 3);

        assert_eq!(cpu.func_stats[&0].self_cycles, 5 + 2);
        assert_eq!(cpu.func_stats[&0x1014].self_cycles, 10 + 3);
        assert_eq!(cpu.func_stats[&0x1014].calls_in, 1);
        assert_eq!(cpu.func_stats[&0x1014].inclusive_cycles, 2 + 10 + 3);
        assert_eq!(cpu.call_edges[&(0x1010, 0x1014)].count, 1);
    }

    #[test]
    fn nested_three_deep() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);

        cpu.on_instruction(0x1000, 0x2000, InstClass::JalCall, 2);
        for _ in 0..3 {
            cpu.on_instruction(0x2000, 0x2004, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x2010, 0x3000, InstClass::JalCall, 2);
        for _ in 0..5 {
            cpu.on_instruction(0x3000, 0x3004, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x3010, 0x4000, InstClass::JalCall, 2);
        for _ in 0..7 {
            cpu.on_instruction(0x4000, 0x4004, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x4010, 0x3014, InstClass::JalrReturn, 3);
        cpu.on_instruction(0x3014, 0x2014, InstClass::JalrReturn, 3);
        cpu.on_instruction(0x2014, 0x1004, InstClass::JalrReturn, 3);

        assert_eq!(cpu.func_stats[&0].self_cycles, 2);
        assert_eq!(cpu.func_stats[&0x2000].self_cycles, 3 + 2 + 3);
        assert_eq!(cpu.func_stats[&0x3000].self_cycles, 5 + 2 + 3);
        assert_eq!(cpu.func_stats[&0x4000].self_cycles, 7 + 3);

        assert_eq!(cpu.func_stats[&0x4000].inclusive_cycles, 12);
        assert_eq!(cpu.func_stats[&0x3000].inclusive_cycles, 22);
        assert_eq!(cpu.func_stats[&0x2000].inclusive_cycles, 30);
    }

    #[test]
    fn tail_call_swaps_top() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);

        cpu.on_instruction(0x1000, 0x2000, InstClass::JalCall, 2);
        for _ in 0..3 {
            cpu.on_instruction(0x2000, 0x2004, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x2010, 0x3000, InstClass::JalTail, 2);
        for _ in 0..5 {
            cpu.on_instruction(0x3000, 0x3004, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x3010, 0x4000, InstClass::JalTail, 2);
        for _ in 0..7 {
            cpu.on_instruction(0x4000, 0x4004, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x4010, 0x1004, InstClass::JalrReturn, 3);

        assert!(cpu.func_stats.contains_key(&0x2000));
        assert!(cpu.func_stats.contains_key(&0x3000));
        assert!(cpu.func_stats.contains_key(&0x4000));

        assert_eq!(cpu.func_stats[&0x2000].inclusive_cycles, 7);
        assert_eq!(cpu.func_stats[&0x3000].inclusive_cycles, 9);
        assert_eq!(cpu.func_stats[&0x4000].inclusive_cycles, 12);
    }

    #[test]
    fn orphaned_return_at_root() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);

        cpu.on_instruction(0x1000, 0x0, InstClass::JalrReturn, 3);
        assert_eq!(cpu.total_cycles_attributed, 3);
        assert_eq!(cpu.func_stats[&0].self_cycles, 3);
    }

    #[test]
    fn root_self_cycles() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);

        for _ in 0..100 {
            cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1);
        }
        assert_eq!(cpu.func_stats[&0].self_cycles, 100);
    }

    #[test]
    fn enable_disable_toggle() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);
        for _ in 0..10 {
            cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1);
        }
        cpu.on_gate_action(GateAction::Disable);
        for _ in 0..50 {
            cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1);
        }
        cpu.on_gate_action(GateAction::Enable);
        for _ in 0..20 {
            cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1);
        }

        assert_eq!(cpu.total_cycles_attributed, 10 + 20);
    }

    #[test]
    fn call_edge_aggregation() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);

        for _ in 0..3 {
            cpu.on_instruction(0x1000, 0x2000, InstClass::JalCall, 2);
            for _ in 0..5 {
                cpu.on_instruction(0x2000, 0x2004, InstClass::Alu, 1);
            }
            cpu.on_instruction(0x2010, 0x1004, InstClass::JalrReturn, 3);
        }

        assert_eq!(cpu.call_edges[&(0x1000, 0x2000)].count, 3);
        assert_eq!(cpu.func_stats[&0x2000].calls_in, 3);
    }

    #[test]
    fn records_stack_high_water_by_active_function() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);

        cpu.on_instruction_with_state(
            0x1000,
            0x2000,
            InstClass::JalCall,
            2,
            0x8000_1ff0,
            0x8000_2000,
        );
        cpu.on_instruction_with_state(0x2000, 0x2004, InstClass::Alu, 1, 0x8000_1f00, 0x8000_2000);

        let max = cpu.max_stack.as_ref().expect("max stack sample");
        assert_eq!(max.used_bytes, 0x100);
        assert_eq!(max.function_pc, 0x2000);
        assert_eq!(cpu.max_stack_by_func[&0x2000].used_bytes, 0x100);
        assert_eq!(cpu.max_frame_by_func[&0x2000].self_bytes, 0xf0);
    }

    #[test]
    fn top_self_collapses_intra_function_pcs() {
        const SYM: &str = "ZZZ_report_sym_collapsed";
        let sym = IntervalSym {
            lo: 0x5000,
            hi: 0x6000,
            name: SYM,
        };

        let mut cpu = CpuCollector::new("esp32c6");
        cpu.func_stats.insert(
            0x5004,
            FuncStats {
                self_cycles: 10,
                inclusive_cycles: 0,
                ..Default::default()
            },
        );
        cpu.func_stats.insert(
            0x5100,
            FuncStats {
                self_cycles: 20,
                inclusive_cycles: 0,
                ..Default::default()
            },
        );
        cpu.total_cycles_attributed = 30;

        let mut out = String::new();
        cpu.write_cpu_summary_text(&mut out, Some(&sym as &dyn PcSymbolizer))
            .unwrap();

        let self_block = extract_top_self_block(&out);
        assert_eq!(
            self_block.matches(SYM).count(),
            1,
            "expected a single collapsed row: {self_block}"
        );
        let line = self_block
            .lines()
            .find(|l| l.contains(SYM))
            .expect("row with symbol name");
        let cycles = line.split_whitespace().next().expect("self cycles column");
        assert_eq!(cycles, "30");
    }

    #[test]
    fn top_self_keeps_pcs_distinct_when_no_symbolizer() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.func_stats.insert(
            0x5004,
            FuncStats {
                self_cycles: 10,
                inclusive_cycles: 0,
                ..Default::default()
            },
        );
        cpu.func_stats.insert(
            0x5100,
            FuncStats {
                self_cycles: 20,
                inclusive_cycles: 0,
                ..Default::default()
            },
        );
        cpu.total_cycles_attributed = 30;

        let mut out = String::new();
        cpu.write_cpu_summary_text(&mut out, None).unwrap();

        let self_block = extract_top_self_block(&out);
        assert!(self_block.contains("0x00005004"));
        assert!(self_block.contains("0x00005100"));
    }
}
