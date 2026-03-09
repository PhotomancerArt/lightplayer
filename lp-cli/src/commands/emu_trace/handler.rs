//! emu-trace command: load a project in the emulator with allocation tracing,
//! tick N frames, stop, and write the trace output.

use anyhow::{Context, Result};
use lp_client::LpClient;
use lp_client::transport_emu_serial::SerialEmuClientTransport;
use lp_model::AsLpPath;
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    LogLevel, Riscv32Emulator, TimeMode,
    alloc_trace::{TraceMetadata, TraceSymbol},
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;
use lp_shared::fs::{LpFs, LpFsStd};
use std::sync::{Arc, Mutex};

use super::args::EmuTraceArgs;

pub fn handle_emu_trace(args: EmuTraceArgs) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(handle_emu_trace_async(args))
}

async fn handle_emu_trace_async(args: EmuTraceArgs) -> Result<()> {
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

    // Build fw-emu with alloc-trace feature
    eprintln!("Building fw-emu with alloc-trace...");
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true)
            .with_features(&["alloc-trace"]),
    )
    .map_err(|e| anyhow::anyhow!("Failed to build fw-emu: {e}"))?;

    // Load ELF
    let elf_data = std::fs::read(&fw_emu_path).context("Failed to read fw-emu ELF")?;
    let load_info = load_elf(&elf_data).map_err(|e| anyhow::anyhow!("Failed to load ELF: {e}"))?;

    // Build trace directory path: traces/YYYY-MM-DD-HHmmss-<project>/
    let timestamp = chrono::Local::now().format("%Y-%m-%d-%H%M%S");
    let sanitized_uid = project_uid.replace(|c: char| !c.is_alphanumeric() && c != '-', "_");
    let trace_dir_name = format!("{timestamp}-{sanitized_uid}");
    let trace_dir = std::path::PathBuf::from("traces").join(&trace_dir_name);

    let metadata = TraceMetadata {
        version: 1,
        timestamp: chrono::Utc::now().to_rfc3339(),
        project: project_uid.clone(),
        frames_requested: args.frames,
        heap_start: 0x80000000,
        heap_size: 256 * 1024,
        symbols: load_info
            .symbol_list
            .iter()
            .map(|s| TraceSymbol {
                addr: s.addr,
                size: s.size,
                name: s.name.clone(),
            })
            .collect(),
    };

    // Create emulator with alloc tracing
    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::None)
        .with_time_mode(TimeMode::Simulated(0))
        .with_allow_unaligned_access(true)
        .with_alloc_trace(&trace_dir, &metadata)
        .context("Failed to enable alloc trace")?;

    let sp_value = 0x80000000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);
    emulator.set_pc(load_info.entry_point);

    let emulator_arc = Arc::new(Mutex::new(emulator));

    let transport = SerialEmuClientTransport::new(emulator_arc.clone())
        .with_backtrace(load_info.symbol_map.clone(), load_info.code_end);

    let client = LpClient::new(Box::new(transport));

    // Push project files
    eprintln!("Syncing project files...");
    let local_fs = LpFsStd::new(dir);
    push_project_files(&client, &local_fs, &project_uid).await?;

    // Load project
    eprintln!("Loading project...");
    let project_path = format!("projects/{project_uid}");
    client
        .project_load(&project_path)
        .await
        .context("Failed to load project")?;

    // Tick N frames
    eprintln!("Running {} frames...", args.frames);
    for i in 0..args.frames {
        {
            let mut emu = emulator_arc.lock().unwrap();
            emu.advance_time(40); // ~25fps
        }

        if (i + 1) % 10 == 0 || i + 1 == args.frames {
            eprint!("\r  frame {}/{}", i + 1, args.frames);
        }
    }
    eprintln!();

    // Stop
    eprintln!("Stopping project...");
    client
        .stop_all_projects()
        .await
        .context("Failed to stop projects")?;

    // Flush trace
    let event_count = {
        let mut emu = emulator_arc.lock().unwrap();
        emu.finish_alloc_trace().context("Failed to flush trace")?
    };

    eprintln!("Trace complete: {event_count} events");
    println!("{}", trace_dir.display());

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

async fn push_project_files(
    client: &LpClient,
    local_fs: &dyn LpFs,
    project_uid: &str,
) -> Result<()> {
    let entries = local_fs
        .list_dir("/".as_path(), true)
        .map_err(|e| anyhow::anyhow!("Failed to list project files: {e:?}"))?;

    for entry in entries {
        if entry.as_str().ends_with('/') {
            continue;
        }
        if local_fs.is_dir(entry.as_path()).unwrap_or(false) {
            continue;
        }
        let content = local_fs
            .read_file(entry.as_path())
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {e:?}", entry.as_str()))?;

        let relative = if entry.as_str().starts_with('/') {
            &entry.as_str()[1..]
        } else {
            entry.as_str()
        };

        let full_path = format!("/projects/{project_uid}/{relative}");
        client
            .fs_write(full_path.as_path(), content)
            .await
            .with_context(|| format!("Failed to write {full_path}"))?;
    }
    Ok(())
}
