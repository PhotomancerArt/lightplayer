//! Frame-driving loop for `lp-cli profile`. Drives the emulator until
//! the profile gate signals stop or the cycle cap is reached.

use anyhow::{Context, Result};
use lp_riscv_emu::{FrameOutcome, Riscv32Emulator, profile::HaltReason};
use lpa_client::TokioLpClient;
use lpfs::LpFsStd;
use std::sync::{Arc, Mutex};

use crate::commands::dev::deploy_project_async;

/// Wall-clock budget (in simulated ms) per outer iteration. Matches
/// the previous m0 cadence.
const FRAME_TICK_MS: u32 = 40;

/// Emulator-side cap on instructions per outer iteration. Prevents a
/// runaway guest from blocking the cycle-budget check.
const MAX_STEPS_PER_FRAME: u64 = 5_000_000;

async fn try_stop_projects(client: &TokioLpClient) {
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
    client: &TokioLpClient,
    emulator_arc: &Arc<Mutex<Riscv32Emulator>>,
    dir: &std::path::Path,
    project_uid: &str,
    max_cycles: u64,
) -> Result<WorkloadOutcome> {
    eprintln!("Deploying project...");
    let local_fs = LpFsStd::new(dir.to_path_buf());
    match deploy_project_async(client, &local_fs, project_uid).await {
        Ok(_) => {}
        Err(e) if is_profile_stop_error(&e) => {
            eprintln!("Profile gate stopped during project deploy.");
            return Ok(WorkloadOutcome::ProfileStopped);
        }
        Err(e) => return Err(e).context("Failed to deploy project"),
    }

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

fn is_profile_stop_error(e: &anyhow::Error) -> bool {
    e.chain().any(|cause| {
        cause
            .to_string()
            .contains("Emulator stopped by profile gate")
    })
}
