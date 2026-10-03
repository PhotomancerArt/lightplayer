//! The device seat: the app chat seated on the device bench, on a scripted
//! or a live model (plan `m-agent-activity-corpus`, P3).
//!
//! [`AgentSeat`] is E4's seat, moved here from the journey tests so the
//! corpus can use it too: the agent runs exactly as in the product — the
//! app chat's run future, its tools, the host bridge's ops on the command
//! queue, the controller pressing offers as the tree has it at the press —
//! and the seat stands in for the actor's batch loop, applying what the run
//! queues and refreshing the readout after each batch. Only the model
//! (when scripted) and the board are fake.
//!
//! [`DeviceScenarioSeat`] wraps it as a corpus [`ScenarioSeat`]: it builds
//! the board a scenario starts with (blank, somebody else's firmware, or
//! LightPlayer running the start project, which the library holds and the
//! editor has open), and reads back what the board ended up running. Like
//! the shipped build, it also holds the tab's own runtimes ([`SeatSims`]):
//! a project opened from Home runs on a sim, as it does in a browser.
//!
//! The bench lives in this test module and is private to it, which is why
//! the seat lives beside it rather than beside the eval driver.

use std::collections::{BTreeMap, VecDeque};

use lpa_agent::provider::ReqwestTransport;
use lpa_agent::{ChatRole, ContentBlock, OpenAiCompatProvider, TurnEvent, TurnRequest};

use super::*;
use crate::app::agent::evals::app_agent_checks::NodeStatusRow;
use crate::app::agent::evals::app_agent_eval_driver::{
    ModelSource, PROGRESS_EVERY, ScriptedProvider,
};
use crate::app::agent::evals::app_agent_eval_harness::{EvalStudio, golden_tree};
use crate::app::agent::evals::app_agent_project_tree::ProjectTree;
use crate::app::agent::evals::app_agent_scenario::{BoardState, ForeignFirmware, Scenario};
use crate::app::agent::evals::app_agent_scenario_seat::{
    BoardRow, DeviceSummary, RunLimits, ScenarioSeat,
};
use crate::app::library::PackageProvenance;
use crate::app::studio::studio_view_channel::{CommandReceiver, command_channel};
use crate::{AgentController, AgentOp, ControllerId, SettingsCommand, StudioCommand};

/// The MAC every corpus board reports (its efuse, and the flash
/// preflight's — the bench's scripted preflight reads this one).
const CORPUS_BOARD_MAC: &str = "60:55:f9:0a:0b:0c";

/// The stamped uid of a board that was LightPlayer before the scenario.
const CORPUS_BOARD_UID: &str = "dev000000corpus0001";

/// What a WLED build with `WLED_DEBUG` prints at boot.
const WLED_BOOT_LINE: &str = "---WLED 0.15.0 2412100 INIT---";

/// How long a live run may wait, in real time, for a hung run to end after
/// it was told to stop.
const STOP_GRACE: Duration = Duration::from_secs(30);

/// The app chat seated on a [`DeviceBench`]'s controller.
pub(super) struct AgentSeat {
    /// Where the run's acts and feedback land (the actor's queue, here).
    rx: CommandReceiver,
    runs: Rc<RefCell<Vec<crate::AgentTaskFuture>>>,
    /// One turn script per run, consumed as runs start (scripted model).
    scripts: Rc<RefCell<VecDeque<Vec<Vec<TurnEvent>>>>>,
    /// Every request a scripted model received, in order.
    requests: Rc<RefCell<Vec<TurnRequest>>>,
    /// A live model: the seat paces the bench to wall-clock time while it
    /// thinks.
    live: bool,
}

impl AgentSeat {
    /// The seat on a scripted model whose runs are queued with
    /// [`Self::script`].
    pub(super) fn new(bench: &mut DeviceBench) -> Self {
        Self::with_source(bench, ModelSource::Scripted(Vec::new()))
    }

    /// The seat on `source`. A scripted source's runs are queued first;
    /// [`Self::script`] adds more.
    pub(super) fn with_source(bench: &mut DeviceBench, source: ModelSource) -> Self {
        let (tx, rx) = command_channel();
        let runs: Rc<RefCell<Vec<crate::AgentTaskFuture>>> = Rc::new(RefCell::new(Vec::new()));
        let scripts: Rc<RefCell<VecDeque<Vec<Vec<TurnEvent>>>>> =
            Rc::new(RefCell::new(VecDeque::new()));
        let requests: Rc<RefCell<Vec<TurnRequest>>> = Rc::new(RefCell::new(Vec::new()));
        let controller = &mut bench.controller;
        controller.set_agent_command_sender(tx);
        // The ack waits poll a timer between checks; one yield per wait
        // gives the seat a turn to apply what the run queued.
        controller.set_agent_timer(|_| Box::pin(YieldOnce::default()) as crate::AgentTimerFuture);
        let (model, api_key, live) = match &source {
            ModelSource::Live { model, api_key } => (model.clone(), api_key.clone(), true),
            ModelSource::Scripted(_) => (
                "scripted/model".to_string(),
                "sk-or-scripted".to_string(),
                false,
            ),
        };
        for command in [
            SettingsCommand::SetAgentProvider(Some(crate::AgentProvider::OpenRouter)),
            SettingsCommand::SetAgentOpenRouterApiKey(Some(api_key)),
            SettingsCommand::SetAppAgentModel(Some(model)),
        ] {
            controller.apply_settings_command(command);
        }
        controller.set_agent_spawner({
            let runs = Rc::clone(&runs);
            move |run| runs.borrow_mut().push(run)
        });
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
            ModelSource::Scripted(first) => {
                scripts.borrow_mut().extend(first);
                controller.set_agent_provider_factory({
                    let scripts = Rc::clone(&scripts);
                    let requests = Rc::clone(&requests);
                    move |_| {
                        let turns = scripts.borrow_mut().pop_front().unwrap_or_default();
                        Box::new(ScriptedProvider::new(turns, Rc::clone(&requests)))
                    }
                });
            }
        }
        Self {
            rx,
            runs,
            scripts,
            requests,
            live,
        }
    }

    /// Queue the turns of the next run.
    pub(super) fn script(&self, turns: Vec<Vec<TurnEvent>>) {
        self.scripts.borrow_mut().push_back(turns);
    }

    /// The user's message to the app chat; the run is driven to its end.
    pub(super) fn send(&mut self, bench: &mut DeviceBench, tasks: &TaskPool, text: &str) {
        self.press(bench, tasks, send_action(text));
    }

    /// The user's click (a card's button); any run it starts or resumes is
    /// driven to its end. A run that does not end inside the bench's
    /// wall-clock ceiling is a hang, and fails the test.
    pub(super) fn press(&mut self, bench: &mut DeviceBench, tasks: &TaskPool, action: UiAction) {
        let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
        if !self.drive(bench, tasks, action, None, deadline, false) {
            panic!(
                "the agent's run did not end; roster now: {:?}",
                bench.view()
            );
        }
    }

    /// [`Self::press`] under a scenario's limits: past one, the run is
    /// stopped the way Stop stops it. `false` when it did not end even so.
    ///
    /// What the press set moving (a chooser's port identifying, a board
    /// coming back) settles before the run it resumes takes its first
    /// look: a live model's own latency gives it that much, and a scripted
    /// one has none. (A design call of the corpus harness: the product
    /// resumes the run at once.)
    pub(super) fn press_within(
        &mut self,
        bench: &mut DeviceBench,
        tasks: &TaskPool,
        action: UiAction,
        limits: RunLimits,
    ) -> bool {
        self.drive(
            bench,
            tasks,
            action,
            Some(limits),
            limits.deadline + STOP_GRACE,
            true,
        )
    }

    fn drive(
        &mut self,
        bench: &mut DeviceBench,
        tasks: &TaskPool,
        action: UiAction,
        limits: Option<RunLimits>,
        give_up: std::time::Instant,
        quiet_first: bool,
    ) -> bool {
        drive(bench.controller.dispatch(action)).expect("the press dispatches");
        self.apply(bench);
        // A press that opened a chooser settles its card on the chooser's
        // answer, which a step folds; the run it resumes starts there.
        actor_step(bench, tasks);
        self.apply(bench);
        if quiet_first {
            wait_quiet(bench, tasks);
            self.apply(bench);
        }
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut next_progress = std::time::Instant::now() + PROGRESS_EVERY;
        loop {
            let next = self.runs.borrow_mut().pop();
            let Some(mut run) = next else {
                // An open the agent started that is still landing resumes
                // it when it does: the actor keeps ticking after a run
                // ends, and so does the seat, until the open has ended.
                if waiting_on_an_open(bench) {
                    if std::time::Instant::now() > give_up {
                        return false;
                    }
                    actor_step(bench, tasks);
                    self.apply(bench);
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                return true;
            };
            while run.as_mut().poll(&mut cx).is_pending() {
                self.apply(bench);
                actor_step(bench, tasks);
                if let Some(limits) = limits {
                    let session = bench.controller.agent_for_test().app_session();
                    let (usage, turns) = (session.mirror.usage, session.mirror.turn_stats.len());
                    if limits.passed(&usage, turns as u32) {
                        bench.controller.agent_for_test().request_app_stop();
                    }
                }
                if std::time::Instant::now() > give_up {
                    return false;
                }
                if self.live {
                    // The model thinks in real time: keep the bench's clock
                    // near it (as `run_until` does), so the board's own
                    // timers are not raced past what its server answers.
                    std::thread::sleep(Duration::from_millis(1));
                    if std::time::Instant::now() > next_progress {
                        next_progress = std::time::Instant::now() + PROGRESS_EVERY;
                        let session = bench.controller.agent_for_test().app_session();
                        eprintln!(
                            "app-agent-eval (device seat): … {} turns, {} out tokens, ${:.4}",
                            session.mirror.turn_stats.len(),
                            session.mirror.usage.output_tokens,
                            session.mirror.usage.reported_cost_usd().unwrap_or(0.0)
                        );
                    }
                }
            }
            self.apply(bench);
        }
    }

    /// What the actor does with a batch the run queued: its acts through
    /// the ordinary dispatch, its feedback in order, then the refreshed
    /// readout (`view_if_changed`).
    fn apply(&mut self, bench: &mut DeviceBench) {
        while self.rx.peek_any(|_| true) {
            for command in drive(self.rx.recv_coalesced()).unwrap_or_default() {
                match command {
                    StudioCommand::Action(action) => {
                        // As the actor does: a refused op is the agent's
                        // to hear (its ack carries the error) and a log line,
                        // never the end of the run — a live model may well
                        // try an edit before a project is open.
                        if let Err(error) = drive(bench.controller.dispatch(action)) {
                            bench.controller.note_action_error(&error);
                        }
                    }
                    StudioCommand::Agent(feedback) => {
                        bench.controller.apply_agent_feedback(feedback)
                    }
                    other => panic!("the app chat queued {other:?}"),
                }
            }
        }
        let _ = bench.controller.view_if_changed();
    }

    pub(super) fn requests(&self) -> usize {
        self.requests.borrow().len()
    }

    /// The last readout in request `index` (what the model saw that turn).
    pub(super) fn readout_of_request(&self, index: usize) -> String {
        let requests = self.requests.borrow();
        requests[index]
            .messages
            .iter()
            .filter(|message| message.role == ChatRole::User)
            .flat_map(|message| &message.content)
            .filter_map(|block| match block {
                ContentBlock::Text { text }
                    if text.starts_with(lpa_agent::toolset::APP_STATE_OPEN) =>
                {
                    Some(text.clone())
                }
                _ => None,
            })
            .last()
            .expect("every request carries the readout")
    }

    /// The newest user message in request `index` that is not the readout
    /// (a run's opening text: the user's message, or what a card did).
    pub(super) fn last_user_text(&self, index: usize) -> String {
        let requests = self.requests.borrow();
        requests[index]
            .messages
            .iter()
            .filter(|message| message.role == ChatRole::User)
            .flat_map(|message| &message.content)
            .filter_map(|block| match block {
                ContentBlock::Text { text }
                    if !text.starts_with(lpa_agent::toolset::APP_STATE_OPEN) =>
                {
                    Some(text.clone())
                }
                _ => None,
            })
            .last()
            .expect("a run opens with text")
    }

    /// The app chat's visible assistant turns, in order.
    pub(super) fn assistant_texts(&self, bench: &mut DeviceBench) -> Vec<String> {
        bench
            .controller
            .agent_for_test()
            .app_session()
            .mirror
            .turns
            .iter()
            .filter_map(|turn| match turn {
                crate::UiAgentTurn::Assistant { text } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// Every `act` result the model was handed, in order.
    pub(super) fn tool_results(&self, bench: &mut DeviceBench) -> Vec<serde_json::Value> {
        let session = bench.controller.agent_for_test().app_session();
        let runtime = session.runtime.borrow();
        let runtime = runtime.as_ref().expect("a run happened");
        runtime
            .transcript()
            .messages
            .iter()
            .flat_map(|message| &message.content)
            .filter_map(|block| match block {
                ContentBlock::ToolResult { content, .. } => serde_json::from_str(content).ok(),
                _ => None,
            })
            .collect()
    }

    /// The app chat's cards, in transcript order.
    pub(super) fn cards(&self, bench: &mut DeviceBench) -> Vec<crate::UiAgentCard> {
        app_cards(bench)
    }
}

/// Step the bench until no card is busy, every board has said what it
/// runs, and every pending link has settled — as a person waits for the
/// card to stop moving. Bounded by the bench's ceiling, never a hang: a
/// board that never settles is the checks' to report.
fn wait_quiet(bench: &mut DeviceBench, tasks: &TaskPool) {
    let deadline = std::time::Instant::now() + REAL_TIME_LIMIT;
    while std::time::Instant::now() < deadline {
        actor_step(bench, tasks);
        let view = bench.view();
        // An Offline card (a sim the last open powered off) will never say
        // what it runs; waiting on it would spend the whole ceiling.
        let quiet = view.devices.iter().all(|card| {
            card.status == lpa_devices::DeviceStatus::Offline
                || (card.activity.is_none()
                    && card.loaded_project != lpa_devices::view::LoadedProject::Unknown)
        }) && view.pending.iter().all(|pending| pending.needs_firmware())
            // An open waiting for its device (a sim starting) lands first.
            && bench.controller.pending_device_lens_for_test().is_none();
        if quiet {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Whether the agent pressed an open that has not ended yet (its sim is
/// still starting).
fn waiting_on_an_open(bench: &mut DeviceBench) -> bool {
    bench
        .controller
        .agent_for_test()
        .app_session()
        .open_wait
        .is_some()
}

/// One bench step, plus what the actor's tick does besides folding: an
/// open that was waiting for its device (a sim that has just said hello)
/// attaches its lens — `try_pending_device_lens`, which the actor runs on
/// every passive refresh. Without it an open from Home never finished.
fn actor_step(bench: &mut DeviceBench, tasks: &TaskPool) {
    bench.step(tasks);
    if bench.controller.pending_device_lens_for_test().is_some() {
        drive(bench.controller.try_pending_device_lens());
    }
}

/// The app chat's Send.
fn send_action(text: &str) -> UiAction {
    UiAction::from_op(
        ControllerId::new(AgentController::NODE_ID),
        AgentOp::AppSend {
            text: text.to_string(),
        },
    )
}

// ---------------------------------------------------------------------
// The tab's runtimes
// ---------------------------------------------------------------------

/// The sims a seat's tab powers on: each one a LightPlayer fake wearing the
/// identity Studio minted for it and its target's pin map, as `fw-browser`
/// boots in a worker.
///
/// Without it the seat had no runtime at all, and every open from Home —
/// `project/new`, `project/open`, which resolve to a Desktop sim — waited
/// forever for a device that could never start: the activity corpus's
/// S4, S18 and S19 all stalled there, the record minted for it sitting
/// Offline beside the board (2026-10-03).
#[derive(Default)]
struct SeatSims {
    /// One fake per minted sim, by uid: a power cycle reaches the same one.
    devices: RefCell<BTreeMap<String, FakeEsp32Device>>,
}

impl SimLinkSource for SeatSims {
    fn open(&self, session: &SimSession) -> Result<SimBacking, String> {
        let device = self
            .devices
            .borrow_mut()
            .entry(session.uid.clone())
            .or_insert_with(|| {
                let mut script = FakeDeviceScript::new(FakeBootState::LightPlayer(
                    FakeLightPlayerState::new()
                        .with_identity(FakeDeviceIdentity::new(&session.uid, &session.display_name))
                        .with_base_mac(&session.base_mac)
                        .with_heartbeat_interval(Duration::from_millis(20)),
                ));
                if let Some(manifest) = lpa_boards::runtime_manifest_json(&session.target) {
                    script = script.with_board_manifest(manifest);
                }
                FakeEsp32Device::new(script)
            })
            .clone();
        let info = crate::sim_link_info(&session.uid, &session.display_name);
        Ok(SimBacking {
            link: GrantedLink {
                link: Box::new(fake_device_link(info.clone(), &device)),
                info,
            },
            control: Rc::new(ScriptedSimControl {
                device,
                restarts: Rc::new(Cell::new(0)),
                manifests: Rc::new(RefCell::new(Vec::new())),
            }),
        })
    }
}

// ---------------------------------------------------------------------
// The corpus seat
// ---------------------------------------------------------------------

/// A corpus scenario's device seat: the bench, the fake board it starts
/// with, and the app chat seated on it.
pub(crate) struct DeviceScenarioSeat {
    bench: DeviceBench,
    tasks: TaskPool,
    device: FakeEsp32Device,
    seat: AgentSeat,
    /// The board the fake is: its pin map is the one the board's outputs
    /// open against, and the one the end project is judged on.
    board: String,
}

impl DeviceScenarioSeat {
    /// The bench with the board `scenario` starts with plugged in (granted
    /// when `connected`), and the app chat on `source`.
    pub(crate) fn new(source: ModelSource, scenario: &Scenario) -> Self {
        let board = scenario
            .board_id()
            .expect("a validated scenario names its board")
            .to_string();
        let start = &scenario.start.board;
        let state = start.state.expect("the device seat has a board state");
        let manifest = lpa_boards::runtime_manifest_json(&board)
            .expect("a validated scenario's board has a pin map");
        let boot = match state {
            BoardState::Blank => FakeBootState::BlankFlash,
            BoardState::Foreign => FakeBootState::ForeignFirmware,
            BoardState::Running | BoardState::Older => {
                let provenance = match state {
                    BoardState::Older => {
                        lpa_link::providers::fake_device::fake_provenance("fake-older-firmware")
                    }
                    _ => lpa_link::providers::fake_device::fake_provenance("fake-firmware"),
                };
                FakeBootState::LightPlayer(FakeLightPlayerState {
                    provenance,
                    ..FakeLightPlayerState::new()
                        .with_identity(FakeDeviceIdentity::new(CORPUS_BOARD_UID, "Venue lights"))
                        .with_base_mac(CORPUS_BOARD_MAC)
                        .with_heartbeat_interval(Duration::from_millis(20))
                })
            }
        };
        let mut script = FakeDeviceScript::new(boot)
            .with_flashed_heartbeat_interval(Duration::from_millis(20))
            .with_board_manifest(manifest);
        if scenario.chip().as_deref() == Some("esp32") {
            script = script.with_classic_esp32_rom();
        }
        if start.firmware == Some(ForeignFirmware::Wled) {
            script = script.with_foreign_banner(&[WLED_BOOT_LINE]);
        }
        let device = FakeEsp32Device::new(script);
        let running = matches!(state, BoardState::Running | BoardState::Older);
        let (mut bench, tasks) = match start.connected || running {
            true => DeviceBench::granted(&device, "usb-corpus"),
            false => DeviceBench::ungranted(&device, "usb-corpus"),
        };
        let sims = Rc::new(SimDeviceTransport::new(Rc::new(SeatSims::default())));
        bench.sims = Some(Rc::clone(&sims));
        bench.controller.set_device_sim_transport(sims);
        let seat = AgentSeat::with_source(&mut bench, source);
        Self {
            bench,
            tasks,
            device,
            seat,
            board,
        }
    }

    /// A running board's start: the start project goes into the library
    /// and onto the board through the card's own push, and the editor
    /// opens on the board — Jordan's lights as he got them.
    fn start_running(&mut self, golden: &str) {
        let (bench, tasks) = (&mut self.bench, &self.tasks);
        bench.run_until(tasks, "the board to identify", |bench| {
            bench
                .view()
                .devices
                .first()
                .is_some_and(|card| card.activity.is_none() && card.state_label == "Ready")
        });
        let files: Vec<(String, Vec<u8>)> = golden_tree(golden).files.into_iter().collect();
        let summary = bench
            .store
            .install_package("Venue lights", &files, PackageProvenance::Created, 2.0)
            .expect("the start project installs");
        bench.settle_library();
        let card = bench.view().devices[0].clone();
        bench.run_until(tasks, "the card to offer a push", |bench| {
            let offers = bench.controller.view().offers;
            offers
                .device_prefix(card.id)
                .is_some_and(|prefix| offers.get(&prefix.clone().child("push")).is_some())
        });
        bench.push_gesture(
            card.id,
            crate::PushSource::Library {
                project_uid: summary.uid.to_string(),
            },
        );
        bench.run_until(tasks, "the board to run the start project", |bench| {
            bench.view().devices.first().is_some_and(|card| {
                card.activity.is_none()
                    && matches!(
                        card.loaded_project,
                        lpa_devices::view::LoadedProject::Running { .. }
                    )
            })
        });
        let uid = bench.registry()[0].uid.clone();
        bench
            .open_lens(&uid)
            .expect("the editor opens on the running board");
        bench.run_until(tasks, "the editor to be ready on the board", |bench| {
            bench.ready_handle().is_some()
        });
    }

    /// The files of the project the board has loaded, read over the board's
    /// own wire (`None` when it runs nothing or will not say).
    fn board_project(&mut self) -> Option<ProjectTree> {
        let mut client =
            lpa_client::LpClient::new(FakeDeviceIo::new(&self.device)).on_borrowed_wire();
        let loaded = drive(client.project_list_loaded()).ok()?.into_value();
        let project = loaded.first()?.path.clone();
        let paths = drive(client.fs_list_dir(project.as_path(), true))
            .ok()?
            .into_value();
        let prefix = format!("{}/", project.as_str().trim_end_matches('/'));
        let mut files = Vec::new();
        for path in paths {
            let Some(relative) = path.as_str().strip_prefix(&prefix) else {
                continue;
            };
            if let Ok(bytes) = drive(client.fs_read(path.as_path())) {
                files.push((relative.to_string(), bytes.into_value()));
            }
        }
        Some(ProjectTree::from_files(files.into_iter().filter(
            |(path, _)| !path.starts_with(".lp/") && !path.is_empty(),
        )))
    }
}

impl ScenarioSeat for DeviceScenarioSeat {
    fn controller(&mut self) -> &mut StudioController {
        &mut self.bench.controller
    }

    fn start(&mut self, scenario: &Scenario) {
        let context = scenario.context_line();
        if !context.is_empty() {
            self.bench
                .controller
                .agent_for_test()
                .set_app_context_notes(vec![format!("context: {context}")]);
        }
        if let Some(golden) = scenario.start_golden() {
            self.start_running(golden);
        } else if scenario.start.board.connected {
            let (bench, tasks) = (&mut self.bench, &self.tasks);
            bench.run_until(tasks, "the board's verdict to settle", |bench| {
                bench
                    .view()
                    .pending
                    .first()
                    .is_some_and(|pending| pending.needs_firmware())
            });
        }
    }

    /// The person waits for the cards to stop moving before they type:
    /// a flash or a push the last turn started lands first.
    fn send(&mut self, text: &str, limits: RunLimits) {
        wait_quiet(&mut self.bench, &self.tasks);
        self.press(send_action(text), limits);
    }

    fn press(&mut self, action: UiAction, limits: RunLimits) {
        let (bench, tasks) = (&mut self.bench, &self.tasks);
        if !self.seat.press_within(bench, tasks, action, limits) {
            eprintln!("app-agent-eval (device seat): the run did not end after Stop");
        }
    }

    fn settle(&mut self) {
        wait_quiet(&mut self.bench, &self.tasks);
        for _ in 0..100 {
            actor_step(&mut self.bench, &self.tasks);
        }
    }

    fn saved_tree(&mut self) -> ProjectTree {
        match self.board_project() {
            Some(tree) => tree,
            // The board would not say: the last bytes Studio pushed.
            None => self
                .bench
                .pushed
                .borrow()
                .last()
                .map(|files| ProjectTree::from_files(files.clone()))
                .unwrap_or_default(),
        }
    }

    fn unsaved(&mut self) -> bool {
        crate::has_unsaved_work(&self.bench.controller.project_for_test().dirty_summary())
    }

    /// The board's project, settled on a server wearing the board's pin
    /// map: what the board's own engine would report.
    fn node_statuses(&mut self) -> Vec<NodeStatusRow> {
        let tree = self.saved_tree();
        if tree.files.is_empty() {
            return Vec::new();
        }
        let mut studio = EvalStudio::on_board(&tree, &self.board);
        studio.settle(6);
        studio.node_statuses()
    }

    /// The board the scenario is about: the tab's own runtimes (a sim an
    /// open from Home started) are not boards, and a running sim must never
    /// pass for the board running the project.
    fn device_summary(&mut self) -> Option<DeviceSummary> {
        let runtimes: Vec<String> = self
            .bench
            .registry()
            .into_iter()
            .filter(|row| {
                row.transport == crate::SIM_TRANSPORT || row.transport == crate::EMU_TRANSPORT
            })
            .map(|row| row.name)
            .collect();
        let mut view = self.bench.view();
        view.devices
            .retain(|card| !runtimes.iter().any(|name| *name == card.title));
        let flashed = self
            .bench
            .manifest_writes
            .borrow()
            .iter()
            .map(|json| {
                lpa_boards::RUNTIME_MANIFEST_SOURCES
                    .iter()
                    .find(|(_, source)| source == json)
                    .map(|(id, _)| id.to_string())
                    .unwrap_or_else(|| "(an unknown board manifest)".to_string())
            })
            .collect();
        Some(DeviceSummary {
            boards: view
                .devices
                .iter()
                .map(|card| BoardRow {
                    state: card.state_label.clone(),
                    loaded: match &card.loaded_project {
                        lpa_devices::view::LoadedProject::Running { label } => {
                            format!("running {label:?}")
                        }
                        other => format!("{other:?}").to_lowercase(),
                    },
                    running: matches!(
                        card.loaded_project,
                        lpa_devices::view::LoadedProject::Running { .. }
                    ),
                })
                .collect(),
            pending: view
                .pending
                .iter()
                .map(|pending| format!("{} ({:?})", pending.state_label, pending.firmware_face))
                .collect(),
            flashed,
            pushes: self.bench.pushed.borrow().len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use lpa_agent::{StopReason, TokenUsage};

    use super::*;

    /// The corpus's S4/S18/S19 stall: from Home the agent presses
    /// `project/new`, the open waits for its sim, and the agent ends its
    /// turn. When the editor comes up the agent is resumed with a note —
    /// no user message in between — and the edit it makes then applies to
    /// the new project.
    #[test]
    fn an_open_the_agent_started_resumes_it_when_the_editor_is_up() {
        let scenario = Scenario::load("s04-sean-new-c6-d5").expect("S4");
        let scripts = vec![
            vec![
                act_turn("n1", "project/new", &[("name", "Porch")]),
                say("Starting a project; once it's up I'll add a clock."),
            ],
            vec![
                call_turn(
                    "e1",
                    lpa_agent::EDIT_PROJECT_TOOL_NAME,
                    serde_json::json!({
                        "edits": [{ "create_node": { "kind": "Clock" } }],
                        "save": false,
                    }),
                ),
                say("Added a clock."),
            ],
        ];
        let mut seat = DeviceScenarioSeat::new(ModelSource::Scripted(scripts), &scenario);
        seat.start(&scenario);
        seat.send("make me a project with a clock", limits());

        let requests = seat.seat.requests();
        assert_eq!(requests, 4, "two turns in each of two runs");
        // The first run ended on Home with the open still under way.
        assert!(
            seat.seat.readout_of_request(1).contains("opening: \""),
            "{}",
            seat.seat.readout_of_request(1)
        );
        // The second run opens with the note, not a user message.
        assert_eq!(
            seat.seat.last_user_text(2),
            "[the project \"Porch\" you opened is now open in the editor]"
        );
        assert!(
            seat.seat
                .readout_of_request(2)
                .contains("page: project editor"),
            "{}",
            seat.seat.readout_of_request(2)
        );
        let session = seat.bench.controller.agent_for_test().app_session();
        let users: Vec<_> = session
            .mirror
            .turns
            .iter()
            .filter(|turn| matches!(turn, crate::UiAgentTurn::User { .. }))
            .collect();
        assert_eq!(users.len(), 1, "the user spoke once: {users:#?}");
        assert!(session.open_wait.is_none() && session.open_settled.is_none());
        let results = seat.seat.tool_results(&mut seat.bench);
        let edit = results.last().expect("the edit's result");
        assert_eq!(edit["results"][0]["ok"], true, "{edit:#}");
        assert!(
            seat.bench
                .controller
                .project_for_test()
                .agent_project_name()
                .contains("Porch")
        );
        assert_eq!(
            seat.seat
                .assistant_texts(&mut seat.bench)
                .last()
                .map(String::as_str),
            Some("Added a clock.")
        );
    }

    /// An open whose sim cannot start resumes the agent with the failure,
    /// once, instead of leaving it waiting on a page that never comes.
    #[test]
    fn an_open_the_agent_started_that_fails_resumes_it_with_the_reason() {
        let scenario = Scenario::load("s04-sean-new-c6-d5").expect("S4");
        let scripts = vec![
            vec![
                act_turn("n1", "project/new", &[("name", "Porch")]),
                say("Starting a project."),
            ],
            vec![say("The project's simulator would not start.")],
        ];
        let mut seat = DeviceScenarioSeat::new(ModelSource::Scripted(scripts), &scenario);
        let sims = Rc::new(SimDeviceTransport::new(Rc::new(RefusingSims)));
        seat.bench.sims = Some(Rc::clone(&sims));
        seat.bench.controller.set_device_sim_transport(sims);
        seat.start(&scenario);
        seat.send("make me a project", limits());

        assert_eq!(seat.seat.requests(), 3, "two turns, then the resumed one");
        let note = seat.seat.last_user_text(2);
        assert!(
            note.starts_with("[opening \"") && note.contains("failed: ") && note.ends_with(']'),
            "{note}"
        );
        assert!(
            note.contains("porch\" failed: ") && note.contains("did not start"),
            "{note}"
        );
        let session = seat.bench.controller.agent_for_test().app_session();
        assert!(session.open_wait.is_none() && session.open_settled.is_none());
    }

    /// The user spoke while the open was under way: the open owes the
    /// agent nothing — the run the user started reads the page itself.
    #[test]
    fn an_open_the_user_spoke_over_does_not_resume_the_agent() {
        let scenario = Scenario::load("s04-sean-new-c6-d5").expect("S4");
        let scripts = vec![
            vec![
                act_turn("n1", "project/new", &[("name", "Porch")]),
                say("Starting a project."),
            ],
            vec![say("Sure.")],
        ];
        let mut seat = DeviceScenarioSeat::new(ModelSource::Scripted(scripts), &scenario);
        seat.start(&scenario);
        // Drive only the first run: the open is still under way after it.
        let (bench, tasks) = (&mut seat.bench, &seat.tasks);
        drive(bench.controller.dispatch(send_action("make me a project"))).expect("sent");
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut run = seat.seat.runs.borrow_mut().pop().expect("a run");
        while run.as_mut().poll(&mut cx).is_pending() {
            seat.seat.apply(bench);
        }
        seat.seat.apply(bench);
        assert!(
            bench
                .controller
                .agent_for_test()
                .app_session()
                .open_wait
                .is_some(),
            "the open is still under way"
        );
        // The user's message, then the open lands: only the user's run.
        seat.seat.send(bench, tasks, "also make it blue");
        wait_quiet(bench, tasks);
        for _ in 0..20 {
            actor_step(bench, tasks);
            seat.seat.apply(bench);
        }
        assert!(bench.ready_handle().is_some(), "the editor is up");
        assert!(seat.seat.runs.borrow().is_empty(), "no resumed run");
        assert_eq!(seat.seat.requests(), 3);
        let session = bench.controller.agent_for_test().app_session();
        assert!(session.open_wait.is_none() && session.open_settled.is_none());
    }

    /// A tab whose runtimes never start.
    struct RefusingSims;

    impl SimLinkSource for RefusingSims {
        fn open(&self, _: &SimSession) -> Result<SimBacking, String> {
            Err("no engine in this test".to_string())
        }
    }

    fn limits() -> RunLimits {
        RunLimits {
            deadline: std::time::Instant::now() + Duration::from_secs(60),
            usd: 1.0,
            turns: 8,
            tokens: None,
        }
    }

    fn call_turn(id: &str, tool: &str, input: serde_json::Value) -> Vec<TurnEvent> {
        vec![
            TurnEvent::ToolUseStart {
                id: id.into(),
                name: tool.into(),
            },
            TurnEvent::ToolInputDelta {
                id: id.into(),
                json_fragment: input.to_string(),
            },
            turn_done(StopReason::ToolUse),
        ]
    }

    fn act_turn(id: &str, action: &str, args: &[(&str, &str)]) -> Vec<TurnEvent> {
        call_turn(
            id,
            lpa_agent::ACT_TOOL_NAME,
            serde_json::json!({
                "action": action,
                "args": args
                    .iter()
                    .map(|(name, value)| (name.to_string(), serde_json::json!(value)))
                    .collect::<serde_json::Map<_, _>>(),
                "why": "the user asked",
            }),
        )
    }

    fn say(text: &str) -> Vec<TurnEvent> {
        vec![
            TurnEvent::TextDelta(text.into()),
            turn_done(StopReason::EndTurn),
        ]
    }

    fn turn_done(stop_reason: StopReason) -> TurnEvent {
        TurnEvent::TurnDone {
            stop_reason,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..TokenUsage::default()
            },
        }
    }
}
