//! Call-graph style CPU cycle attribution (callgrind-like semantics).

use ::alloc::format;
use ::alloc::string::String;
use ::alloc::vec::Vec;
use std::any::Any;
use std::collections::HashMap;

use crate::emu::cycle_model::InstClass;

use super::{Collector, FinishCtx, GateAction, PcSymbolizer};

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

/// Aggregates per-function and per-edge cycle stats while a profile gate is active.
pub struct CpuCollector {
    shadow_stack: Vec<Frame>,
    pub func_stats: HashMap<u32, FuncStats>,
    pub call_edges: HashMap<(u32, u32), CallEdge>,
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

    fn push_frame(&mut self, caller_pc: u32, callee_pc: u32, cycles_at_entry: u64) {
        self.shadow_stack.push(Frame {
            caller_pc,
            callee_pc,
            cycles_at_entry,
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
    fn on_instruction_inner(&mut self, pc: u32, target_pc: u32, class: InstClass, cycles: u32) {
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
                self.push_frame(pc, target_pc, cycles_at_entry_for_callee);
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
                self.push_frame(pc, target_pc, cycles_at_entry_for_callee);
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
        self.on_instruction_inner(pc, target_pc, class, cycles);
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

        writeln!(w, "Top 20 by self cycles:")?;
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
        writeln!(w)?;

        writeln!(w, "Top 20 by inclusive cycles:")?;
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::GateAction;
    use super::*;

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
}
