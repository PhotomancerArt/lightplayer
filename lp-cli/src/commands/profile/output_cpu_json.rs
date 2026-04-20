//! Canonical `cpu-profile.json` for m3 diff (schema-versioned).

use std::path::Path;

use lp_riscv_emu::profile::{CpuCollector, PcSymbolizer};
use serde_json::{Value, json};

fn pc_key(pc: u32) -> String {
    format!("0x{pc:08x}")
}

pub fn write(cpu: &CpuCollector, symbols: &dyn PcSymbolizer, dest: &Path) -> std::io::Result<()> {
    let v = build(cpu, symbols);
    std::fs::write(dest, serde_json::to_vec_pretty(&v)?)?;
    Ok(())
}

pub fn build(cpu: &CpuCollector, symbols: &dyn PcSymbolizer) -> Value {
    let mut func_stats = serde_json::Map::new();
    for (pc, st) in &cpu.func_stats {
        func_stats.insert(
            pc_key(*pc),
            json!({
                "name": symbols.symbolize(*pc),
                "self_cycles": st.self_cycles,
                "inclusive_cycles": st.inclusive_cycles,
                "calls_in": st.calls_in,
                "calls_out": st.calls_out,
            }),
        );
    }

    let mut edges = Vec::new();
    for ((caller, callee), e) in &cpu.call_edges {
        edges.push(json!({
            "caller": pc_key(*caller),
            "caller_name": symbols.symbolize(*caller),
            "callee": pc_key(*callee),
            "callee_name": symbols.symbolize(*callee),
            "count": e.count,
            "inclusive_cycles": e.inclusive_cycles,
        }));
    }
    edges.sort_by(|a, b| {
        let ka = (
            a["caller"].as_str().unwrap_or(""),
            a["callee"].as_str().unwrap_or(""),
        );
        let kb = (
            b["caller"].as_str().unwrap_or(""),
            b["callee"].as_str().unwrap_or(""),
        );
        ka.cmp(&kb)
    });

    json!({
        "schema_version": 1,
        "cycle_model": cpu.cycle_model_label,
        "total_cycles_attributed": cpu.total_cycles_attributed,
        "func_stats": Value::Object(func_stats),
        "call_edges": edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_riscv_emu::emu::cycle_model::InstClass;
    use lp_riscv_emu::profile::{Collector, GateAction, TraceSymbol};

    use crate::commands::profile::symbolize::Symbolizer;

    #[test]
    fn schema_version_is_1() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);
        cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1);
        let sym = Symbolizer::new(&[]);
        let v = build(&cpu, &sym);
        assert_eq!(v["schema_version"], 1);
    }

    #[test]
    fn cycle_model_label_round_trips() {
        let cpu = CpuCollector::new("uniform");
        let sym = Symbolizer::new(&[]);
        let v = build(&cpu, &sym);
        assert_eq!(v["cycle_model"], "uniform");
    }

    #[test]
    fn func_stats_keys_are_lowercase_8_hex() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);
        cpu.on_instruction(0x1000, 0x2000, InstClass::JalCall, 2);
        cpu.on_instruction(0x2000, 0x2004, InstClass::Alu, 1);
        let sym = Symbolizer::new(&[]);
        let v = build(&cpu, &sym);
        let fs = v["func_stats"].as_object().unwrap();
        for k in fs.keys() {
            assert!(
                k.len() == 10
                    && k.starts_with("0x")
                    && k[2..].chars().all(|c| c.is_ascii_hexdigit()),
                "bad key: {k}"
            );
            assert_eq!(k.as_str(), k.to_ascii_lowercase());
        }
    }

    #[test]
    fn call_edges_includes_both_pcs_and_names() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);
        cpu.on_instruction(0x1000, 0x2000, InstClass::JalCall, 2);
        cpu.on_instruction(0x2000, 0x2004, InstClass::Alu, 1);
        cpu.on_instruction(0x2010, 0x1004, InstClass::JalrReturn, 3);
        let syms = vec![TraceSymbol {
            addr: 0x1000,
            size: 0x100,
            name: "f".into(),
        }];
        let sym = Symbolizer::new(&syms);
        let v = build(&cpu, &sym);
        let e0 = v["call_edges"].as_array().unwrap()[0].as_object().unwrap();
        assert!(e0.contains_key("caller"));
        assert!(e0.contains_key("caller_name"));
        assert!(e0.contains_key("callee"));
        assert!(e0.contains_key("callee_name"));
    }

    #[test]
    fn round_trips_through_serde() {
        let mut cpu = CpuCollector::new("esp32c6");
        cpu.on_gate_action(GateAction::Enable);
        cpu.on_instruction(0x1000, 0x1004, InstClass::Alu, 1);
        let sym = Symbolizer::new(&[]);
        let v = build(&cpu, &sym);
        let s = serde_json::to_string(&v).unwrap();
        let v2: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v, v2);
    }
}
