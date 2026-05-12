//! Function-focused analysis for an existing `cpu-profile.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::args::ProfileFunctionArgs;

pub fn handle_profile_function(args: ProfileFunctionArgs) -> Result<()> {
    let profile = load_cpu_profile(&args.dir)?;
    let target = find_function(&profile, &args.function, args.exact)?;
    let elf = args.elf.or_else(default_emu_elf);
    print_function_report(&profile, &target, args.top, elf.as_deref());
    Ok(())
}

#[derive(Debug, Clone)]
struct FunctionMatch {
    pc: String,
    name: String,
    self_cycles: u64,
    inclusive_cycles: u64,
    calls_in: u64,
    calls_out: u64,
}

#[derive(Debug, Clone, Default)]
struct EdgeSummary {
    count: u64,
    cycles: u64,
}

#[derive(Debug, Clone)]
struct EdgeRow {
    count: u64,
    cycles: u64,
    name: String,
    pc: String,
    callsite: Option<String>,
}

fn load_cpu_profile(dir: &Path) -> Result<Value> {
    let path = dir.join("cpu-profile.json");
    let file =
        std::fs::File::open(&path).with_context(|| format!("open profile {}", path.display()))?;
    serde_json::from_reader(file).with_context(|| format!("parse profile {}", path.display()))
}

fn find_function(profile: &Value, needle: &str, exact: bool) -> Result<FunctionMatch> {
    let stats = profile["func_stats"]
        .as_object()
        .context("cpu-profile.json missing func_stats object")?;
    let mut matches = stats
        .iter()
        .filter_map(|(pc, stat)| {
            let name = stat["name"].as_str()?;
            let is_match = if exact {
                name == needle
            } else {
                name.contains(needle)
            };
            is_match.then(|| FunctionMatch {
                pc: pc.clone(),
                name: name.to_owned(),
                self_cycles: stat["self_cycles"].as_u64().unwrap_or(0),
                inclusive_cycles: stat["inclusive_cycles"].as_u64().unwrap_or(0),
                calls_in: stat["calls_in"].as_u64().unwrap_or(0),
                calls_out: stat["calls_out"].as_u64().unwrap_or(0),
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|a, b| {
        b.self_cycles
            .cmp(&a.self_cycles)
            .then_with(|| b.inclusive_cycles.cmp(&a.inclusive_cycles))
            .then_with(|| a.name.cmp(&b.name))
    });

    match matches.as_slice() {
        [] => bail!("no function in profile matched `{needle}`"),
        [one] => Ok(one.clone()),
        many => {
            eprintln!("multiple functions matched `{needle}`; using hottest self-cycles match:");
            for item in many.iter().take(8) {
                eprintln!(
                    "  {:>10} self  {:>10} incl  {}  {}",
                    item.self_cycles, item.inclusive_cycles, item.pc, item.name
                );
            }
            Ok(many[0].clone())
        }
    }
}

fn print_function_report(profile: &Value, target: &FunctionMatch, top: usize, elf: Option<&Path>) {
    let total = profile["total_cycles_attributed"].as_u64().unwrap_or(0);
    println!("Function: {}", target.name);
    println!("PC: {}", target.pc);
    println!(
        "Self: {} cycles ({:.1}%)",
        target.self_cycles,
        pct(target.self_cycles, total)
    );
    println!(
        "Inclusive: {} cycles ({:.1}%)",
        target.inclusive_cycles,
        pct(target.inclusive_cycles, total)
    );
    println!(
        "Calls in: {}   Calls out: {}",
        target.calls_in, target.calls_out
    );
    if let Some(path) = elf {
        println!("ELF: {}", path.display());
    }
    println!();

    let (incoming, outgoing, callsites) = collect_edges(profile, target, elf);
    print_grouped("Incoming by caller", "caller", &incoming, top, total);
    print_grouped("Outgoing by callee", "callee", &outgoing, top, total);
    print_rows("Hottest incoming callsites", &callsites, top, total);
}

fn collect_edges(
    profile: &Value,
    target: &FunctionMatch,
    elf: Option<&Path>,
) -> (
    Vec<(String, EdgeSummary)>,
    Vec<(String, EdgeSummary)>,
    Vec<EdgeRow>,
) {
    let mut incoming = BTreeMap::<String, EdgeSummary>::new();
    let mut outgoing = BTreeMap::<String, EdgeSummary>::new();
    let mut callsites = Vec::new();

    if let Some(edges) = profile["call_edges"].as_array() {
        for edge in edges {
            let callee = edge["callee"].as_str().unwrap_or_default();
            let caller = edge["caller"].as_str().unwrap_or_default();
            let cycles = edge["inclusive_cycles"].as_u64().unwrap_or(0);
            let count = edge["count"].as_u64().unwrap_or(0);

            if callee == target.pc {
                let caller_name = edge["caller_name"].as_str().unwrap_or(caller).to_owned();
                add_summary(&mut incoming, caller_name.clone(), count, cycles);
                callsites.push(EdgeRow {
                    count,
                    cycles,
                    name: caller_name,
                    pc: caller.to_owned(),
                    callsite: elf.and_then(|path| addr2line(path, caller)),
                });
            }
            if caller == target.pc {
                let callee_name = edge["callee_name"].as_str().unwrap_or(callee).to_owned();
                add_summary(&mut outgoing, callee_name, count, cycles);
            }
        }
    }

    let mut incoming = incoming.into_iter().collect::<Vec<_>>();
    incoming.sort_by(|a, b| b.1.cycles.cmp(&a.1.cycles).then_with(|| a.0.cmp(&b.0)));
    let mut outgoing = outgoing.into_iter().collect::<Vec<_>>();
    outgoing.sort_by(|a, b| b.1.cycles.cmp(&a.1.cycles).then_with(|| a.0.cmp(&b.0)));
    callsites.sort_by(|a, b| b.cycles.cmp(&a.cycles).then_with(|| a.name.cmp(&b.name)));

    (incoming, outgoing, callsites)
}

fn add_summary(map: &mut BTreeMap<String, EdgeSummary>, name: String, count: u64, cycles: u64) {
    let entry = map.entry(name).or_default();
    entry.count += count;
    entry.cycles += cycles;
}

fn print_grouped(title: &str, label: &str, rows: &[(String, EdgeSummary)], top: usize, total: u64) {
    println!("{title}:");
    if rows.is_empty() {
        println!("  none");
        println!();
        return;
    }
    println!("  {:>10} {:>7} {:>7}  {}", "cycles", "%", "calls", label);
    for (name, row) in rows.iter().take(top) {
        println!(
            "  {:>10} {:>6.1}% {:>7}  {}",
            row.cycles,
            pct(row.cycles, total),
            row.count,
            name
        );
    }
    println!();
}

fn print_rows(title: &str, rows: &[EdgeRow], top: usize, total: u64) {
    println!("{title}:");
    if rows.is_empty() {
        println!("  none");
        println!();
        return;
    }
    println!("  {:>10} {:>7} {:>7}  caller", "cycles", "%", "calls");
    for row in rows.iter().take(top) {
        println!(
            "  {:>10} {:>6.1}% {:>7}  {} ({})",
            row.cycles,
            pct(row.cycles, total),
            row.count,
            row.name,
            row.pc
        );
        if let Some(site) = &row.callsite {
            println!("  {:>28}  {}", "", site);
        }
    }
    println!();
}

fn pct(value: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        value as f64 * 100.0 / total as f64
    }
}

fn default_emu_elf() -> Option<PathBuf> {
    let path = PathBuf::from("target/riscv32imac-unknown-none-elf/release-emu/fw-emu");
    path.exists().then_some(path)
}

fn addr2line(elf: &Path, pc: &str) -> Option<String> {
    let tool = std::env::var_os("ADDR2LINE")
        .map(PathBuf::from)
        .or_else(which_addr2line)?;
    let output = Command::new(tool)
        .arg("-e")
        .arg(elf)
        .arg("-Cfip")
        .arg(pc)
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .to_owned()
    })
}

fn which_addr2line() -> Option<PathBuf> {
    [
        "/opt/homebrew/opt/binutils/bin/addr2line",
        "rust-addr2line",
        "llvm-addr2line",
        "addr2line",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.components().count() > 1 && path.exists() || path.components().count() == 1)
}

#[cfg(test)]
mod tests {
    use super::{find_function, pct};
    use serde_json::json;

    #[test]
    fn pct_handles_empty_total() {
        assert_eq!(pct(10, 0), 0.0);
    }

    #[test]
    fn find_function_selects_hottest_match() {
        let profile = json!({
            "func_stats": {
                "0x00000001": {
                    "name": "memcpy_small",
                    "self_cycles": 5,
                    "inclusive_cycles": 10,
                    "calls_in": 1,
                    "calls_out": 0
                },
                "0x00000002": {
                    "name": "memcpy",
                    "self_cycles": 50,
                    "inclusive_cycles": 60,
                    "calls_in": 2,
                    "calls_out": 0
                }
            }
        });

        let found = find_function(&profile, "memcpy", false).expect("function match");
        assert_eq!(found.pc, "0x00000002");
        assert_eq!(found.self_cycles, 50);
    }

    #[test]
    fn find_function_exact_requires_full_name() {
        let profile = json!({
            "func_stats": {
                "0x00000001": {
                    "name": "memcpy_small",
                    "self_cycles": 50,
                    "inclusive_cycles": 60,
                    "calls_in": 1,
                    "calls_out": 0
                }
            }
        });

        assert!(find_function(&profile, "memcpy", true).is_err());
        assert!(find_function(&profile, "memcpy_small", true).is_ok());
    }
}
