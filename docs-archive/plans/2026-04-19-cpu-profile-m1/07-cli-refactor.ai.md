# Phase 7 — CLI refactor (args, handler/workload/output split, wire-up)

Refactor `lp-cli/src/commands/profile/handler.rs` into a slim
orchestrator and split out `workload.rs` + `output.rs`. Add the
`--mode` and `--max-cycles` args, drop `--frames`. Wire in the
`EventsCollector`, the mode gate, and the new
`run_until_yield_or_stop` loop. Update the metadata struct fields.

This phase depends on phases 5 and 6.

## Subagent assignment

`generalPurpose` subagent. Multi-file refactor in lp-cli only;
no behavior outside lp-cli changes. Per the design doc, the new
files have well-pinned shapes.

## Files to touch

```
lp-cli/src/commands/profile/
├── args.rs                 # UPDATE: + --mode, + --max-cycles, drop --frames
├── handler.rs              # REWRITE: slim orchestrator
├── workload.rs             # NEW: frame-driving + ProfileStop loop
├── output.rs               # NEW: meta.json + report.txt writers
├── mod.rs                  # UPDATE: + pub mod workload; + pub mod output;

lp-riscv/lp-riscv-emu/src/profile/mod.rs   # UPDATE: SessionMetadata fields
                                            # (drop frames_requested,
                                            #  add mode/max_cycles/
                                            #   cycles_used/terminated_by)
```

## Edits

### `args.rs`

Per design doc, "CLI surface". Replace `frames` with `mode` +
`max_cycles`:

```rust
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use super::mode::ProfileMode;

#[derive(Debug, Parser)]
#[command(name = "profile", about = "Run a profiling session or compare two profile directories.")]
pub struct ProfileCli {
    #[command(subcommand)]
    pub subcommand: Option<ProfileSubcommand>,
    #[command(flatten)]
    pub run: ProfileArgs,
}

#[derive(Debug, Subcommand)]
pub enum ProfileSubcommand {
    Diff(ProfileDiffArgs),
}

#[derive(Debug, Args)]
pub struct ProfileArgs {
    /// Workload directory (defaults to examples/basic).
    #[arg(default_value = "examples/basic")]
    pub dir: PathBuf,

    /// Collectors to enable (comma-separated). m1 supports: alloc, events.
    /// Default: events. (events is implicitly fed to the mode gate even
    /// when not in this list, but events.jsonl is only written when
    /// "events" is included here.)
    #[arg(long, default_value = "events", value_delimiter = ',')]
    pub collect: Vec<String>,

    /// Profile mode (state machine over the perf-event stream).
    #[arg(long, value_enum, default_value_t = ProfileMode::SteadyRender)]
    pub mode: ProfileMode,

    /// Safety cap on emulated cycles. The run terminates with exit
    /// code 0 and a warning if reached.
    #[arg(long, default_value_t = 200_000_000)]
    pub max_cycles: u64,

    /// Optional human-readable note appended to the profile dir.
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProfileDiffArgs {
    pub a: PathBuf,
    pub b: PathBuf,
}
```

`--frames` is removed entirely. The m0 frame-driving bug (the loop
only advanced the clock without running emulator instructions) is
fixed by the new `workload.rs` (see below).

### `mod.rs`

```rust
pub mod args;
pub mod diff_stub;
pub mod handler;
pub mod mode;          // (declared in phase 6)
pub mod output;        // NEW
pub mod workload;      // NEW
```

### `workload.rs`

```rust
//! Frame-driving loop for `lp-cli profile`. Drives the emulator until
//! the profile gate signals stop or the cycle cap is reached.

use anyhow::{Context, Result};
use lp_client::LpClient;
use lp_model::AsLpPath;
use lp_riscv_emu::{Riscv32Emulator, profile::HaltReason};
use lp_riscv_emu::emu::emulator::state::FrameOutcome;
use lp_shared::fs::{LpFs, LpFsStd};
use std::sync::{Arc, Mutex};

/// Wall-clock budget (in simulated ms) per outer iteration. Matches
/// the previous m0 cadence.
const FRAME_TICK_MS: u32 = 40;

/// Emulator-side cap on instructions per outer iteration. Prevents a
/// runaway guest from blocking the cycle-budget check.
const MAX_STEPS_PER_FRAME: u64 = 5_000_000;

#[derive(Debug)]
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
            let cycle = emu.cycle_count();   // accessor name TBD;
                                             //  whatever exists today
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
                return Ok(WorkloadOutcome::ProfileStopped);
            }
            FrameOutcome::Halted(reason) => {
                eprintln!();
                return Ok(WorkloadOutcome::GuestHalted(reason));
            }
        }
    }
}

async fn push_project_files(
    client: &LpClient,
    local_fs: &dyn LpFs,
    project_uid: &str,
) -> Result<()> {
    /* unchanged from existing handler.rs */
}
```

(Move `push_project_files` verbatim from `handler.rs`.)

### `output.rs`

```rust
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
) -> SessionMetadata {
    SessionMetadata {
        schema_version: 1,
        timestamp: chrono::Utc::now().to_rfc3339(),
        project,
        workload,
        note,
        clock_source: "emu_estimated",
        mode: mode.slug().to_string(),
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
        WorkloadOutcome::ProfileStopped    => "profile_stop",
        WorkloadOutcome::MaxCyclesReached  => "max_cycles",
        WorkloadOutcome::GuestHalted(_)    => "guest_halt",
    });
    let pretty = serde_json::to_string_pretty(&value)?;
    std::fs::write(&path, pretty)?;
    Ok(())
}
```

### `handler.rs` — slim orchestrator

After refactor, `handler.rs` should be ~80–120 lines. Responsibilities:

1. Build fw-emu binary.
2. Load ELF.
3. Compute trace_dir name (timestamp + workload + mode + note).
4. Build initial `SessionMetadata` (delegates to `output::build_initial_metadata`).
5. Build collectors: always-on `EventsCollector`, plus `AllocCollector`
   if requested.
6. Build emulator with `with_profile_session(...)`.
7. Build `Box<dyn Gate>` from `args.mode` and call
   `emulator.set_profile_gate(gate)` (new accessor — small wrapper
   that calls `ProfileSession::set_gate` on the held session).
8. Build LpClient transport.
9. Call `workload::run_workload(...)`.
10. Read final cycle count from emulator.
11. Call `emulator.finish_profile_session()` -> aggregate counts.
12. Call `output::update_metadata_finish(...)`.
13. Print summary (event counts + report path + trace_dir on stdout).

Trace-dir naming change per design doc:
```rust
let dir_label = kebab_case(&args.dir.to_string_lossy());
let mode_slug = args.mode.slug();
let mut profile_dir_name = format!("{timestamp}--{dir_label}--{mode_slug}");
if let Some(note) = &args.note { /* …--<note> */ }
```

Collector validation update — extend `validate_collectors` to accept
`"events"` in addition to `"alloc"`.

Add `set_profile_gate` accessor on `Riscv32Emulator` (small public
method that delegates to `self.profile_session.as_mut().unwrap().set_gate(g)`).
This belongs in phase 5; if missed, fold into this phase as a one-line
addition.

### `lp-riscv-emu/src/profile/mod.rs` — `SessionMetadata` field changes

Drop `frames_requested`. Add four new fields:

```rust
#[derive(Debug, Clone)]
pub struct SessionMetadata {
    pub schema_version: u32,
    pub timestamp: String,
    pub project: String,
    pub workload: String,
    pub note: Option<String>,
    pub clock_source: &'static str,
    pub mode: String,           // NEW
    pub max_cycles: u64,        // NEW
    pub cycles_used: u64,       // NEW (overwritten by output::update_metadata_finish)
    pub terminated_by: String,  // NEW (idem)
    pub symbols: Vec<TraceSymbol>,
}
```

Update `ProfileSession::new`'s meta.json builder: emit the new
fields, drop `frames_requested`. `schema_version` stays at 1 per the
user's call ("nothing is really using it yet").

The `serde_json::Value` literal in `new`:
```rust
serde_json::json!({
    "schema_version": metadata.schema_version,
    "timestamp": metadata.timestamp,
    "project": metadata.project,
    "workload": metadata.workload,
    "note": metadata.note,
    "clock_source": metadata.clock_source,
    "mode": metadata.mode,
    "max_cycles": metadata.max_cycles,
    "cycles_used": metadata.cycles_used,
    "terminated_by": metadata.terminated_by,
    "symbols": metadata.symbols,
    "collectors": collectors_meta_object(),
})
```

## Validation

```bash
cargo build -p lp-cli
cargo test  -p lp-cli

# Sanity smoke (uses real emu+fw build, slow):
cargo run -p lp-cli -- profile --collect events --mode steady-render
# Should produce profiles/<ts>--examples-basic--steady-render/{meta.json,events.jsonl,report.txt}
# meta.json should have mode/max_cycles/cycles_used/terminated_by populated.
```

The full e2e assertion lives in phase 8.

Existing fw-tests integration test
(`lp-fw/fw-tests/tests/profile_alloc_emu.rs`) will break on the
`SessionMetadata` shape change — phase 8 owns the fix.

## Out of scope for this phase

- E2E assertion on events.jsonl content (phase 8).
- Updating `profile_alloc_emu.rs` for new metadata fields (phase 8).
