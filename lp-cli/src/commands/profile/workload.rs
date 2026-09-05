//! Frame-driving loop for `lp-cli profile`. Drives the emulator until
//! the profile gate signals stop or the cycle cap is reached.

use anyhow::{Context, Result};
use lp_emu_core::profile::HaltReason;
use lp_riscv_emu::{FrameOutcome, Riscv32Emulator};
use lpa_client::TokioLpClient;
use lpc_wire::{
    NodeReadQuery, NodeReadSelection, ProjectProbeRequest, ProjectReadEvent, ProjectReadQuery,
    ProjectReadQueryEvent, ProjectReadRequest, ReadLevel, ResourcePayloadRead, ResourceReadQuery,
    RuntimeReadQuery, ShapeReadQuery, WireProjectHandle,
};
use lpfs::LpFsStd;
use std::sync::{Arc, Mutex};

use super::args::WorkloadArg;
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
    steps: WorkloadArg,
) -> Result<WorkloadOutcome> {
    eprintln!("Deploying project...");
    let local_fs = LpFsStd::new(dir.to_path_buf());
    let handle = match deploy_project_async(client, &local_fs, project_uid).await {
        Ok(handle) => handle,
        Err(e) if is_profile_stop_error(&e) => {
            eprintln!("Profile gate stopped during project deploy.");
            return Ok(WorkloadOutcome::ProfileStopped);
        }
        Err(e) => return Err(e).context("Failed to deploy project"),
    };

    if steps == WorkloadArg::StudioSync
        && let Some(outcome) = studio_sync(client, handle).await
    {
        return Ok(outcome);
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

/// How many nodes Studio asks for per slot-detail page
/// (`lpa-studio-core::app::project::project_sync::INITIAL_SYNC_SLOT_PAGE_NODES`).
const INITIAL_SYNC_SLOT_PAGE_NODES: usize = 16;

/// Send Studio's staged initial sync, so the trace carries a `project-read`
/// window with the shape a real Studio open produces.
///
/// The stages mirror `lpa-studio-core`'s `ProjectSync` exactly — skeleton
/// (every query, no slots, no probes), then per-node slot detail paged by id,
/// then one probe per read — but the requests are built from `lpc_wire` here
/// rather than reached for through Studio, which would drag the whole UI crate
/// into `lp-cli` for four struct literals.
///
/// ⚠️ Every slot page sends `since: None`. The server's per-root gate includes
/// a root only when `since == 0` or the root's revision is newer, and the
/// skeleton has already advanced this reader past every root — a stateful
/// `since` here would exclude every root and the stage would "succeed" empty.
///
/// Returns `Some(outcome)` when the profile gate stopped the emulator part-way
/// through, which is the normal end of a `--mode startup` run: the gate stops
/// at the frame that contained the first shader compile, and the staged reads
/// are being served in those same ticks, so the later stages simply do not
/// happen. What lands in the trace is whatever completed first.
async fn studio_sync(client: &TokioLpClient, handle: WireProjectHandle) -> Option<WorkloadOutcome> {
    eprintln!("Sending Studio's staged initial sync...");
    let skeleton = match read_stage(client, handle, "skeleton", skeleton_request()).await {
        Ok(events) => events,
        Err(stopped) => return stopped,
    };

    let node_ids = node_ids_from(&skeleton);
    eprintln!("  skeleton: {} node(s)", node_ids.len());
    for (page, ids) in node_ids.chunks(INITIAL_SYNC_SLOT_PAGE_NODES).enumerate() {
        let request = ProjectReadRequest {
            since: None,
            queries: vec![ProjectReadQuery::Nodes(NodeReadQuery {
                level: ReadLevel::Detail,
                nodes: NodeReadSelection::ByIds(ids.to_vec()),
                include_slots: true,
            })],
            probes: Vec::new(),
        };
        if let Err(stopped) =
            read_stage(client, handle, &format!("slot page {}", page + 1), request).await
        {
            return stopped;
        }
    }

    let probe = ProjectReadRequest {
        since: None,
        queries: Vec::new(),
        probes: vec![ProjectProbeRequest::BindingGraph(
            lpc_wire::BindingGraphProbeRequest {
                include_values: false,
            },
        )],
    };
    if let Err(stopped) = read_stage(client, handle, "binding-graph probe", probe).await {
        return stopped;
    }
    None
}

/// Stage 1: every query except per-node slot detail, and no probes — the shape
/// the classic's empirics matrix proves always fits.
fn skeleton_request() -> ProjectReadRequest {
    ProjectReadRequest {
        since: None,
        queries: vec![
            ProjectReadQuery::Shapes(ShapeReadQuery {
                level: ReadLevel::Detail,
            }),
            ProjectReadQuery::Nodes(NodeReadQuery {
                level: ReadLevel::Detail,
                nodes: NodeReadSelection::All,
                include_slots: false,
            }),
            ProjectReadQuery::Resources(ResourceReadQuery {
                level: ReadLevel::Summary,
                payloads: ResourcePayloadRead::None,
            }),
            ProjectReadQuery::Runtime(RuntimeReadQuery),
        ],
        probes: Vec::new(),
    }
}

/// One staged read. `Err(Some(..))` means the profile gate stopped the run —
/// an expected ending, not a failure; `Err(None)` means the read itself failed
/// and the run should carry on into the frame loop so the trace still lands.
async fn read_stage(
    client: &TokioLpClient,
    handle: WireProjectHandle,
    label: &str,
    request: ProjectReadRequest,
) -> Result<Vec<ProjectReadEvent>, Option<WorkloadOutcome>> {
    match client.project_read(handle, request).await {
        Ok(events) => Ok(events),
        Err(e) if is_profile_stop_error(&e) => {
            eprintln!("  {label}: profile gate stopped the run");
            Err(Some(WorkloadOutcome::ProfileStopped))
        }
        Err(e) => {
            eprintln!("  {label}: read failed (continuing): {e:#}");
            Err(None)
        }
    }
}

/// The node ids the skeleton delivered, in the order the tree deltas named
/// them — the same set Studio pages over.
fn node_ids_from(events: &[ProjectReadEvent]) -> Vec<lpc_model::NodeId> {
    let mut ids = Vec::new();
    for event in events {
        let ProjectReadEvent::Query {
            event:
                ProjectReadQueryEvent::Nodes(lpc_wire::ProjectReadNodeEvent::TreeDeltas { deltas }),
            ..
        } = event
        else {
            continue;
        };
        for delta in deltas {
            let id = delta.node_id();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

fn is_profile_stop_error(e: &anyhow::Error) -> bool {
    e.chain().any(|cause| {
        cause
            .to_string()
            .contains("Emulator stopped by profile gate")
    })
}
