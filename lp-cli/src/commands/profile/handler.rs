//! `lp-cli profile` — run a workload under the emulator with unified profiling.

use anyhow::{Context, Result, bail};
use lp_client::LpClient;
use lp_client::transport_emu_serial::SerialEmuClientTransport;
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    LogLevel, Riscv32Emulator, TimeMode,
    profile::alloc::AllocCollector,
    profile::cpu::CpuCollector,
    profile::events::EventsCollector,
    profile::{Collector, HaltReason, PcSymbolizer, TraceSymbol},
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;
use std::collections::HashSet;
use std::path::Component;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::args::ProfileArgs;
use super::output;
use super::output_cpu_json;
use super::output_speedscope;
use super::symbolize::Symbolizer;
use super::workload;

pub fn handle_profile(args: ProfileArgs) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(handle_profile_async(args))
}

async fn handle_profile_async(args: ProfileArgs) -> Result<()> {
    validate_collectors(&args.collect)?;

    let dir = std::env::current_dir()
        .context("Failed to get current directory")?
        .join(&args.dir)
        .canonicalize()
        .with_context(|| {
            format!(
                "Failed to resolve project directory: {}",
                args.dir.display()
            )
        })?;

    let project_uid = read_project_uid(&dir)?;

    eprintln!("Building fw-emu (feature profile)...");
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true)
            .with_features(&["profile"]),
    )
    .map_err(|e| anyhow::anyhow!("Failed to build fw-emu: {e}"))?;

    let elf_data = std::fs::read(&fw_emu_path).context("Failed to read fw-emu ELF")?;
    let load_info = load_elf(&elf_data).map_err(|e| anyhow::anyhow!("Failed to load ELF: {e}"))?;

    let timestamp_dir = chrono::Local::now().format("%Y-%m-%dT%H-%M-%S");
    let dir_label = derive_dir_label(&args.dir, &std::env::current_dir().unwrap_or_default());
    let mode_slug = args.mode.slug();
    let mut profile_dir_name = format!("{timestamp_dir}--{dir_label}--{mode_slug}");
    if let Some(note) = &args.note {
        let note_kebab = kebab_case(note);
        if !note_kebab.is_empty() {
            profile_dir_name = format!("{profile_dir_name}--{note_kebab}");
        }
    }
    let trace_dir = std::path::PathBuf::from("profiles").join(&profile_dir_name);

    std::fs::create_dir_all(&trace_dir).with_context(|| {
        format!(
            "Failed to create profile output directory {}",
            trace_dir.display()
        )
    })?;

    let heap_start = 0x8000_0000u32;
    let heap_size = 320 * 1024;

    let trace_symbols: Vec<TraceSymbol> = load_info
        .symbol_list
        .iter()
        .map(|s| TraceSymbol {
            addr: s.addr,
            size: s.size,
            name: s.name.clone(),
        })
        .collect();

    let metadata = output::build_initial_metadata(
        project_uid.clone(),
        args.dir.display().to_string(),
        args.note.clone(),
        trace_symbols.clone(),
        args.mode,
        args.max_cycles,
        args.cycle_model.label().to_string(),
    );

    let mut requested: Vec<String> = args.collect.iter().map(|s| s.trim().to_string()).collect();
    let wants_cpu = requested.iter().any(|c| c == "cpu");
    if wants_cpu && !requested.iter().any(|c| c == "events") {
        requested.push("events".to_string());
    }

    let mut collectors: Vec<Box<dyn Collector>> = Vec::new();
    for name in &requested {
        let name = name.trim();
        match name {
            "alloc" => collectors.push(Box::new(AllocCollector::new(
                &trace_dir, heap_start, heap_size,
            )?)),
            "events" => collectors.push(Box::new(EventsCollector::new(&trace_dir)?)),
            "cpu" => collectors.push(Box::new(CpuCollector::new(args.cycle_model.label()))),
            other => bail!("unknown collector '{other}'; supported: alloc, events, cpu"),
        }
    }

    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::None)
        .with_cycle_model(args.cycle_model.to_emu())
        .with_time_mode(TimeMode::Simulated(0))
        .with_allow_unaligned_access(true)
        .with_profile_session(trace_dir.clone(), &metadata, collectors)
        .context("Failed to start profile session")?;

    let sp_value = 0x8000_0000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);
    emulator.set_pc(load_info.entry_point);
    emulator.set_profile_gate(args.mode.build_gate());

    let emulator_arc = Arc::new(Mutex::new(emulator));

    let transport = SerialEmuClientTransport::new(emulator_arc.clone())
        .with_backtrace(load_info.symbol_map.clone(), load_info.code_end);

    let client = LpClient::new(Box::new(transport));

    let workload_result =
        workload::run_workload(&client, &emulator_arc, &dir, &project_uid, args.max_cycles).await;

    if let Err(e) = &workload_result {
        eprintln!("Workload stopped early: {e:#}");
    }

    let cycles_used = {
        let emu = emulator_arc.lock().unwrap();
        emu.get_cycle_count()
    };

    let workload_name = args.dir.display().to_string();

    let mut session = {
        let mut emu = emulator_arc.lock().unwrap();
        emu.take_profile_session()
            .context("profile session missing at finish")?
    };

    let symbolizer = Symbolizer::new(&trace_symbols);

    if let Some(cpu) = session
        .collectors()
        .iter()
        .find_map(|c| c.as_any().downcast_ref::<CpuCollector>())
    {
        output_speedscope::write(
            cpu,
            &symbolizer as &dyn PcSymbolizer,
            &workload_name,
            mode_slug,
            &trace_dir.join("cpu-profile.speedscope.json"),
        )?;
        output_cpu_json::write(
            cpu,
            &symbolizer as &dyn PcSymbolizer,
            &trace_dir.join("cpu-profile.json"),
        )?;
    }

    let counts = session
        .finish_with_symbolizer(Some(&symbolizer as &dyn PcSymbolizer))
        .context("Failed to flush profile session")?;

    if let Ok(outcome) = &workload_result {
        if let workload::WorkloadOutcome::GuestHalted(reason) = outcome {
            match reason {
                HaltReason::Oom { size } => {
                    eprintln!("Guest halted: OOM (size {size})");
                }
                HaltReason::ProfileStop => {
                    eprintln!("Guest halted: profile stop");
                }
            }
        }
        output::update_metadata_finish(&trace_dir, cycles_used, outcome)?;
    }

    for (name, n) in &counts {
        eprintln!("Trace complete: {name}: {n} events");
    }
    eprintln!(
        "Report written to {}",
        trace_dir.join("report.txt").display()
    );

    println!("{}", trace_dir.display());

    Ok(())
}

fn validate_collectors(names: &[String]) -> Result<()> {
    let mut seen = HashSet::new();
    for raw in names {
        let name = raw.trim();
        if name.is_empty() {
            bail!("empty collector name in --collect");
        }
        if !seen.insert(name) {
            bail!("duplicate collector '{name}' in --collect");
        }
        if name != "alloc" && name != "events" && name != "cpu" {
            bail!("unknown collector '{name}'; supported: alloc, events, cpu");
        }
    }
    Ok(())
}

fn read_project_uid(dir: &std::path::Path) -> Result<String> {
    let project_json = dir.join("project.json");
    let content = std::fs::read_to_string(&project_json)
        .with_context(|| format!("Failed to read {}", project_json.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&content).context("Failed to parse project.json")?;
    let uid = value["uid"]
        .as_str()
        .context("project.json missing 'uid' field")?;
    Ok(uid.to_string())
}

fn derive_dir_label(input_dir: &Path, cwd: &Path) -> String {
    if input_dir.as_os_str().is_empty() {
        return "unknown".to_string();
    }

    if input_dir.is_relative() {
        return finalize_dir_label(rel_label_from_components(input_dir));
    }

    let (eff_in, eff_cwd) = match (input_dir.canonicalize(), cwd.canonicalize()) {
        (Ok(i), Ok(w)) => (i, w),
        _ => (input_dir.to_path_buf(), cwd.to_path_buf()),
    };

    if eff_in.starts_with(&eff_cwd) {
        if let Ok(rest) = eff_in.strip_prefix(&eff_cwd) {
            if rest.as_os_str().is_empty() {
                return "unknown".to_string();
            }
            return finalize_dir_label(rel_label_from_components(rest));
        }
    }

    finalize_dir_label(last_two_segment_label(input_dir))
}

fn rel_label_from_components(path: &Path) -> String {
    let mut stack: Vec<String> = Vec::new();
    for c in path.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = stack.pop();
            }
            Component::Normal(os) => {
                let part = kebab_case(&os.to_string_lossy());
                if !part.is_empty() {
                    stack.push(part);
                }
            }
        }
    }
    stack.join("-")
}

fn finalize_dir_label(s: String) -> String {
    let trimmed = s.trim_matches('-');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

fn last_two_segment_label(path: &Path) -> String {
    let normals: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(os) => {
                let part = kebab_case(&os.to_string_lossy());
                (!part.is_empty()).then_some(part)
            }
            _ => None,
        })
        .collect();
    match normals.len() {
        0 => String::new(),
        1 => normals[0].clone(),
        _ => normals[normals.len() - 2..].join("-"),
    }
}

fn kebab_case(s: &str) -> String {
    let kebab: String = s
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let mut result = String::new();
    for c in kebab.chars() {
        if c == '-' && result.ends_with('-') {
            continue;
        }
        result.push(c);
    }
    result.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn relative_path_examples_basic() {
        assert_eq!(
            derive_dir_label(Path::new("examples/basic"), Path::new("/any/cwd")),
            "examples-basic"
        );
    }

    #[test]
    fn relative_path_with_dot_slash_prefix() {
        assert_eq!(
            derive_dir_label(Path::new("./examples/basic"), Path::new("/any/cwd")),
            "examples-basic"
        );
    }

    #[test]
    fn absolute_under_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("examples/basic")).unwrap();
        let cwd = tmp.path().canonicalize().unwrap();
        let input = cwd.join("examples/basic");
        assert_eq!(derive_dir_label(&input, &cwd), "examples-basic");
    }

    #[test]
    fn absolute_outside_cwd_last_two() {
        assert_eq!(
            derive_dir_label(
                Path::new("/some/distant/path/projects/myshader"),
                Path::new("/unrelated/cwd"),
            ),
            "projects-myshader"
        );
    }

    #[test]
    fn absolute_single_component() {
        assert_eq!(
            derive_dir_label(Path::new("/myshader"), Path::new("/repo")),
            "myshader"
        );
    }

    #[test]
    fn empty_path_is_unknown() {
        assert_eq!(
            derive_dir_label(Path::new(""), Path::new("/repo")),
            "unknown"
        );
    }

    #[test]
    fn weird_chars_kebab_per_component() {
        assert_eq!(
            derive_dir_label(
                Path::new("examples/foo bar.shader"),
                Path::new("/cwd"),
            ),
            "examples-foo-bar-shader"
        );
    }
}
