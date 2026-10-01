//! Driving the real app chat through a scenario: a headless Studio with a
//! library, the app session over a live or scripted model, and the loop
//! that interleaves the agent's run with Studio's command batches.
//!
//! The model is reached exactly as the product reaches it: settings select
//! OpenRouter, and the provider factory builds the OpenAI-compatible
//! provider from the settings' config — here over the host `reqwest`
//! transport instead of the browser's fetch.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

use lpa_agent::provider::ReqwestTransport;
use lpa_agent::{
    BoxStream, ChatRole, ContentBlock, ModelProvider, OpenAiCompatProvider, TokenUsage, TurnEvent,
    TurnRequest,
};
use lpfs::LpFsMemory;

use super::app_agent_eval_harness::{NoTimer, no_timer, xiao_c6_server};
use super::app_agent_project_tree::ProjectTree;
use super::app_agent_scenario::{Scenario, ScenarioStart};
use super::app_agent_transcript::{EvalStep, EvalTranscript};
use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
use crate::app::studio::studio_edit_e2e_tests::{InProcessServerIo, drive};
use crate::app::studio::studio_view_channel::{CommandSender, StudioViewReceiver};
use crate::{
    AgentController, AgentOp, AgentProvider, AgentTaskFuture, ControllerId, HOME_NODE_ID, HomeOp,
    SettingsCommand, StudioActor, StudioCommand, StudioController, StudioServerClient, UiAction,
    UiAgentStatus, UiAgentTurn, has_unsaved_work,
};

/// A hang guard for one scenario (network turns included). Not a gate:
/// a scenario that hits it is reported as stopped.
const SCENARIO_WALL_LIMIT: Duration = Duration::from_secs(20 * 60);

/// Where the model comes from.
pub(crate) enum ModelSource {
    /// A real OpenRouter model. The key is held here only for the provider
    /// factory; it never reaches a report.
    Live { model: String, api_key: String },
    /// One turn script per Send (the e2e `ScriptedProvider` pattern).
    Scripted(Vec<Vec<Vec<TurnEvent>>>),
}

/// What a driven scenario produced (the harness judges it).
pub(crate) struct DrivenRun {
    pub(crate) project: ProjectTree,
    pub(crate) transcript: EvalTranscript,
    pub(crate) unsaved: bool,
    pub(crate) usage: TokenUsage,
    /// Model turns across the scenario.
    pub(crate) turns: u32,
    pub(crate) statuses: Vec<super::app_agent_checks::NodeStatusRow>,
}

/// A headless Studio with a memory library and the app chat wired to
/// `source`.
pub(crate) struct AgentEvalStudio {
    actor: StudioActor<NoTimer>,
    tx: CommandSender,
    views: StudioViewReceiver,
    tasks: Rc<RefCell<Vec<AgentTaskFuture>>>,
    store: LibraryStore,
    requests: Rc<RefCell<Vec<TurnRequest>>>,
}

impl AgentEvalStudio {
    pub(crate) fn new(source: ModelSource) -> Self {
        let io = InProcessServerIo {
            server: Rc::new(RefCell::new(xiao_c6_server())),
            inbox: Rc::new(RefCell::new(VecDeque::new())),
            sent: Rc::new(RefCell::new(Vec::new())),
        };
        let client = StudioServerClient::from_io_for_test("in-process", Box::new(io));
        let mut controller = StudioController::connected_with_client_for_test(client);
        let store = LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(|| [7u8; 16]),
            Rc::new(|| "2026-10-01-0900".to_string()),
        );
        controller.attach_library(Rc::new(MemoryLibraryHost::new(
            store.clone(),
            Rc::new(|| 2.0),
        )));

        let (model, api_key) = match &source {
            ModelSource::Live { model, api_key } => (model.clone(), api_key.clone()),
            ModelSource::Scripted(_) => {
                ("scripted/model".to_string(), "sk-or-scripted".to_string())
            }
        };
        for command in [
            SettingsCommand::SetAgentProvider(Some(AgentProvider::OpenRouter)),
            SettingsCommand::SetAgentOpenRouterApiKey(Some(api_key)),
            SettingsCommand::SetAgentModel(Some(model)),
        ] {
            controller.apply_settings_command(command);
        }

        let tasks: Rc<RefCell<Vec<AgentTaskFuture>>> = Rc::new(RefCell::new(Vec::new()));
        let spawner_tasks = Rc::clone(&tasks);
        controller.set_agent_spawner(move |future| spawner_tasks.borrow_mut().push(future));

        let requests: Rc<RefCell<Vec<TurnRequest>>> = Rc::new(RefCell::new(Vec::new()));
        match source {
            ModelSource::Live { .. } => controller.set_agent_provider_factory(|config| {
                let crate::AgentProviderConfig::OpenAiCompat(config) = config else {
                    panic!("OpenRouter resolves to the OpenAI-compatible provider");
                };
                Box::new(OpenAiCompatProvider::new(
                    config.clone(),
                    ReqwestTransport::new(),
                ))
            }),
            ModelSource::Scripted(scripts) => {
                let remaining = RefCell::new(VecDeque::from(scripts));
                let log = Rc::clone(&requests);
                controller.set_agent_provider_factory(move |_| {
                    let turns = remaining.borrow_mut().pop_front().unwrap_or_default();
                    Box::new(ScriptedProvider {
                        turns: RefCell::new(turns.into()),
                        requests: Rc::clone(&log),
                    })
                });
            }
        }

        let (mut actor, handle) = StudioActor::new(controller, no_timer as NoTimer);
        // The agent's ack waits poll a timer between checks; an instant
        // timer would spin its whole budget inside one poll of the run.
        // Yielding once per wait lets the driver run a batch in between.
        actor
            .controller_mut_for_test()
            .agent_for_test()
            .set_timer(|_| Box::pin(YieldOnce(false)) as crate::AgentTimerFuture);
        Self {
            actor,
            tx: handle.tx,
            views: handle.view,
            tasks,
            store,
            requests,
        }
    }

    /// Every request a scripted provider received (empty for a live one).
    pub(crate) fn scripted_requests(&self) -> Vec<TurnRequest> {
        self.requests.borrow().clone()
    }

    /// Put the scenario's start project in front of the user.
    pub(crate) fn start(&mut self, start: &ScenarioStart, golden: impl Fn(&str) -> ProjectTree) {
        match start {
            ScenarioStart::Blank => self.act(UiAction::from_op(
                ControllerId::new(HOME_NODE_ID),
                HomeOp::CreateProject {
                    template: crate::ProjectTemplate::Blank,
                    name: None,
                },
            )),
            ScenarioStart::Golden(name) => {
                // A board-targeted package opens by waiting for that board,
                // and the eval has none: the in-process server IS the board
                // (it wears the XIAO C6's pin map). So the package opens
                // untargeted on it, and the Hardware row's own op puts the
                // target back — the same open project the user would have.
                let mut tree = golden(name);
                let target = strip_target(&mut tree);
                let files: Vec<(String, Vec<u8>)> = tree.files.into_iter().collect();
                let summary = self
                    .store
                    .install_package("Sean's LEDs", &files, PackageProvenance::Created, 2.0)
                    .expect("the golden installs");
                self.act(UiAction::from_op(
                    ControllerId::new(HOME_NODE_ID),
                    HomeOp::OpenPackage {
                        key: summary.uid.to_string(),
                        prefer: None,
                    },
                ));
                if let Some(target) = target {
                    self.act(UiAction::from_op(
                        ControllerId::new(HOME_NODE_ID),
                        HomeOp::SetPackageTarget {
                            uid: summary.uid.to_string(),
                            target: Some(target),
                        },
                    ));
                }
            }
        }
        self.settle(4);
        if self
            .controller()
            .project_for_test()
            .active_library_uid()
            .is_none()
        {
            let view = self.controller().view();
            panic!(
                "the scenario's start project did not open; console: {:#?}",
                view.console
                    .entries
                    .iter()
                    .rev()
                    .take(12)
                    .collect::<Vec<_>>()
            );
        }
    }

    /// Facts the readout cannot show (the scenario's board line).
    pub(crate) fn set_context(&mut self, context: &str) {
        self.controller()
            .agent_for_test()
            .set_app_context_notes(vec![format!("context: {context}")]);
    }

    /// Send one message to the app chat and drive the run to its end, or
    /// until a limit is passed (then the run is stopped the way Stop stops
    /// it, between events).
    pub(crate) fn send(&mut self, text: &str, limits: RunLimits) {
        self.act(UiAction::from_op(
            ControllerId::new(AgentController::NODE_ID),
            AgentOp::AppSend {
                text: text.to_string(),
            },
        ));
        self.drive_runs(limits);
    }

    /// The app chat's last visible assistant text.
    pub(crate) fn last_assistant_text(&mut self) -> String {
        let turns = self
            .controller()
            .agent_for_test()
            .app_session()
            .mirror
            .turns
            .clone();
        turns
            .iter()
            .rev()
            .find_map(|turn| match turn {
                UiAgentTurn::Assistant { text } => Some(text.clone()),
                UiAgentTurn::User { .. } => Some(String::new()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// The app chat's status (an error ends a scenario).
    pub(crate) fn status(&mut self) -> UiAgentStatus {
        self.controller()
            .agent_for_test()
            .app_session()
            .mirror
            .status
            .clone()
    }

    /// The model-facing transcript so far, flattened.
    pub(crate) fn transcript_steps(&mut self) -> Vec<EvalStep> {
        let session = self.controller().agent_for_test().app_session();
        let runtime = session.runtime.borrow();
        let Some(runtime) = runtime.as_ref() else {
            return Vec::new();
        };
        flatten(&runtime.transcript().messages)
    }

    /// Cumulative usage of the app chat.
    pub(crate) fn usage(&mut self) -> TokenUsage {
        self.controller()
            .agent_for_test()
            .app_session()
            .mirror
            .usage
    }

    /// Model turns so far.
    pub(crate) fn turns(&mut self) -> u32 {
        self.controller()
            .agent_for_test()
            .app_session()
            .mirror
            .turn_stats
            .len() as u32
    }

    /// The open project's saved bytes, as the library holds them.
    pub(crate) fn saved_tree(&mut self) -> ProjectTree {
        let Some(uid) = self.controller().project_for_test().active_library_uid() else {
            return ProjectTree::default();
        };
        let uid = self
            .store
            .resolve_key(&uid)
            .expect("the open package resolves");
        let files = self
            .store
            .open(uid)
            .expect("the open package opens")
            .read_all_files()
            .expect("its files read");
        ProjectTree::from_files(
            files
                .into_iter()
                .filter(|(path, _)| !path.starts_with(".lp/") && !path.starts_with("/.lp/")),
        )
    }

    pub(crate) fn unsaved(&mut self) -> bool {
        has_unsaved_work(&self.controller().project_for_test().dirty_summary())
    }

    pub(crate) fn node_statuses(&mut self) -> Vec<super::app_agent_checks::NodeStatusRow> {
        let mut rows = Vec::new();
        super::app_agent_eval_harness::collect_statuses(
            self.controller().project_for_test().root_nodes(),
            &mut rows,
        );
        rows
    }

    /// Refresh `reads` times so compiles finish and statuses settle.
    pub(crate) fn settle(&mut self, reads: usize) {
        for _ in 0..reads {
            self.command(crate::app::studio::studio_edit_e2e_tests::project_action(
                crate::ProjectOp::RefreshProject,
            ));
        }
    }

    fn controller(&mut self) -> &mut StudioController {
        self.actor.controller_mut_for_test()
    }

    fn act(&mut self, action: UiAction) {
        self.command(StudioCommand::Action(action));
    }

    fn command(&mut self, command: StudioCommand) {
        self.tx.send(command);
        drive(self.actor.run_one_batch_for_test());
        while self.views.try_recv().is_some() {}
    }

    /// Run one queued batch if anything is queued. A command queue with
    /// nothing in it is Pending on the first poll, and dropping that
    /// future consumes nothing.
    fn try_batch(&mut self) -> bool {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);
        let mut batch = Box::pin(self.actor.run_one_batch_for_test());
        match batch.as_mut().poll(&mut cx) {
            Poll::Ready(()) => {}
            Poll::Pending => {
                // Pending on the first poll ⇔ nothing queued; a batch that
                // started is driven to completion (its futures resolve
                // in-process).
                drop(batch);
                return false;
            }
        }
        drop(batch);
        while self.views.try_recv().is_some() {}
        true
    }

    /// Drive every spawned run future to completion, interleaving Studio
    /// batches so the agent's ops and acks flow.
    fn drive_runs(&mut self, limits: RunLimits) {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);
        loop {
            let Some(mut task) = self.tasks.borrow_mut().pop() else {
                break;
            };
            loop {
                if let Poll::Ready(()) = Pin::as_mut(&mut task).poll(&mut cx) {
                    break;
                }
                let progressed = self.try_batch();
                let spent = self.usage().reported_cost_usd().unwrap_or(0.0);
                if Instant::now() > limits.deadline
                    || spent > limits.usd
                    || self.turns() > limits.turns
                {
                    self.controller().agent_for_test().request_app_stop();
                }
                if !progressed {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
            while self.try_batch() {}
        }
    }
}

/// When a run is stopped mid-flight.
#[derive(Clone, Copy)]
pub(crate) struct RunLimits {
    pub(crate) deadline: Instant,
    /// Reported cost across the scenario, US dollars.
    pub(crate) usd: f64,
    /// Model turns across the scenario.
    pub(crate) turns: u32,
}

/// Run one scenario on the real app chat. Scripted replies answer the
/// agent's questions; the budget stops the run.
pub(crate) fn drive_scenario(
    studio: &mut AgentEvalStudio,
    scenario: &Scenario,
    golden: impl Fn(&str) -> ProjectTree,
) -> DrivenRun {
    let deadline = Instant::now() + SCENARIO_WALL_LIMIT;
    studio.start(&scenario.start, golden);
    studio.set_context(&scenario.context);
    let mut steps = Vec::new();
    let mut replies: VecDeque<_> = scenario.replies.iter().cloned().collect();
    let mut message = scenario.user.clone();
    let mut stop: Option<String> = None;
    loop {
        let turns_left = scenario.budget.turns.saturating_sub(studio.turns());
        if turns_left == 0 {
            stop = Some(format!("turn budget ({}) spent", scenario.budget.turns));
            break;
        }
        let before = studio.transcript_steps().len();
        studio.send(
            &message,
            RunLimits {
                deadline,
                usd: scenario.budget.usd,
                turns: scenario.budget.turns,
            },
        );
        let all = studio.transcript_steps();
        steps.extend(all.into_iter().skip(before));
        if let UiAgentStatus::Error { message, .. } = studio.status() {
            stop = Some(format!("provider error: {message}"));
            break;
        }
        if Instant::now() > deadline {
            stop = Some("the scenario's wall-clock hang guard fired".to_string());
            break;
        }
        let spent = studio.usage().reported_cost_usd().unwrap_or(0.0);
        if spent > scenario.budget.usd {
            stop = Some(format!(
                "cost budget spent (${spent:.4} > ${})",
                scenario.budget.usd
            ));
            break;
        }
        let said = studio.last_assistant_text();
        if !asks_a_question(&said) {
            break;
        }
        let Some(position) = replies
            .iter()
            .position(|reply| said.to_lowercase().contains(&reply.when_asked_about))
        else {
            stop = Some("the agent asked a question the scenario has no reply for".to_string());
            break;
        };
        let reply = replies.remove(position).expect("found");
        steps.push(EvalStep::ScriptedReply {
            about: reply.when_asked_about.clone(),
            text: reply.text.clone(),
        });
        message = reply.text;
    }
    if let Some(reason) = stop {
        steps.push(EvalStep::Stopped { reason });
    }
    studio.settle(6);
    DrivenRun {
        project: studio.saved_tree(),
        transcript: EvalTranscript { steps },
        unsaved: studio.unsaved(),
        usage: studio.usage(),
        turns: studio.turns(),
        statuses: studio.node_statuses(),
    }
}

/// Remove `target` from the tree's manifest, returning it.
fn strip_target(tree: &mut ProjectTree) -> Option<String> {
    let mut manifest = tree.manifest()?;
    let target = manifest
        .as_object_mut()?
        .remove("target")?
        .as_str()
        .map(str::to_string);
    tree.files.insert(
        "project.json".to_string(),
        serde_json::to_vec_pretty(&manifest).expect("a manifest serializes"),
    );
    target
}

/// The key for a live run: `OPENROUTER_API_KEY`, else
/// `~/.lightplayer/settings.json`'s `agent.openrouter_api_key`. Read, never
/// written anywhere.
pub(crate) fn openrouter_key_from_env() -> Option<String> {
    if let Ok(key) = std::env::var("OPENROUTER_API_KEY")
        && !key.trim().is_empty()
    {
        return Some(key.trim().to_string());
    }
    let home = std::env::var_os("HOME")?;
    let text = std::fs::read_to_string(Path::new(&home).join(".lightplayer/settings.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value["agent"]["openrouter_api_key"]
        .as_str()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_string)
}

/// Whether the agent ended its turn on a question to the user.
fn asks_a_question(text: &str) -> bool {
    let tail: String = text
        .trim_end()
        .chars()
        .rev()
        .take(300)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    tail.contains('?')
}

/// The model-facing transcript as eval steps: user text, assistant text,
/// tool calls and results; state blocks become `State` steps.
fn flatten(messages: &[lpa_agent::ChatMessage]) -> Vec<EvalStep> {
    let mut names = std::collections::BTreeMap::new();
    let mut steps = Vec::new();
    for message in messages {
        for block in &message.content {
            let from_user = message.role == ChatRole::User;
            match (from_user, block) {
                (true, ContentBlock::Text { text }) => {
                    if text.starts_with(lpa_agent::toolset::APP_STATE_OPEN) {
                        steps.push(EvalStep::State { text: text.clone() });
                    } else {
                        steps.push(EvalStep::User { text: text.clone() });
                    }
                }
                (false, ContentBlock::Text { text }) => {
                    steps.push(EvalStep::Assistant { text: text.clone() });
                }
                (_, ContentBlock::ToolUse { id, name, input }) => {
                    names.insert(id.clone(), name.clone());
                    steps.push(EvalStep::ToolCall {
                        name: name.clone(),
                        input: input.clone(),
                    });
                }
                (
                    _,
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    },
                ) => steps.push(EvalStep::ToolResult {
                    name: names.get(tool_use_id).cloned().unwrap_or_default(),
                    content: content.clone(),
                }),
                _ => {}
            }
        }
    }
    steps
}

/// A timer that is pending exactly once, then ready.
struct YieldOnce(bool);

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            Poll::Pending
        }
    }
}

struct NoopWake;
impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

/// One-turn-script provider (the e2e tests' pattern).
struct ScriptedProvider {
    turns: RefCell<VecDeque<Vec<TurnEvent>>>,
    requests: Rc<RefCell<Vec<TurnRequest>>>,
}

impl ModelProvider for ScriptedProvider {
    fn run_turn(&self, req: TurnRequest) -> BoxStream<'_, TurnEvent> {
        self.requests.borrow_mut().push(req);
        let events = self.turns.borrow_mut().pop_front().unwrap_or_default();
        Box::pin(futures_util::stream::iter(events))
    }
}
