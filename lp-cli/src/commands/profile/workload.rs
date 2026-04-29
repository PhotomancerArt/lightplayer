//! Frame-driving loop for `lp-cli profile`. Drives the emulator until
//! the profile gate signals stop or the cycle cap is reached.

use anyhow::{Context, Result};
use lp_client::LpClient;
use lp_riscv_emu::{FrameOutcome, Riscv32Emulator, profile::HaltReason};
use lpc_model::AsLpPath;
use lpfs::{LpFs, LpFsStd};
use std::sync::{Arc, Mutex};

/// Wall-clock budget (in simulated ms) per outer iteration. Matches
/// the previous m0 cadence.
const FRAME_TICK_MS: u32 = 40;

/// Emulator-side cap on instructions per outer iteration. Prevents a
/// runaway guest from blocking the cycle-budget check.
const MAX_STEPS_PER_FRAME: u64 = 5_000_000;

async fn try_stop_projects(client: &LpClient) {
    if let Err(e) = client.stop_all_projects().await {
        eprintln!("warning: failed to stop projects (continuing): {e:#}");
    }
}

pub enum WorkloadOutcome {
    /// The profile gate requested stop.
    ProfileStopped,
    /// `--max-cycles` was hit before the gate stopped or the guest halted.
    MaxCyclesReached,
    /// The guest halted on its own (OOM, exit, etc.).
    GuestHalted(HaltReason),
}

/// Push project files, load the project, then drive frames until
/// `outcome` is determined. Reports progress on stderr.
pub async fn run_workload(
    client: &LpClient,
    emulator_arc: &Arc<Mutex<Riscv32Emulator>>,
    dir: &std::path::Path,
    project_uid: &str,
    max_cycles: u64,
) -> Result<WorkloadOutcome> {
    eprintln!("Syncing project files...");
    let local_fs = LpFsStd::new(dir.to_path_buf());
    push_project_files(client, &local_fs, project_uid).await?;

    eprintln!("Loading project...");
    let project_path = format!("projects/{project_uid}");
    client
        .project_load(&project_path)
        .await
        .context("Failed to load project")?;

    eprintln!("Driving frames (mode-gated; --max-cycles {max_cycles})...");
    let mut last_print_cycle = 0u64;
    loop {
        let outcome = {
            let mut emu = emulator_arc.lock().unwrap();
            emu.advance_time(FRAME_TICK_MS);
            // Bug fix from m0: actually run guest instructions for
            // the simulated tick window.
            let outcome = emu.run_until_yield_or_stop(MAX_STEPS_PER_FRAME);
            let cycle = emu.get_cycle_count();
            if cycle >= max_cycles {
                eprintln!();
                eprintln!("warning: --max-cycles ({max_cycles}) reached");
                try_stop_projects(client).await;
                return Ok(WorkloadOutcome::MaxCyclesReached);
            }
            if cycle.saturating_sub(last_print_cycle) >= 5_000_000 {
                eprint!("\r  cycle {cycle}/{max_cycles}");
                last_print_cycle = cycle;
            }
            outcome
        };
        match outcome {
            FrameOutcome::Yielded => continue,
            FrameOutcome::ProfileStop => {
                eprintln!();
                // Don't bother with stopAllProjects: the profile gate has
                // halted the emulator's run loop, so any further RPC will
                // just trigger an EmulatorError::ProfileStopped during
                // teardown. The trace data we want is already collected.
                return Ok(WorkloadOutcome::ProfileStopped);
            }
            FrameOutcome::Halted(reason) => {
                let r = reason;
                eprintln!();
                try_stop_projects(client).await;
                return Ok(WorkloadOutcome::GuestHalted(r));
            }
        }
    }
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
