//! Profile output orchestration: meta.json field assembly + final
//! report file location reporting. The actual file writes happen
//! inside `ProfileSession::new` / `::finish`; this module owns the
//! shape of the metadata struct passed in.

use anyhow::Result;
use lp_riscv_emu::profile::SessionMetadata;
use std::path::Path;

use super::mode::ProfileMode;
use super::workload::WorkloadOutcome;

pub fn build_initial_metadata(
    project: String,
    workload: String,
    note: Option<String>,
    symbols: Vec<lp_riscv_emu::profile::TraceSymbol>,
    mode: ProfileMode,
    max_cycles: u64,
    cycle_model: String,
) -> SessionMetadata {
    SessionMetadata {
        schema_version: 1,
        timestamp: chrono::Utc::now().to_rfc3339(),
        project,
        workload,
        note,
        clock_source: "emu_estimated",
        mode: mode.slug().to_string(),
        cycle_model,
        max_cycles,
        // Placeholders; rewritten via update_metadata_finish() after run.
        cycles_used: 0,
        terminated_by: "running".to_string(),
        symbols,
    }
}

/// After the run, patch meta.json on disk to reflect the actual
/// cycles_used and terminated_by. (ProfileSession::new wrote the
/// initial version; we overwrite it here.)
pub fn update_metadata_finish(
    trace_dir: &Path,
    cycles_used: u64,
    outcome: &WorkloadOutcome,
) -> Result<()> {
    let path = trace_dir.join("meta.json");
    let raw = std::fs::read_to_string(&path)?;
    let mut value: serde_json::Value = serde_json::from_str(&raw)?;
    value["cycles_used"] = serde_json::json!(cycles_used);
    value["terminated_by"] = serde_json::json!(match outcome {
        WorkloadOutcome::ProfileStopped => "profile_stop",
        WorkloadOutcome::MaxCyclesReached => "max_cycles",
        WorkloadOutcome::GuestHalted(_) => "guest_halt",
    });
    let pretty = serde_json::to_string_pretty(&value)?;
    std::fs::write(&path, pretty)?;
    Ok(())
}
