//! Speedscope "evented" profile writer for the CpuCollector.
//!
//! NOTE: events are SYNTHETIC, fabricated from aggregated call_edges
//! at finish time (R13/R15). The flame chart is shape-correct
//! (every callee bar has the right cumulative width relative to its
//! parent) but the x-axis is "synthetic cycles", not wall-clock.
//! Multiple non-contiguous calls to the same function smash together
//! into one bar. For real chronological events, use the cpu-log
//! collector (m6 future work).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use lp_riscv_emu::profile::cpu::{CallEdge, ROOT_PC};
use lp_riscv_emu::profile::{CpuCollector, PcSymbolizer};
use serde_json::{Value, json};

pub fn write(
    cpu: &CpuCollector,
    symbols: &dyn PcSymbolizer,
    workload: &str,
    mode: &str,
    dest: &Path,
) -> std::io::Result<()> {
    let v = build(cpu, symbols, workload, mode);
    std::fs::write(dest, serde_json::to_vec_pretty(&v)?)?;
    Ok(())
}

/// Bucket raw `(jal_pc, callee)` edges into `(caller_symbol_lo, callee)` for a tree walk.
fn bucket_call_edges(cpu: &CpuCollector, sym: &dyn PcSymbolizer) -> HashMap<(u32, u32), CallEdge> {
    let mut out: HashMap<(u32, u32), CallEdge> = HashMap::new();
    for ((jal, callee), e) in &cpu.call_edges {
        let parent = sym.entry_lo_for_pc(*jal);
        let agg = out.entry((parent, *callee)).or_default();
        agg.count += e.count;
        agg.inclusive_cycles += e.inclusive_cycles;
    }
    out
}

fn collect_pcs(cpu: &CpuCollector, edges: &HashMap<(u32, u32), CallEdge>) -> Vec<u32> {
    let mut pcs: HashSet<u32> = HashSet::new();
    pcs.insert(ROOT_PC);
    for k in cpu.func_stats.keys() {
        pcs.insert(*k);
    }
    for (a, b) in edges.keys() {
        pcs.insert(*a);
        pcs.insert(*b);
    }
    let mut v: Vec<u32> = pcs.into_iter().collect();
    v.sort_unstable();
    v
}

fn build_frames(pcs: &[u32], sym: &dyn PcSymbolizer) -> (Vec<Value>, HashMap<u32, usize>) {
    let mut frames = Vec::new();
    let mut pc_to_frame = HashMap::new();
    for &pc in pcs {
        let name = sym.symbolize(pc).into_owned();
        let idx = frames.len();
        frames.push(json!({ "name": name }));
        pc_to_frame.insert(pc, idx);
    }
    (frames, pc_to_frame)
}

#[derive(Debug, Clone, Copy)]
struct RawEv {
    open: bool,
    frame: usize,
    at: u64,
}

/// `stack_edges` holds the active `(caller_fn, callee)` pairs on the DFS stack. If the same pair
/// is entered again before the outer visit completes, treat it as a graph cycle (e.g. mutual
/// recursion) and emit a single flat open/close span without recursing (m2 limitation).
fn emit_events(
    caller_fn: u32,
    edges: &HashMap<(u32, u32), CallEdge>,
    pc_to_frame: &HashMap<u32, usize>,
    cursor: &mut u64,
    events: &mut Vec<RawEv>,
    stack_edges: &mut HashSet<(u32, u32)>,
) {
    let mut callees: Vec<(u32, &CallEdge)> = edges
        .iter()
        .filter_map(|((c, d), e)| (*c == caller_fn).then_some((*d, e)))
        .collect();
    callees.sort_by_key(|(d, _)| *d);

    for (callee, edge) in callees {
        let frame = pc_to_frame[&callee];

        if stack_edges.contains(&(caller_fn, callee)) {
            events.push(RawEv {
                open: true,
                frame,
                at: *cursor,
            });
            *cursor = cursor.saturating_add(edge.inclusive_cycles);
            events.push(RawEv {
                open: false,
                frame,
                at: *cursor,
            });
            continue;
        }

        stack_edges.insert((caller_fn, callee));
        events.push(RawEv {
            open: true,
            frame,
            at: *cursor,
        });
        let before = *cursor;
        emit_events(callee, edges, pc_to_frame, cursor, events, stack_edges);
        let recursed = cursor.saturating_sub(before);
        let self_time = edge.inclusive_cycles.saturating_sub(recursed);
        *cursor = cursor.saturating_add(self_time);
        events.push(RawEv {
            open: false,
            frame,
            at: *cursor,
        });
        stack_edges.remove(&(caller_fn, callee));
    }
}

pub fn build(cpu: &CpuCollector, sym: &dyn PcSymbolizer, workload: &str, mode: &str) -> Value {
    let edges = bucket_call_edges(cpu, sym);
    let callee_set: HashSet<u32> = edges.keys().map(|(_, d)| *d).collect();

    let pcs = collect_pcs(cpu, &edges);
    let (frames, pc_to_frame) = build_frames(&pcs, sym);

    let mut cursor = 0u64;
    let mut raw_events = Vec::new();
    let mut stack_edges = HashSet::new();

    let mut roots: Vec<(u32, &CallEdge)> = edges
        .iter()
        .filter_map(|((c, d), e)| {
            let attach = *c == ROOT_PC || !callee_set.contains(c);
            attach.then_some((*d, e))
        })
        .collect();
    roots.sort_by_key(|(d, _)| *d);

    for (callee, edge) in roots {
        let caller_fn = ROOT_PC;
        let frame = pc_to_frame[&callee];

        if stack_edges.contains(&(caller_fn, callee)) {
            raw_events.push(RawEv {
                open: true,
                frame,
                at: cursor,
            });
            cursor = cursor.saturating_add(edge.inclusive_cycles);
            raw_events.push(RawEv {
                open: false,
                frame,
                at: cursor,
            });
            continue;
        }

        stack_edges.insert((caller_fn, callee));
        raw_events.push(RawEv {
            open: true,
            frame,
            at: cursor,
        });
        let before = cursor;
        emit_events(
            callee,
            &edges,
            &pc_to_frame,
            &mut cursor,
            &mut raw_events,
            &mut stack_edges,
        );
        let recursed = cursor.saturating_sub(before);
        let self_time = edge.inclusive_cycles.saturating_sub(recursed);
        cursor = cursor.saturating_add(self_time);
        raw_events.push(RawEv {
            open: false,
            frame,
            at: cursor,
        });
        stack_edges.remove(&(caller_fn, callee));
    }

    let json_events: Vec<Value> = raw_events
        .iter()
        .map(|e| {
            json!({
                "type": if e.open { "O" } else { "C" },
                "frame": e.frame,
                "at": e.at,
            })
        })
        .collect();

    json!({
        "$schema": "https://www.speedscope.app/file-format-schema.json",
        "exporter": "lp-cli profile m2",
        "name": workload,
        "activeProfileIndex": 0,
        "profiles": [{
            "type": "evented",
            "name": mode,
            "unit": "none",
            "startValue": 0,
            "endValue": cpu.total_cycles_attributed,
            "events": json_events,
        }],
        "shared": {
            "frames": frames,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_riscv_emu::emu::cycle_model::InstClass;
    use lp_riscv_emu::profile::cpu::ROOT_PC;
    use lp_riscv_emu::profile::{Collector, GateAction, TraceSymbol};

    use crate::commands::profile::symbolize::Symbolizer;

    #[test]
    fn smoke_empty_cpu_produces_valid_envelope() {
        let cpu = CpuCollector::new("esp32c6");
        let syms: [TraceSymbol; 0] = [];
        let sym = Symbolizer::new(&syms);
        let v = build(&cpu, &sym, "w", "m");
        assert_eq!(
            v["$schema"],
            "https://www.speedscope.app/file-format-schema.json"
        );
        assert_eq!(v["profiles"][0]["type"], "evented");
        assert!(v["profiles"][0]["events"].as_array().unwrap().is_empty());
    }

    /// Three nested calls root -> A -> B -> C with symbols so bucketing groups jal sites per function.
    #[test]
    fn three_function_call_graph_produces_correct_event_count() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);

        // Root -> A (caller_pc 0 gives a true root edge after bucketing)
        cpu.on_instruction(ROOT_PC, 0x1000, InstClass::JalCall, 2);
        // A: 0x1000..0x1040 — call B from 0x1008
        for _ in 0..3 {
            cpu.on_instruction(0x1004, 0x1008, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x1008, 0x2000, InstClass::JalCall, 2);
        // B: 0x2000..0x2040 — call C from 0x2008
        for _ in 0..2 {
            cpu.on_instruction(0x2000, 0x2004, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x2008, 0x3000, InstClass::JalCall, 2);
        // C: 0x3000..0x3040
        for _ in 0..4 {
            cpu.on_instruction(0x3000, 0x3004, InstClass::Alu, 1);
        }
        cpu.on_instruction(0x3010, 0x200c, InstClass::JalrReturn, 3);
        cpu.on_instruction(0x200c, 0x100c, InstClass::JalrReturn, 3);
        cpu.on_instruction(0x100c, 0x4, InstClass::JalrReturn, 3);

        let symbols = vec![
            TraceSymbol {
                addr: 0x1000,
                size: 0x100,
                name: "a".into(),
            },
            TraceSymbol {
                addr: 0x2000,
                size: 0x100,
                name: "b".into(),
            },
            TraceSymbol {
                addr: 0x3000,
                size: 0x100,
                name: "c".into(),
            },
        ];
        let sym = Symbolizer::new(&symbols);
        let v = build(&cpu, &sym, "w", "m");
        let n = v["profiles"][0]["events"].as_array().unwrap().len();
        assert_eq!(n, 6, "expected 3 open + 3 close events");
    }

    #[test]
    fn json_parses_back_with_speedscope_fields() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);
        cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1);
        let sym = Symbolizer::new(&[]);
        let v = build(&cpu, &sym, "w", "m");
        assert_eq!(
            v["$schema"],
            "https://www.speedscope.app/file-format-schema.json"
        );
        assert_eq!(v["profiles"][0]["type"], "evented");
    }

    #[test]
    fn end_value_matches_total_attributed_cycles() {
        let mut cpu = CpuCollector::new("uniform");
        cpu.on_gate_action(GateAction::Enable);
        for _ in 0..7 {
            cpu.on_instruction(0x500, 0x504, InstClass::Alu, 1);
        }
        let sym = Symbolizer::new(&[]);
        let v = build(&cpu, &sym, "w", "m");
        assert_eq!(v["profiles"][0]["endValue"], cpu.total_cycles_attributed);
    }

    /// Self-recursive call creates `(entry_lo, callee)` self-edge; DFS must not recurse forever.
    #[test]
    fn self_recursive_edge_cycle_guard_finishes() {
        let symbols = vec![TraceSymbol {
            addr: 0x1000,
            size: 0x1000,
            name: "selfy".into(),
        }];
        let sym = Symbolizer::new(&symbols);

        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);
        cpu.on_instruction(ROOT_PC, 0x1000, InstClass::JalCall, 2);
        cpu.on_instruction(0x1004, 0x1008, InstClass::Alu, 1);
        cpu.on_instruction(0x1008, 0x1000, InstClass::JalCall, 2);
        cpu.on_instruction(0x1004, 0x1008, InstClass::Alu, 1);
        cpu.on_instruction(0x1010, 0x100c, InstClass::JalrReturn, 3);
        cpu.on_instruction(0x100c, 0x4, InstClass::JalrReturn, 3);

        let v = build(&cpu, &sym, "w", "m");
        let events = v["profiles"][0]["events"].as_array().unwrap();
        assert!(
            !events.is_empty(),
            "expected synthetic events for recursive graph"
        );
        // Smoke: JSON structure intact (no stack overflow / hang).
        assert_eq!(v["profiles"][0]["type"], "evented");
    }
}
