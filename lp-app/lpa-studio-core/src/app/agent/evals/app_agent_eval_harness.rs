//! Stage A of the app-agent evals: run one scenario through a headless
//! Studio over an in-process server, judge the resulting project, and
//! write what happened to `target/app-agent-evals/<run>/<scenario>/`.
//!
//! The server wears the **XIAO ESP32-C6's real pin map**
//! (`lpc_hardware::default_esp32c6_hardware_manifest`), so `ws281x:local:D6`
//! opens here the way it opens on a board and a wrong label fails here the
//! way it fails there. Stage B (`lp-cli/tests/app_agent_emu_decode.rs`)
//! then deploys the written tree to an emulated C6.
//!
//! A run is driven by an [`EvalDriver`]:
//! - [`EvalDriver::Golden`] — the committed project tree, no model (proves
//!   the checks pass on known-good projects);
//! - [`EvalDriver::Live`] — a real OpenRouter model through the real app
//!   chat (`just app-agent-eval`; never CI);
//! - [`EvalDriver::Scripted`] — the real app chat over canned model turns
//!   (the deterministic leg of the agent path).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpfs::LpFsMemory;

use lpa_agent::{TokenUsage, TurnEvent};

use super::app_agent_checks::{CheckInput, CheckResult, NodeStatusRow, run_checks};
use super::app_agent_eval_driver::{
    AgentEvalStudio, DrivenRun, ModelSource, drive_scenario, openrouter_key_from_env,
};
use super::app_agent_project_tree::ProjectTree;
use super::app_agent_scenario::{Scenario, ScenarioStart, fixtures_dir};
use super::app_agent_transcript::EvalTranscript;
use crate::app::project::node::NodeController;
use crate::app::project::project_node_tree_view::ProjectNodeStatusTone;
use crate::app::studio::studio_edit_e2e_tests::{InProcessServerIo, drive, project_action};
use crate::app::studio::studio_view_channel::{CommandSender, StudioViewReceiver};
use crate::{
    ProjectOp, StudioActor, StudioController, StudioServerClient, UiStudioView, has_unsaved_work,
};

/// Where the in-process server keeps the project under test.
const EVAL_PROJECT_DIR: &str = "/projects/app-agent-eval";

/// How a run produces its project.
#[derive(Clone, Debug)]
pub(crate) enum EvalDriver {
    /// A committed project tree (`golden/<name>/`), loaded as-is.
    Golden(String),
    /// A real OpenRouter model (slug), keyed from the environment.
    Live { model: String },
    /// Canned model turns, one script per message the scenario sends.
    Scripted(Vec<Vec<Vec<TurnEvent>>>),
}

impl EvalDriver {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Golden(name) => format!("golden:{name}"),
            Self::Live { model } => format!("openrouter:{model}"),
            Self::Scripted(_) => "scripted".to_string(),
        }
    }
}

/// Everything one scenario run produced.
pub(crate) struct EvalOutcome {
    pub(crate) scenario: Scenario,
    pub(crate) driver: String,
    pub(crate) start: Option<ProjectTree>,
    pub(crate) project: ProjectTree,
    pub(crate) statuses: Vec<NodeStatusRow>,
    pub(crate) unsaved: bool,
    pub(crate) transcript: EvalTranscript,
    pub(crate) checks: Vec<CheckResult>,
    /// Model usage across the scenario (zero for a golden).
    pub(crate) usage: TokenUsage,
    /// Model turns across the scenario.
    pub(crate) turns: u32,
}

impl EvalOutcome {
    pub(crate) fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }

    /// One line per failed check, for assertion messages.
    pub(crate) fn failures(&self) -> String {
        self.checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| format!("{}: {}", check.name, check.reason))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Run `scenario` with `driver`. Transcript checks are skipped for a
/// golden (it had no conversation).
pub(crate) fn run_scenario(scenario: &Scenario, driver: &EvalDriver) -> EvalOutcome {
    let start = start_tree(&scenario.start);
    let (project, transcript, statuses, unsaved, usage, turns) = match driver {
        EvalDriver::Golden(name) => {
            let project = golden_tree(name);
            let mut studio = EvalStudio::with_project(&project);
            studio.settle(6);
            let statuses = studio.node_statuses();
            let unsaved = studio.unsaved();
            (
                project,
                EvalTranscript::default(),
                statuses,
                unsaved,
                TokenUsage::default(),
                0,
            )
        }
        EvalDriver::Live { model } => {
            let api_key = openrouter_key_from_env().expect(
                "the live leg needs OPENROUTER_API_KEY or ~/.lightplayer/settings.json \
                 agent.openrouter_api_key",
            );
            // reqwest needs a reactor; worker threads drive its IO while
            // this thread polls the run (tests are edges).
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("a tokio runtime");
            let _guard = runtime.enter();
            let mut studio = AgentEvalStudio::new(ModelSource::Live {
                model: model.clone(),
                api_key,
            });
            driven(drive_scenario(&mut studio, scenario, golden_tree))
        }
        EvalDriver::Scripted(scripts) => {
            let mut studio = AgentEvalStudio::new(ModelSource::Scripted(scripts.clone()));
            driven(drive_scenario(&mut studio, scenario, golden_tree))
        }
    };

    let mut judged = scenario.clone();
    if matches!(driver, EvalDriver::Golden(_)) {
        judged.checks.retain(|check| !check.needs_transcript());
    }
    let checks = run_checks(&CheckInput {
        scenario: &judged,
        start: start.as_ref(),
        project: &project,
        statuses: Some(&statuses),
        unsaved: Some(unsaved),
        transcript: &transcript,
    });
    EvalOutcome {
        scenario: scenario.clone(),
        driver: driver.label(),
        start,
        project,
        statuses,
        unsaved,
        transcript,
        checks,
        usage,
        turns,
    }
}

type DrivenParts = (
    ProjectTree,
    EvalTranscript,
    Vec<NodeStatusRow>,
    bool,
    TokenUsage,
    u32,
);

fn driven(run: DrivenRun) -> DrivenParts {
    (
        run.project,
        run.transcript,
        run.statuses,
        run.unsaved,
        run.usage,
        run.turns,
    )
}

/// Write `outcome` under `run_dir/<dir_name>/`: the project tree,
/// `transcript.json` and `report.json`. Never writes provider settings.
pub(crate) fn write_outcome(
    run_dir: &Path,
    dir_name: &str,
    outcome: &EvalOutcome,
) -> std::io::Result<PathBuf> {
    let dir = run_dir.join(dir_name);
    let project_dir = dir.join("project");
    if project_dir.exists() {
        std::fs::remove_dir_all(&project_dir)?;
    }
    outcome.project.write_to_dir(&project_dir)?;
    std::fs::write(
        dir.join("transcript.json"),
        serde_json::to_vec_pretty(&outcome.transcript).expect("transcript serializes"),
    )?;
    let report = serde_json::json!({
        "scenario": outcome.scenario.name,
        "summary": outcome.scenario.summary,
        "driver": outcome.driver,
        "leds": outcome.scenario.leds,
        "started_from": match &outcome.start {
            Some(_) => format!("{:?}", outcome.scenario.start),
            None => "blank".to_string(),
        },
        "passed": outcome.passed(),
        "checks": outcome.checks,
        "statuses": outcome.statuses,
        "unsaved": outcome.unsaved,
        "turns": outcome.turns,
        "tokens_in": outcome.usage.input_tokens
            + outcome.usage.cache_read_tokens
            + outcome.usage.cache_write_tokens,
        "tokens_out": outcome.usage.output_tokens,
        "cost_usd": outcome.usage.reported_cost_usd(),
    });
    std::fs::write(
        dir.join("report.json"),
        serde_json::to_vec_pretty(&report).expect("report serializes"),
    )?;
    Ok(dir)
}

/// `target/app-agent-evals/<run>` at the workspace root.
pub(crate) fn eval_run_dir(run: &str) -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    target.join("app-agent-evals").join(run)
}

pub(crate) fn golden_tree(name: &str) -> ProjectTree {
    let dir = fixtures_dir().join("golden").join(name);
    ProjectTree::from_dir(&dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
}

fn start_tree(start: &ScenarioStart) -> Option<ProjectTree> {
    match start {
        ScenarioStart::Blank => None,
        ScenarioStart::Golden(name) => Some(golden_tree(name)),
    }
}

/// The eval actor's timer: instant (the harness drives batches itself).
pub(crate) type NoTimer = fn(core::time::Duration) -> core::future::Ready<()>;

pub(crate) fn no_timer(_: core::time::Duration) -> core::future::Ready<()> {
    core::future::ready(())
}

/// A headless Studio connected to an in-process server wearing the XIAO
/// C6 pin map, with one project loaded.
pub(crate) struct EvalStudio {
    actor: StudioActor<NoTimer>,
    tx: CommandSender,
    views: StudioViewReceiver,
    pub(crate) view: Option<UiStudioView>,
}

impl EvalStudio {
    /// Load `tree` on a fresh server and connect Studio to it.
    pub(crate) fn with_project(tree: &ProjectTree) -> Self {
        let mut server = xiao_c6_server();
        for (path, bytes) in &tree.files {
            server
                .base_fs_mut()
                .write_file(format!("{EVAL_PROJECT_DIR}/{path}").as_path(), bytes)
                .expect("write project file");
        }
        server
            .load_project(EVAL_PROJECT_DIR.as_path())
            .expect("the eval project loads");
        server.advance_frame(16).expect("tick");
        let io = InProcessServerIo {
            server: Rc::new(RefCell::new(server)),
            inbox: Rc::new(RefCell::new(VecDeque::new())),
            sent: Rc::new(RefCell::new(Vec::new())),
        };
        let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
        let controller = StudioController::connected_with_client_for_test(client);
        let (actor, handle) = StudioActor::new(controller, no_timer as NoTimer);
        let mut studio = Self {
            actor,
            tx: handle.tx,
            views: handle.view,
            view: None,
        };
        studio.act(project_action(ProjectOp::ConnectRunningProject));
        studio
    }

    /// Send one command and run its batch.
    pub(crate) fn act(&mut self, command: crate::StudioCommand) {
        self.tx.send(command);
        drive(self.actor.run_one_batch_for_test());
        if let Some(view) = self.views.try_recv() {
            self.view = Some(view);
        }
    }

    /// Refresh `reads` times (each read ticks the server a frame), so
    /// compiles finish and statuses settle.
    pub(crate) fn settle(&mut self, reads: usize) {
        for _ in 0..reads {
            self.act(project_action(ProjectOp::RefreshProject));
        }
    }

    /// Every node's status as the project controller holds it.
    pub(crate) fn node_statuses(&mut self) -> Vec<NodeStatusRow> {
        let project = self.actor.controller_mut_for_test().project_for_test();
        let mut rows = Vec::new();
        collect_statuses(project.root_nodes(), &mut rows);
        rows
    }

    /// The app agent's readout of this studio, ids minted.
    pub(crate) fn readout(&mut self) -> String {
        self.actor
            .controller_mut_for_test()
            .app_agent_readout_for_test()
            .mint()
            .0
    }

    /// Whether unsaved authored edits remain.
    pub(crate) fn unsaved(&mut self) -> bool {
        let project = self.actor.controller_mut_for_test().project_for_test();
        has_unsaved_work(&project.dirty_summary())
    }
}

pub(crate) fn collect_statuses(nodes: &[NodeController], rows: &mut Vec<NodeStatusRow>) {
    for node in nodes {
        let status = node.status();
        rows.push(NodeStatusRow {
            path: node.address().to_string(),
            kind: node.kind().to_string(),
            status: status.label.clone(),
            detail: status.detail.clone(),
            ok: status.tone == ProjectNodeStatusTone::Good,
            failed: matches!(
                status.tone,
                ProjectNodeStatusTone::Error | ProjectNodeStatusTone::Fault
            ),
        });
        collect_statuses(node.children(), rows);
    }
}

/// A server with nothing loaded whose outputs open against the XIAO
/// ESP32-C6's manifest (D6 = GPIO16, D10 = GPIO18, two RMT channels).
pub(crate) fn xiao_c6_server() -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::with_hardware_manifest(
        lpc_hardware::default_esp32c6_hardware_manifest(),
    )));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        None,
        None,
        graphics,
    )
}
