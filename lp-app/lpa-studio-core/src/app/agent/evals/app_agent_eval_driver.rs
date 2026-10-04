//! Driving the real app chat through a scenario: the loop
//! ([`drive_scenario`]) that plays the person's side — scripted replies,
//! the `otherwise` fallback, follow-ups, card clicks — against any
//! [`ScenarioSeat`], and the project seat itself ([`AgentEvalStudio`]): a
//! headless Studio with a library, over an in-process server wearing the
//! scenario's board's pin map, the app session on a live or scripted
//! model, interleaving the agent's run with Studio's command batches.
//!
//! The model is reached exactly as the product reaches it: settings select
//! OpenRouter, and the provider factory builds the OpenAI-compatible
//! provider from the settings' config — here over the host `reqwest`
//! transport instead of the browser's fetch.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, VecDeque};
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

use super::app_agent_check_spec::XIAO_C6_BOARD_ID;
use super::app_agent_checks::NodeStatusRow;
use super::app_agent_eval_harness::{NoTimer, board_server, golden_tree, no_timer};
use super::app_agent_project_tree::ProjectTree;
use super::app_agent_scenario::{CardDo, Scenario, StartProject};
use super::app_agent_scenario_seat::{DeviceSummary, RunLimits, ScenarioSeat};
use super::app_agent_transcript::{EvalStep, EvalTranscript};
use super::app_agent_user_side::{UserMove, UserSide, question_tail, questions_in};
use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
use crate::app::studio::offer_press_test_api::OfferPressTestApi;
use crate::app::studio::studio_edit_e2e_tests::{InProcessServerIo, drive};
use crate::app::studio::studio_view_channel::{CommandSender, StudioViewReceiver};
use crate::{
    AgentController, AgentOp, AgentProvider, AgentTaskFuture, ControllerId, HOME_NODE_ID, HomeOp,
    OfferArgs, SettingsCommand, StudioActor, StudioCommand, StudioController, StudioServerClient,
    UiAction, UiAgentStatus, UiAgentTurn, has_unsaved_work,
};

/// A hang guard for one scenario (network turns included). Not a gate:
/// a scenario that hits it is reported as stopped.
const SCENARIO_WALL_LIMIT: Duration = Duration::from_secs(20 * 60);

/// How often a long run prints where it is (live runs take minutes).
pub(crate) const PROGRESS_EVERY: Duration = Duration::from_secs(30);

/// How often the idle loop refreshes the project while the model thinks:
/// Studio's lens interval (`DEVICE_REFRESH_INTERVAL`, 150 ms), as the page
/// would. It used to be every 2 ms, and each refresh leaks ~24 KB in the
/// in-process engine's bump allocator (lpvm-wasm's wasmtime runtime, whose
/// `free` does nothing), so a long model wait faulted the root module
/// ("module mirror texture: alloc texture: OutOfMemory") — ticket
/// `_auto/03-impl/2026-10-03-module-mirror-texture-oom-in-evals.md`. While
/// the agent's own tool waits on the engine (its ack and settle waits) the
/// refresh stays prompt: those waits are bounded, and they count ticks.
const IDLE_REFRESH_EVERY: Duration = Duration::from_millis(150);

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
    pub(crate) statuses: Vec<NodeStatusRow>,
    /// The board's end state (device seat).
    pub(crate) device: Option<DeviceSummary>,
}

/// The project seat: a headless Studio with a memory library and the app
/// chat wired to `source`, over an in-process server wearing one board's
/// pin map.
pub(crate) struct AgentEvalStudio {
    actor: StudioActor<NoTimer>,
    tx: CommandSender,
    views: StudioViewReceiver,
    tasks: Rc<RefCell<Vec<AgentTaskFuture>>>,
    store: LibraryStore,
    requests: Rc<RefCell<Vec<TurnRequest>>>,
    /// Set when the agent's ack wait asks the timer for a tick: the run is
    /// waiting on the engine, not on the model.
    agent_waits: Rc<Cell<bool>>,
}

impl AgentEvalStudio {
    /// On the XIAO ESP32-C6 (Sean's board).
    pub(crate) fn new(source: ModelSource) -> Self {
        Self::on_board(source, XIAO_C6_BOARD_ID)
    }

    /// On `board` (a catalog id with a runtime pin map).
    pub(crate) fn on_board(source: ModelSource, board: &str) -> Self {
        let io = InProcessServerIo {
            server: Rc::new(RefCell::new(board_server(board))),
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
            SettingsCommand::SetAppAgentModel(Some(model)),
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
                    Box::new(ScriptedProvider::new(turns, Rc::clone(&log)))
                });
            }
        }

        let (mut actor, handle) = StudioActor::new(controller, no_timer as NoTimer);
        // The agent's ack waits poll a timer between checks; an instant
        // timer would spin its whole budget inside one poll of the run.
        // Yielding once per wait lets the driver run a batch in between.
        let agent_waits = Rc::new(Cell::new(false));
        let waits = Rc::clone(&agent_waits);
        actor
            .controller_mut_for_test()
            .agent_for_test()
            .set_timer(move |_| {
                waits.set(true);
                Box::pin(YieldOnce(false)) as crate::AgentTimerFuture
            });
        Self {
            actor,
            tx: handle.tx,
            views: handle.view,
            tasks,
            store,
            requests,
            agent_waits,
        }
    }

    /// Every request a scripted provider received (empty for a live one).
    pub(crate) fn scripted_requests(&self) -> Vec<TurnRequest> {
        self.requests.borrow().clone()
    }

    /// Put the scenario's start project in front of the user, and its
    /// context line in the agent's state block.
    pub(crate) fn start(&mut self, scenario: &Scenario) {
        self.open_start(&scenario.start.project);
        self.set_context(&scenario.context_line());
    }

    /// Open the start project.
    fn open_start(&mut self, project: &StartProject) {
        match project {
            StartProject::None => {
                panic!("the project seat starts with a project (Scenario::validate)")
            }
            StartProject::Blank => self.act(UiAction::from_op(
                ControllerId::new(HOME_NODE_ID),
                HomeOp::CreateProject {
                    template: crate::ProjectTemplate::Blank,
                    name: None,
                },
            )),
            StartProject::Golden(name) => {
                // A board-targeted package opens by waiting for that board,
                // and the eval has none: the in-process server IS the board
                // (it wears the board's pin map). So the package opens
                // untargeted on it, and the Hardware row's own op puts the
                // target back — the same open project the user would have.
                let mut tree = golden_tree(name);
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

    /// Start with no project open: the user is on Home.
    pub(crate) fn start_on_home(&mut self) {
        self.controller().show_home_for_test();
        // One batch hydrates the gallery's library inputs, as a page load
        // does before Home can offer anything from the library.
        self.command(StudioCommand::LibraryChanged);
        assert!(
            self.controller().view().home.is_some(),
            "no project open shows Home"
        );
    }

    /// Facts the readout cannot show (the scenario's board line).
    pub(crate) fn set_context(&mut self, context: &str) {
        let notes = match context.is_empty() {
            true => Vec::new(),
            false => vec![format!("context: {context}")],
        };
        self.controller()
            .agent_for_test()
            .set_app_context_notes(notes);
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

    /// Press a card's `action` the way the user's click does (the card's
    /// button, or the button it names), and drive any run the press
    /// resumes to its end. An offer is pressed by path through
    /// [`OfferPressTestApi`] instead.
    pub(crate) fn press_card(&mut self, action: UiAction, limits: RunLimits) {
        self.act(action);
        self.drive_runs(limits);
    }

    /// Every visible assistant turn's text, in order.
    pub(crate) fn assistant_texts(&mut self) -> Vec<String> {
        self.controller()
            .agent_for_test()
            .app_session()
            .mirror
            .turns
            .iter()
            .filter_map(|turn| match turn {
                UiAgentTurn::Assistant { text } => Some(text.clone()),
                _ => None,
            })
            .collect()
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

    pub(crate) fn node_statuses(&mut self) -> Vec<NodeStatusRow> {
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
        let mut next_progress = Instant::now() + PROGRESS_EVERY;
        let mut last_refresh = Instant::now();
        loop {
            let Some(mut task) = self.tasks.borrow_mut().pop() else {
                break;
            };
            loop {
                if let Poll::Ready(()) = Pin::as_mut(&mut task).poll(&mut cx) {
                    break;
                }
                let progressed = self.try_batch();
                let (usage, turns) = (self.usage(), self.turns());
                if limits.passed(&usage, turns) {
                    self.controller().agent_for_test().request_app_stop();
                }
                if !progressed {
                    // Idle: what the page's refresh timer would do — pull,
                    // so the engine ticks and statuses advance under a
                    // waiting agent. Paced while the model thinks
                    // ([`IDLE_REFRESH_EVERY`]); prompt while the agent's
                    // own tool waits on the engine.
                    let agent_waits = self.agent_waits.replace(false);
                    if agent_waits || last_refresh.elapsed() >= IDLE_REFRESH_EVERY {
                        self.tx
                            .send(crate::app::studio::studio_edit_e2e_tests::project_action(
                                crate::ProjectOp::RefreshProject,
                            ));
                        last_refresh = Instant::now();
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                if Instant::now() > next_progress {
                    next_progress = Instant::now() + PROGRESS_EVERY;
                    let (turns, usage, status) = (self.turns(), self.usage(), self.status());
                    let last = self
                        .controller()
                        .agent_for_test()
                        .app_session()
                        .mirror
                        .turns
                        .last()
                        .map(|turn| format!("{turn:?}").chars().take(160).collect::<String>())
                        .unwrap_or_default();
                    eprintln!(
                        "app-agent-eval: … {turns} turns, {} out tokens, ${:.4}, {status:?}; last: {last}",
                        usage.output_tokens,
                        usage.reported_cost_usd().unwrap_or(0.0)
                    );
                }
            }
            while self.try_batch() {}
        }
    }
}

/// The eval studio presses offers through its command queue, as the
/// web's click does; a run the press resumes is driven by the caller.
impl OfferPressTestApi for AgentEvalStudio {
    type Outcome = ();

    fn offer_tree(&mut self) -> crate::UiOfferTree {
        self.controller().view().offers
    }

    fn dispatch_press(&mut self, action: UiAction) {
        self.act(action);
    }
}

/// The project seat is a [`ScenarioSeat`]: the open project is the
/// library's saved copy, and its statuses are the in-process server's.
impl ScenarioSeat for AgentEvalStudio {
    fn controller(&mut self) -> &mut StudioController {
        self.actor.controller_mut_for_test()
    }

    fn start(&mut self, scenario: &Scenario) {
        AgentEvalStudio::start(self, scenario);
    }

    fn send(&mut self, text: &str, limits: RunLimits) {
        AgentEvalStudio::send(self, text, limits);
    }

    fn press(&mut self, action: UiAction, limits: RunLimits) {
        self.press_card(action, limits);
    }

    fn settle(&mut self) {
        AgentEvalStudio::settle(self, 6);
    }

    fn saved_tree(&mut self) -> ProjectTree {
        AgentEvalStudio::saved_tree(self)
    }

    fn unsaved(&mut self) -> bool {
        AgentEvalStudio::unsaved(self)
    }

    fn node_statuses(&mut self) -> Vec<NodeStatusRow> {
        AgentEvalStudio::node_statuses(self)
    }

    fn device_summary(&mut self) -> Option<DeviceSummary> {
        None
    }
}

/// What the driver does next.
enum Next {
    Send(String),
    Press(UiAction),
}

/// Run one scenario on the real app chat, in `seat`: the person's opening
/// message, then their side of the conversation — scripted replies, the
/// `otherwise` fallback, follow-ups, card clicks — until they have nothing
/// more to say or a limit stops it.
pub(crate) fn drive_scenario<S: ScenarioSeat>(seat: &mut S, scenario: &Scenario) -> DrivenRun {
    let budget = scenario.budget;
    let limits = RunLimits {
        deadline: Instant::now() + SCENARIO_WALL_LIMIT,
        usd: budget.usd,
        turns: budget.turns,
        tokens: budget.tokens,
    };
    seat.start(scenario);
    let mut user = UserSide::new(&scenario.user);
    let mut steps = Vec::new();
    let mut seen = Seen {
        transcript: seat.transcript_steps().len(),
        notices: seat.notices().len(),
        cards: BTreeSet::new(),
    };
    let mut decided: BTreeSet<String> = BTreeSet::new();
    let mut next = Next::Send(scenario.user.say.clone());
    let mut stop: Option<String> = None;
    'conversation: loop {
        if seat.turns() >= budget.turns {
            stop = Some(format!("turn budget ({}) spent", budget.turns));
            break;
        }
        match next {
            Next::Send(text) => seat.send(&text, limits),
            Next::Press(action) => seat.press(action, limits),
        }
        seen.collect(seat, &mut steps);
        if let UiAgentStatus::Error { message, .. } = seat.status() {
            stop = Some(format!("provider error: {message}"));
            break;
        }
        if Instant::now() > limits.deadline {
            stop = Some("the scenario's wall-clock hang guard fired".to_string());
            break;
        }
        let usage = seat.usage();
        if limits.passed(&usage, seat.turns()) {
            stop = Some(format!(
                "budget spent (${:.4}, {} tokens, {} turns)",
                usage.reported_cost_usd().unwrap_or(0.0),
                super::app_agent_scenario_seat::total_tokens(&usage),
                seat.turns()
            ));
            break;
        }

        // Cards first: a click resumes the agent's run.
        let pending: Vec<crate::UiAgentCard> = seat
            .cards()
            .into_iter()
            .filter(|card| card.is_pending() && !decided.contains(&card.id))
            .collect();
        for card in pending {
            decided.insert(card.id.clone());
            let offer = card.offer.as_ref().map(ToString::to_string);
            let choice = user.card_rule(offer.as_deref());
            match choice.action {
                CardDo::Leave => steps.push(EvalStep::CardLeft {
                    card: card.id.clone(),
                    offer,
                }),
                CardDo::Click => {
                    let args = choice
                        .args
                        .iter()
                        .fold(OfferArgs::new(), |args, (name, value)| {
                            args.with(name, value)
                        });
                    match seat.card_click(&card, &args) {
                        Ok(action) => {
                            steps.push(EvalStep::CardClicked {
                                card: card.id.clone(),
                                offer,
                                args: choice.args,
                            });
                            next = Next::Press(action);
                            continue 'conversation;
                        }
                        Err(reason) => {
                            stop = Some(format!(
                                "the person could not click card {}: {reason}",
                                card.id
                            ));
                            break 'conversation;
                        }
                    }
                }
            }
        }

        let said = seat.last_assistant_text();
        let question = |said: &str| EvalStep::Question {
            tail: question_tail(said),
            questions: questions_in(said),
        };
        match user.after_turn(&said) {
            UserMove::Reply { replies, text } => {
                steps.push(question(&said));
                for (about, text) in replies {
                    steps.push(EvalStep::ScriptedReply { about, text });
                }
                next = Next::Send(text);
            }
            UserMove::Fallback {
                question: tail,
                text,
            } => {
                steps.push(question(&said));
                steps.push(EvalStep::FallbackReply {
                    question: tail,
                    text: text.clone(),
                });
                next = Next::Send(text);
            }
            UserMove::FollowUp { text } => {
                steps.push(EvalStep::FollowUp { text: text.clone() });
                next = Next::Send(text);
            }
            UserMove::Stop { reason } => {
                steps.push(question(&said));
                stop = Some(reason);
                break;
            }
            UserMove::Done => break,
        }
    }
    if let Some(reason) = stop {
        steps.push(EvalStep::Stopped { reason });
    }
    seat.settle();
    seen.collect(seat, &mut steps);
    DrivenRun {
        project: seat.saved_tree(),
        transcript: EvalTranscript { steps },
        unsaved: seat.unsaved(),
        usage: seat.usage(),
        turns: seat.turns(),
        statuses: seat.node_statuses(),
        device: seat.device_summary(),
    }
}

/// How much of the seat's transcript, notices and cards the run's steps
/// already hold.
struct Seen {
    transcript: usize,
    notices: usize,
    cards: BTreeSet<String>,
}

impl Seen {
    /// Append what is new since the last look: the model-facing
    /// transcript, the chat's notices, and each card the first time it
    /// appears.
    fn collect<S: ScenarioSeat>(&mut self, seat: &mut S, steps: &mut Vec<EvalStep>) {
        let transcript = seat.transcript_steps();
        steps.extend(transcript.iter().skip(self.transcript).cloned());
        self.transcript = self.transcript.max(transcript.len());
        let notices = seat.notices();
        steps.extend(
            notices
                .iter()
                .skip(self.notices)
                .map(|text| EvalStep::Notice { text: text.clone() }),
        );
        self.notices = self.notices.max(notices.len());
        for card in seat.cards() {
            if self.cards.insert(card.id.clone()) {
                steps.push(EvalStep::CardHanded {
                    card: card.id.clone(),
                    offer: card.offer.as_ref().map(ToString::to_string),
                    title: card.title.clone(),
                    destructive: card.destructive,
                });
            }
        }
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

/// The model-facing transcript as eval steps: user text, assistant text,
/// tool calls and results; state blocks become `State` steps.
pub(crate) fn flatten(messages: &[lpa_agent::ChatMessage]) -> Vec<EvalStep> {
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

/// One-turn-script provider (the e2e tests' pattern): each `run_turn`
/// answers with the next scripted turn and logs the request it was sent.
/// The device journey eval (E4) seats it on the device bench too.
pub(crate) struct ScriptedProvider {
    turns: RefCell<VecDeque<Vec<TurnEvent>>>,
    requests: Rc<RefCell<Vec<TurnRequest>>>,
}

impl ScriptedProvider {
    pub(crate) fn new(turns: Vec<Vec<TurnEvent>>, requests: Rc<RefCell<Vec<TurnRequest>>>) -> Self {
        Self {
            turns: RefCell::new(turns.into()),
            requests,
        }
    }
}

impl ModelProvider for ScriptedProvider {
    fn run_turn(&self, req: TurnRequest) -> BoxStream<'_, TurnEvent> {
        self.requests.borrow_mut().push(req);
        let events = self.turns.borrow_mut().pop_front().unwrap_or_default();
        Box::pin(futures_util::stream::iter(events))
    }
}
