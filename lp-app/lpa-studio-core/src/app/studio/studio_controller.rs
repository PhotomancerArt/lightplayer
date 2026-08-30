//! The studio's top controller: one dispatch surface over the runtime
//! pool, the project mirror, the device flows and the library.
//!
//! # The single-session web policy
//!
//! Pool capacity is a POLICY, not a shape (ADR
//! `2026-08-03-studio-runs-n-device-sessions`): [`RuntimePool`] admits a
//! sim and several boards at once, and the app on top decides how many
//! it actually runs. The WEB app runs exactly ONE — a sim or a board,
//! never both, one per browser tab — so that decision lives here, at
//! [`StudioController::install_session`] and the sim-reuse open, and
//! never in the pool (a desktop shell with real session wayfinding is
//! meant to inherit the N-session shape unchanged).
//!
//! The rule: opening a project or connecting a board tears the tab's
//! other session down first, and is refused only while an operation is
//! in flight — a flash or a deploy is the one thing teardown cannot end
//! honestly, so the refusal names it instead. Recorded in the
//! forthcoming session·project-control ADR (single-session web policy +
//! studio-or-site navigation).

use core::future::Future;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::{CancelSignal, ProgressDeadline};
use lpa_link::{
    DeviceState, LinkManagementRequest, LinkManagementResult, LinkProvider, LinkProviderKind,
};

use crate::app::device::device_event_adapter::{management_event_sink, probe_event_sink};
use crate::app::device::link_ux::management_result_logs;
use crate::app::device::{DEPLOY_NODE_ID, DeployOp, DeployTarget, DeviceOpenOutcome};
use crate::app::home::home_view_builder::HomeInputs;
use crate::app::home::{HOME_NODE_ID, HomeOp, UiHomeView, home_view_builder};
use crate::app::library::{CatalogOp, LibraryHost};
use crate::app::places::device_session::{self, DeviceContent, DeviceSyncState};
use crate::app::studio::console_command::ConsoleCommand;
use crate::app::studio::refresh_cadence::RefreshCadence;
use crate::app::studio::ui_console_view::UiConsoleView;
use crate::core::log::{
    DeviceEventKind, DeviceEventLog, DeviceEventRecord, DeviceEventRecorder, LogClock, LogFilter,
    LogRing,
};
use crate::core::notice::UiNotices;
use crate::{
    AssetContentFetchOp, AssetEditOp, ConnectFlowState, Controller, ControllerContext,
    DeviceController, DeviceOp, ModuleExportOp, NodeClearDebugOp, NodeCopyOp, NodeCreateOp,
    NodeImportOp, NodePasteOp, NodeRemoveOp, NodeRevertOp, PanelAutoSaveOp, PanelClearOp,
    PanelWriteOp, PatchPulseOp, PlaylistActivateOp, ProjectConnectResult, ProjectController,
    ProjectEditRun, ProjectOp, ProjectRefreshOutcome, ProjectState, ProjectSyncRun, RuntimePayload,
    RuntimePool, ServerFailureKind, ServerSnapshot, ServerState, SlotEditOp, StudioSnapshot,
    UiAction, UiActions, UiActivityView, UiError, UiLogDraft, UiLogEntry, UiLogLevel, UiLogOrigin,
    UiNotice, UiResult, UiStatus, UiStudioView, UiViewContent, UxActivityTarget, UxUpdate,
    UxUpdateSink,
};

/// How often the quiet PortHeld retry re-attempts the granted attach
/// (D32: "quiet periodic retry" — calm enough to never fight the other
/// holder, fast enough that closing the other tab feels responsive).
const PORT_HELD_RETRY_SECS: f64 = 5.0;

/// Minimum gap between view publishes that carry *only* streamed log lines
/// (session console tails, drained producer batches). Anything structural —
/// a revision advance or a local mutation — publishes immediately and takes
/// pending lines along; this throttle only bounds how often a bare log
/// stream can force a full-view rebuild.
const LOG_ONLY_PUBLISH_MIN_GAP_SECS: f64 = 0.25;

/// Flap guard for the sim crash auto-reboot: one reboot per window. A
/// crash landing within this many seconds of the previous auto-reboot
/// means the loaded project crashes the worker itself — rebooting again
/// would loop, so the session stays Failed for manual restart.
const SIM_CRASH_REBOOT_GUARD_SECS: f64 = 30.0;

/// Whether a fresh sim crash at `now` may auto-reboot, given when the
/// previous auto-reboot ran (`None` = never; epoch seconds).
fn sim_crash_reboot_allowed(last_reboot_at: Option<f64>, now: f64, guard_secs: f64) -> bool {
    match last_reboot_at {
        None => true,
        Some(last) => now - last >= guard_secs,
    }
}

/// Close a payload the pool never took (a refused install — capacity or
/// the single-session policy): the provider already minted a live
/// session for it, so refusing without closing would leak a worker or
/// hold a serial port the user cannot see.
async fn close_runtime_payload(payload: crate::RuntimePayload) {
    match payload {
        crate::RuntimePayload::Sim(sim) => {
            let _ = sim.connector.close(&sim.session.id).await;
        }
        crate::RuntimePayload::Device(handle) => {
            let _ = handle.close().await;
        }
    }
}

pub struct StudioController {
    device: DeviceController,
    /// The runtime sessions the studio is attached to, plus the editor
    /// lens. The pool ADMITS a sim and several boards at once (P2 of the
    /// runtime-pool milestone); what this app actually runs in one is
    /// the single-session policy above. Every network op resolves its
    /// wire client through one of the pool's two named seams (lens-bound
    /// editor ops vs device-targeted deploy/reconcile ops).
    pool: RuntimePool,
    /// Test-only escape from the single-session policy (module doc).
    ///
    /// The POOL still runs N sessions and the model still supports them —
    /// two boards attached at once (the multi-device roadmap's substitute
    /// for physically plugging in two boards), a board beside an open sim
    /// project — so the tests that prove that need a controller that
    /// admits them. Nothing in production clears the policy: the browser
    /// app is the only shell today, and it runs one session per tab.
    #[cfg(test)]
    multi_session_for_test: bool,
    project: ProjectController,
    /// Platform timer factory for the INITIAL project sync's progress
    /// deadline, installed by `StudioActor::new` exactly like the agent
    /// timer below. Absent (tests without an actor) the sync runs ungated —
    /// the pre-2026-08-26 behavior. See [`Self::sync_project_after_attach`].
    sync_timer: Option<crate::AgentTimerFactory>,
    /// Bounded, chronological log buffer. Capped in core (P3/Q5) rather than in
    /// the web crate's retired 80-entry mirror.
    logs: LogRing,
    /// The console's display filter (min level + origin toggles), mutated by
    /// [`ConsoleCommand`]s. Display-side only: the ring keeps everything, the
    /// filter shapes the emitted [`UiConsoleView`].
    log_filter: LogFilter,
    /// The device lifecycle event ring (M0 of the multi-device roadmap):
    /// state transitions, connect-flow changes, pool lifecycle, management
    /// phases, sweep decisions, parse anomalies — and raw RX/TX in capture
    /// mode. `Rc` because the device controller and per-connect event
    /// sinks record into it through [`DeviceEventRecorder`] clones.
    device_events: Rc<std::cell::RefCell<DeviceEventLog>>,
    /// The injected wall clock that stamps [`UiLogDraft`]s at push time.
    /// Producers never stamp — see the `core::log` module docs.
    now_secs: LogClock,
    /// The mutable cell behind `now_secs` when built via the test
    /// constructors, so pacing tests can advance time past a completion
    /// gap (`advance_clock_for_test`).
    #[cfg(test)]
    test_clock: Option<std::rc::Rc<std::cell::Cell<f64>>>,
    /// Optional per-entry mirror hook, invoked for **every** stamped entry as
    /// it enters the ring — independent of the display filter (which only
    /// shapes the emitted console view). The web shell installs its JS-console
    /// mirror here (P4), making ring entry the single mirroring point.
    on_entry: Option<Box<dyn Fn(&UiLogEntry)>>,
    /// The project revision reflected in the last emitted view. `view()` is
    /// change-gated via [`Self::view_if_changed`]: a snapshot is only rebuilt
    /// and emitted when an applied read advanced this revision or a local
    /// mutation set [`Self::dirty`].
    applied_revision: Option<i64>,
    /// Set when local (non-network) state changed since the last emitted view —
    /// a dispatched action, focus change, or an action-outcome log — so the
    /// next gate emits even though the project revision did not move.
    dirty: bool,
    /// Set when only *streamed* log lines arrived (session console tails,
    /// drained producer batches) since the last emitted view. Kept separate
    /// from [`Self::dirty`] and published on a throttle
    /// ([`LOG_ONLY_PUBLISH_MIN_GAP_SECS`]) so a per-line stream cannot force
    /// full-view rebuilds at stream rate.
    logs_dirty: bool,
    /// `now_secs` at the last log-only publish (throttle anchor).
    last_log_only_publish: f64,
    /// The injected library host (M4b): catalog transactions, project
    /// open/close, and gallery snapshots all go through this seam. Also
    /// held by the project flows.
    library_host: Option<Rc<dyn LibraryHost>>,
    /// Cached gallery inputs, hydrated from a host catalog snapshot by
    /// [`Self::refresh_library`] — `view()` never reads a live store.
    home_inputs: Option<HomeInputs>,
    /// A library re-hydration is due (attach, home op, save, close, or a
    /// cross-tab `LibraryChanged` ping). Drained at the end of every
    /// dispatch and by the actor after each batch.
    library_refresh_pending: bool,
    /// A home-card open in flight: keeps the gallery on screen (card busy)
    /// while the simulator opens, and tells the connect flow which package
    /// to push instead of probing running projects.
    pending_open: Option<PendingOpen>,
    /// When the next quiet PortHeld retry is due (D32; `None` while no
    /// port is held). Epoch seconds on the injected clock.
    port_held_retry_at: Option<f64>,
    /// Per-card UI view-state (selected tab, open sheet), keyed by the
    /// card's `identity_key()`. Core-owned so it survives the card ⇄ pane
    /// growth and session replaces, and is e2e-drivable (2026-07-25
    /// re-home). Pruned lazily: absent keys default; a stale key just
    /// never re-reads. The in-place `op` is NOT stored here — it derives
    /// live from the session's `operation_label`.
    card_ui: std::collections::HashMap<String, crate::CardUiState>,
    /// The CARD-OWNED op flows in flight (state-flow model §2), one per
    /// managed SESSION: set at management dispatch, fed by the manage
    /// event sink, and — the point (I1) — NOT cleared when the session
    /// dies, because heavy ops sever the very session that used to
    /// narrate them. `Failed` stays until the user takes its one exit
    /// (`CardUiOp::ClearOp`).
    ///
    /// Keyed by `RuntimeId` (M4), not by the card key: a first-provision
    /// flash STAMPS a uid mid-op, which moves the card's
    /// `identity_key()` from its session key to that uid — an op keyed by
    /// the card key would lose its card at the instant the flash
    /// succeeded. The session id does not move.
    ///
    /// The one thing a session id does not survive is the replug that
    /// ends a recovery write: the board comes back on the same ENDPOINT
    /// under a new session. [`Self::migrate_card_op`] carries the flow
    /// across, because the endpoint is the physical board's continuity.
    device_card_ops: std::collections::BTreeMap<crate::RuntimeId, Rc<RefCell<crate::CardOp>>>,
    /// The most recent finished device filesystem backup, carried on the
    /// home view until the shell downloads it. Kept (rather than emitted and
    /// forgotten) because the view is a full snapshot; `device_backup_seq` is
    /// what stops a re-render from re-downloading it.
    device_backup: Option<crate::UiDeviceBackup>,
    /// Session-monotonic backup counter. Never reset — the shell compares it
    /// against the last one it acted on.
    device_backup_seq: u64,
    /// When the last sim crash auto-reboot ran (`None` = never). Epoch
    /// seconds on the injected clock; the flap guard: a second crash
    /// within [`SIM_CRASH_REBOOT_GUARD_SECS`] stays Failed for manual
    /// restart instead of reboot-looping a crashing project.
    sim_crash_reboot_at: Option<f64>,
    /// Injected randomness for identity minting (`dev` uids). The web
    /// shell installs crypto randomness at startup; the default is a
    /// clock-derived fallback good enough for tests.
    random: Rc<dyn Fn() -> [u8; 16]>,
    /// The LOCAL `YYYY-MM-DD-HHMM` stamp the library dates slugs with —
    /// the same string the setup flow derives a device name's date from
    /// (`derive_device_name`, design §3). Injected for the same reason as
    /// the clock: core reads neither time nor timezone. The default
    /// derives UTC from `now_secs`, which is honest in tests and one
    /// timezone off in a shell that forgets to install its own.
    local_stamp: Rc<dyn Fn() -> String>,
    /// The one open setup wizard (flow design F5b: the wizard is a card).
    /// One at a time — two would be two flows racing for one serial port.
    setup: Option<crate::SetupSession>,
    /// The device session the open setup flow is driving, once a port
    /// grant produced one. Kept beside the session because the card key
    /// the executor addresses is derived from it.
    setup_device: Option<crate::RuntimeId>,
    /// The device sessions that existed when the flow asked for its port.
    /// `open_provider` is a long await (chooser, open, reset, boot wait)
    /// that emits renders while the new session installs — before
    /// `setup_device` can bind. Any session NOT in this snapshot is the
    /// flow's own and stands down with it (the connect-window card flash,
    /// G2 2026-08-05). Cleared when the port request settles.
    setup_port_snapshot: Option<Vec<crate::RuntimeId>>,
    /// The layered settings store (user > host > baked defaults). Pure
    /// state: the platform edges load its layers and persist the user
    /// layer via [`Self::set_on_user_settings`].
    settings: crate::app::settings::SettingsStore,
    /// Persistence hook for the user settings layer, invoked with the
    /// layer's JSON whenever a user-driven settings mutation changes it.
    /// The web shell installs a localStorage writer; an Electron shell
    /// would install its own sink. Layer *loads* never fire it.
    on_user_settings: Option<Box<dyn Fn(&str)>>,
    /// Clipboard sink for node copy, installed by the shell (the web app
    /// wires `clipboard::write_text`). Core never touches the clipboard:
    /// it produces envelope text and hands it here.
    on_copy_text: Option<Box<dyn Fn(&str)>>,
    /// The shader agent's per-node chat sessions (P5). Pure state plus
    /// injected platform facilities (spawner, provider factory); runs are
    /// spawned tasks reporting back through the actor's command queue.
    agent: crate::AgentController,
}

/// What one session's identity resolution produced (device identity
/// design §3): the identity itself, plus whether the registry already
/// remembers this board — the difference between a sighting worth
/// recording and a stranger that registers nothing.
#[derive(Clone, Debug)]
struct SessionIdentity {
    identity: crate::app::places::DeviceIdentity,
    /// A registry row exists under the resolved uid.
    registered: bool,
}

/// What a home card asked to open.
#[derive(Clone, Debug)]
enum PendingOpen {
    /// A library package, by key (`prj…` uid or slug).
    Package(String),
    /// An embedded example, by id (opened as a transient view session).
    Example(String),
    /// A fetched View-access shared project (P5): the bytes ride the
    /// pending open so a cold link can boot the sim first.
    SharedTransient {
        uid: String,
        name: String,
        package_files: Vec<(String, Vec<u8>)>,
        history_files: Vec<(String, Vec<u8>)>,
    },
}

impl PendingOpen {
    /// The card key the gallery marks busy.
    fn card_key(&self) -> &str {
        match self {
            PendingOpen::Package(key) => key,
            PendingOpen::Example(id) => id,
            PendingOpen::SharedTransient { uid, .. } => uid,
        }
    }

    /// The action that runs this open again — what the failed opening
    /// frame's Retry dispatches. Built from the pending open itself so
    /// Retry can never drift from what was actually attempted.
    fn retry_action(&self) -> UiAction {
        let op = match self {
            PendingOpen::Package(key) => HomeOp::OpenPackage { key: key.clone() },
            PendingOpen::Example(id) => HomeOp::OpenExample { id: id.clone() },
            PendingOpen::SharedTransient {
                uid,
                name,
                package_files,
                history_files,
            } => HomeOp::OpenSharedTransient {
                uid: uid.clone(),
                name: name.clone(),
                package_files: package_files.clone(),
                history_files: history_files.clone(),
            },
        };
        UiAction::from_op(crate::ControllerId::new(HOME_NODE_ID), op)
    }
}

impl StudioController {
    /// Create a controller with the platform's wall clock.
    ///
    /// `now_secs` returns seconds since the Unix epoch as `f64`; the web
    /// shell passes `|| js_sys::Date::now() / 1000.0`, tests pass fixed or
    /// stepping fakes. Core takes the closure instead of reading a clock so
    /// the crate stays platform-free (P1).
    pub fn new(now_secs: impl Fn() -> f64 + 'static) -> Self {
        let now_secs: LogClock = Rc::new(now_secs);
        let now_secs_for_stamp = Rc::clone(&now_secs);
        let device_events = Rc::new(std::cell::RefCell::new(DeviceEventLog::new()));
        let mut device = DeviceController::new();
        device.set_event_recorder(DeviceEventRecorder::new(
            Rc::clone(&device_events),
            Rc::clone(&now_secs),
        ));
        Self {
            device,
            pool: RuntimePool::new(),
            #[cfg(test)]
            multi_session_for_test: false,
            project: ProjectController::new(),
            sync_timer: None,
            logs: LogRing::new(),
            log_filter: LogFilter::default(),
            device_events,
            now_secs,
            #[cfg(test)]
            test_clock: None,
            on_entry: None,
            applied_revision: None,
            // The first view is always new to the UI, so start dirty.
            dirty: true,
            logs_dirty: false,
            last_log_only_publish: f64::NEG_INFINITY,
            library_host: None,
            home_inputs: None,
            library_refresh_pending: false,
            pending_open: None,
            port_held_retry_at: None,
            card_ui: std::collections::HashMap::new(),
            device_card_ops: std::collections::BTreeMap::new(),
            device_backup: None,
            device_backup_seq: 0,
            sim_crash_reboot_at: None,
            random: Rc::new(clock_fallback_random),
            local_stamp: {
                let clock = Rc::clone(&now_secs_for_stamp);
                Rc::new(move || utc_slug_stamp(clock()))
            },
            setup: None,
            setup_device: None,
            setup_port_snapshot: None,
            settings: crate::app::settings::SettingsStore::default(),
            on_user_settings: None,
            on_copy_text: None,
            agent: crate::AgentController::new(),
        }
    }

    /// Install the platform spawner for agent run futures (the web shell
    /// passes `spawn_local`). Install before the actor takes ownership.
    pub fn set_agent_spawner(&mut self, spawner: impl Fn(crate::AgentTaskFuture) + 'static) {
        self.agent.set_spawner(spawner);
    }

    /// Install the agent's model-provider factory (the web shell matches
    /// the config variant and builds the corresponding provider over the
    /// browser fetch transport; tests inject scripted fakes). Install
    /// before the actor takes ownership.
    pub fn set_agent_provider_factory(
        &mut self,
        factory: impl Fn(&crate::AgentProviderConfig) -> Box<dyn lpa_agent::ModelProvider> + 'static,
    ) {
        self.agent.set_provider_factory(factory);
    }

    /// Install the agent's model-list fetcher (P8: the web shell wraps
    /// `lpa_agent::list_*_models` over the browser fetch transport; tests
    /// inject scripted fakes). Install before the actor takes ownership.
    pub fn set_agent_models_fetcher(
        &mut self,
        fetcher: impl Fn(&crate::AgentProviderConfig) -> crate::AgentModelsFetchFuture + 'static,
    ) {
        self.agent.set_models_fetcher(fetcher);
    }

    /// Hand the agent sub-controller the actor's command sender (called by
    /// `StudioActor::new`); run futures report progress through it.
    pub(crate) fn set_agent_command_sender(
        &mut self,
        tx: crate::app::studio::studio_view_channel::CommandSender,
    ) {
        self.agent.set_command_sender(tx);
    }

    /// Install the platform timer factory the agent host bridge's
    /// engine-verdict wait polls on (called by `StudioActor::new`, boxed
    /// from its `make_timer`).
    pub fn set_agent_timer(
        &mut self,
        timer: impl FnMut(core::time::Duration) -> crate::AgentTimerFuture + 'static,
    ) {
        self.agent.set_timer(timer);
    }

    /// Install the platform timer factory the initial project sync's
    /// progress deadline polls on (called by `StudioActor::new`, boxed from
    /// its `make_timer` — the same seam shape as [`Self::set_agent_timer`]).
    /// Without it the initial sync awaits unbounded, and a device that dies
    /// mid-stream turns "Syncing project" into a forever-hang (2026-08-26).
    pub fn set_sync_timer(
        &mut self,
        timer: impl FnMut(core::time::Duration) -> crate::AgentTimerFuture + 'static,
    ) {
        self.sync_timer = Some(std::rc::Rc::new(std::cell::RefCell::new(timer)));
    }

    /// Write every agent session's latest engine status and def-side param
    /// records into its shared bridge cell (P2: the engine-verdict seam;
    /// P3: the params-diff seam). Runs at the end of every processed batch
    /// — cheap while no sessions exist — so a running agent's bounded
    /// verdict wait observes the status Revision advancing as pulls land,
    /// and its params diff sees acked def edits.
    fn refresh_agent_engine_status(&mut self) {
        let project = &self.project;
        let history_changed = self.agent.refresh_engine_status(
            |artifact| project.agent_engine_status(artifact),
            |artifact| project.agent_param_defs(artifact),
            |artifact| project.agent_visual_preview(artifact),
        );
        if history_changed {
            // Thumbnail attaches and late verdict resolutions change the
            // DTO without necessarily advancing the sync revision.
            self.mark_dirty();
        }
    }

    /// Fold one spawned-run feedback message into the agent state and mark
    /// the view dirty (the actor applies these synchronously, in order).
    pub fn apply_agent_feedback(&mut self, feedback: crate::AgentFeedback) {
        self.agent.apply_feedback(feedback);
        self.mark_dirty();
    }

    /// Install the platform's user-settings persistence sink (localStorage
    /// on the web), invoked with the user layer's JSON after every
    /// user-driven settings mutation. Install it before the actor takes
    /// ownership of the controller.
    pub fn set_on_user_settings(&mut self, hook: impl Fn(&str) + 'static) {
        self.on_user_settings = Some(Box::new(hook));
    }

    /// Install the clipboard sink node copy writes through.
    pub fn set_on_copy_text(&mut self, hook: impl Fn(&str) + 'static) {
        self.on_copy_text = Some(Box::new(hook));
    }

    /// Install the persisted user settings layer from its JSON document
    /// (the boot localStorage read — call before the actor spawns so
    /// settings are effective before panes render). A parse error logs one
    /// warning and leaves the layer empty.
    pub fn load_user_settings_json(&mut self, json: &str) {
        match crate::StudioSettings::from_json_str(json) {
            Ok(settings) => self.settings.set_user_layer(settings),
            Err(error) => log::warn!("stored settings ignored (unreadable): {error}"),
        }
    }

    /// The layered settings store (effective values for feature code; the
    /// UI reads the view's settings slice instead).
    pub fn settings(&self) -> &crate::app::settings::SettingsStore {
        &self.settings
    }

    /// Apply one settings command: layer loads replace a whole overlay;
    /// user mutations also persist the user layer through the
    /// [`Self::set_on_user_settings`] hook. Always marks the view dirty.
    pub fn apply_settings_command(&mut self, command: crate::SettingsCommand) {
        use crate::SettingsCommand;
        match command {
            SettingsCommand::HostLayerLoaded(settings) => self.settings.set_host_layer(settings),
            SettingsCommand::UserLayerLoaded(settings) => self.settings.set_user_layer(settings),
            SettingsCommand::SetAgentProvider(provider) => {
                self.settings.set_agent_provider(provider);
                self.persist_user_settings();
                self.request_agent_models(false);
            }
            SettingsCommand::SetAgentAnthropicApiKey(key) => {
                self.settings.set_agent_anthropic_api_key(key);
                self.persist_user_settings();
                self.request_agent_models(false);
            }
            SettingsCommand::SetAgentOpenAiApiKey(key) => {
                self.settings.set_agent_openai_api_key(key);
                self.persist_user_settings();
                self.request_agent_models(false);
            }
            SettingsCommand::SetAgentCustomBaseUrl(base_url) => {
                self.settings.set_agent_custom_base_url(base_url);
                self.persist_user_settings();
                self.request_agent_models(false);
            }
            SettingsCommand::SetAgentCustomApiKey(key) => {
                self.settings.set_agent_custom_api_key(key);
                self.persist_user_settings();
                self.request_agent_models(false);
            }
            SettingsCommand::SetAgentOpenRouterApiKey(key) => {
                self.settings.set_agent_openrouter_api_key(key);
                self.persist_user_settings();
                self.request_agent_models(false);
            }
            SettingsCommand::SetAgentModel(model) => {
                self.settings.set_agent_model(model);
                self.persist_user_settings();
            }
            SettingsCommand::SetAgentPriceInputPerMtok(value) => {
                self.settings.set_agent_price_input_per_mtok(value);
                self.persist_user_settings();
            }
            SettingsCommand::SetAgentPriceOutputPerMtok(value) => {
                self.settings.set_agent_price_output_per_mtok(value);
                self.persist_user_settings();
            }
            SettingsCommand::RequestModels { force } => self.request_agent_models(force),
            SettingsCommand::ModelsLoaded {
                provider,
                fingerprint,
                result,
            } => {
                let fetched_at = (self.now_secs)();
                self.settings
                    .agent_models_loaded(provider, &fingerprint, result, fetched_at);
            }
        }
        self.mark_dirty();
    }

    /// Ensure the selected provider's model list is (being) fetched (P8).
    /// Resolves the discovery credentials, debounces through the store's
    /// fingerprint check, and spawns the platform fetch, which reports
    /// back as [`SettingsCommand::ModelsLoaded`] on the command queue.
    /// Without sufficient credentials — or without the platform seams
    /// (host tests, story builds) — any stored state is dropped instead,
    /// so the dropdown falls back to free text rather than spinning.
    fn request_agent_models(&mut self, force: bool) {
        let provider = self.settings.agent_provider();
        let Some(config) = self.settings.agent_discovery_config() else {
            self.settings.clear_agent_models(provider);
            return;
        };
        let fingerprint = crate::app::settings::discovery_fingerprint(&config);
        if !self
            .settings
            .request_agent_models(provider, fingerprint.clone(), force)
        {
            return;
        }
        if !self
            .agent
            .spawn_models_fetch(provider, fingerprint, &config)
        {
            self.settings.clear_agent_models(provider);
        }
    }

    /// Push the current user layer through the persistence hook (no-op
    /// while none is installed, e.g. in tests).
    fn persist_user_settings(&self) {
        if let Some(hook) = &self.on_user_settings {
            hook(&self.settings.user_layer().to_json_string());
        }
    }

    /// Install the platform's randomness (crypto bytes on the web) for
    /// identity minting. The constructor default derives bytes from the
    /// clock — unique enough for tests, not for production.
    pub fn set_random(&mut self, random: impl Fn() -> [u8; 16] + 'static) {
        self.random = Rc::new(random);
    }

    /// Install the platform's LOCAL slug stamp (`YYYY-MM-DD-HHMM`) — the
    /// same closure the library host dates package slugs with, so a
    /// device named at provision and the project generated beside it read
    /// the same day. The default is UTC off the injected clock.
    pub fn set_local_stamp(&mut self, stamp: impl Fn() -> String + 'static) {
        self.local_stamp = Rc::new(stamp);
    }

    /// Install the platform's timer factory for hardware device-session
    /// deadlines (gloo timers on the web; poll timers in host tests).
    /// Install it before any hardware connect — the default makes every
    /// deadline fire immediately.
    pub fn set_device_timers(&mut self, timers: lpa_link::DeviceTimers) {
        self.device.set_timers(timers);
    }

    /// The controller's shared stamping clock, for the actor's progressive
    /// log updates (which stamp `UxUpdate::Log` drafts outside `push_log`).
    pub(crate) fn clock(&self) -> LogClock {
        Rc::clone(&self.now_secs)
    }

    /// Install a hook invoked for **every** stamped entry entering the log
    /// ring, regardless of the console display filter.
    ///
    /// Install it before the actor takes ownership of the controller. The web
    /// shell uses this as the single JS-console mirroring point: every entry
    /// — hand-built drafts, batch-recorded producer drafts, and drained
    /// `log::` sink records — reaches the browser console exactly once.
    /// Progressive live-view entries (the actor's `UxUpdate::Log` path) are
    /// deliberately *not* mirrored there: their drafts are buffered by the
    /// producers and enter the ring — and therefore this hook — when the
    /// controller records them.
    pub fn set_on_entry(&mut self, hook: impl Fn(&UiLogEntry) + 'static) {
        self.on_entry = Some(Box::new(hook));
    }

    /// Invoke the mirror hook (if installed) for one entry entering the ring.
    fn notify_entry(&self, entry: &UiLogEntry) {
        if let Some(hook) = &self.on_entry {
            hook(entry);
        }
    }

    /// Install the device-event mirror hook: sees every record accepted by
    /// the device event log (M0). The web shell uses it to persist the
    /// trace across refreshes and to stream capture-mode records to a
    /// scenario-runner sink. Install before the actor takes ownership.
    pub fn set_on_device_event(&mut self, hook: impl Fn(&DeviceEventRecord) + 'static) {
        self.device_events.borrow_mut().set_on_record(hook);
    }

    /// Turn device-event capture mode (raw RX/TX recording) on or off.
    pub fn set_device_event_capture(&mut self, capture: bool) {
        self.device_events.borrow_mut().set_capture(capture);
    }

    /// Whether device-event capture mode is on.
    pub fn device_event_capture(&self) -> bool {
        self.device_events.borrow().capture()
    }

    /// The retained device event records as JSONL (the export affordance).
    pub fn device_events_jsonl(&self) -> String {
        self.device_events.borrow().to_jsonl()
    }

    /// Read access to the device event log (tests, diagnostics).
    pub fn device_events(&self) -> std::cell::Ref<'_, DeviceEventLog> {
        self.device_events.borrow()
    }

    /// Record one device event stamped with the controller's clock.
    fn record_device_event(
        &self,
        session: Option<&str>,
        endpoint: Option<&str>,
        kind: DeviceEventKind,
    ) {
        self.device_events.borrow_mut().record(DeviceEventRecord {
            t: (self.now_secs)(),
            session: session.map(str::to_string),
            endpoint: endpoint.map(str::to_string),
            kind,
        });
    }

    pub fn snapshot(&self) -> StudioSnapshot {
        StudioSnapshot::new(
            self.device.flow_state().clone(),
            self.server_snapshot(),
            self.project.snapshot(),
            self.logs.to_vec(),
        )
    }

    /// The server slice of the snapshot/view surfaces: the LENS session's
    /// server protocol state (identical to the retired `ServerController`
    /// snapshot in P1, where the pool holds at most one session), or
    /// `Disconnected` while no session exists.
    fn server_snapshot(&self) -> ServerSnapshot {
        let state = self
            .pool
            .lens_session()
            .map(|session| session.server_state().clone())
            .unwrap_or(ServerState::Disconnected);
        ServerSnapshot::new(state)
    }

    /// Whether the lens session's server protocol answered (`Connected`).
    fn has_lightplayer_state(&self) -> bool {
        self.pool
            .lens_session()
            .is_some_and(|session| matches!(session.server_state(), ServerState::Connected { .. }))
    }

    /// Device `id`'s link state (M4: the board the flow named, never "the"
    /// device — kind-asserted, so the sim is never a device, D22).
    fn device_state_for(&self, id: crate::RuntimeId) -> Option<lpa_link::DeviceState> {
        self.pool
            .device_session(id)
            .and_then(crate::RuntimeSession::device_state)
    }

    /// Device `id`'s live hardware [`lpa_link::DeviceSession`] (test stubs
    /// have none).
    fn hardware_session_for(&self, id: crate::RuntimeId) -> Option<&lpa_link::DeviceSession> {
        self.pool
            .device_session(id)
            .and_then(crate::RuntimeSession::hardware_session)
    }

    /// Transport label for device `id` ("USB" for serial classes), derived
    /// from that SESSION's link record — never from the shared connect
    /// flow, which the sim's open may have moved on (P2 coexistence).
    fn transport_label_for(&self, id: crate::RuntimeId) -> Option<&'static str> {
        self.pool
            .device_session(id)?
            .payload()
            .link_session()?
            .provider_kind
            .transport_label()
    }

    /// The device an APP-LEVEL surface means when no card named one: the
    /// lens session when the lens is on a device, else the oldest device
    /// session.
    ///
    /// Reads only — the shell view's device summary, and the ops that
    /// genuinely have no card to inherit from (the console's log-level
    /// selector, `ConnectLightPlayer`'s documented lens fallback). An
    /// operation reaches this ONLY through `DeviceTarget::Ambient`'s
    /// enumerated set; anything else names a card.
    /// The shell view's device summary: whatever board the app-level
    /// surfaces mean ([`Self::ambient_device_id`]). The card surfaces read
    /// their OWN session's sync instead (M3's per-session evidence).
    fn ambient_device_sync(&self) -> Option<&DeviceSyncState> {
        self.device_sync_for(self.ambient_device_id()?)
    }

    pub(crate) fn ambient_device_id(&self) -> Option<crate::RuntimeId> {
        self.pool
            .lens()
            .filter(|id| self.pool.device_session(*id).is_some())
            .or_else(|| {
                self.pool
                    .oldest_device_session()
                    .map(crate::RuntimeSession::id)
            })
    }

    /// The delay before the next passive tick: the MINIMUM over sessions
    /// (runtime-pool P2, per-session tick policy).
    ///
    /// - The LENS session contributes its kind cadence (sim fast, device
    ///   calm) tightened to the verdict-chase interval while a
    ///   just-accepted asset apply awaits its compile verdict, plus its
    ///   own passive-refresh backoff.
    /// - Non-lens sessions (device AND detached sim — P3) contribute the
    ///   time until their next slow status heartbeat, which drains their
    ///   buffered logs so nothing accumulates unboundedly while detached.
    ///   The sim's worker still self-ticks; no wire op rides its
    ///   heartbeat.
    /// - An empty pool falls back to the calm device interval, matching
    ///   the retired disconnected default.
    pub fn next_refresh_interval(&self) -> core::time::Duration {
        let now = (self.now_secs)();
        let lens = self.pool.lens();
        let mut delay: Option<Duration> = None;
        for session in self.pool.sessions() {
            let candidate = if Some(session.id()) == lens {
                // Completion-based pacing: count the lens gap down from the
                // last pull's COMPLETION stamp, so a slow pull pushes the
                // next tick out instead of stacking behind it.
                session.refresh_due_in(now, self.lens_refresh_gap(session))
            } else {
                session.heartbeat_due_in(now)
            };
            delay = Some(delay.map_or(candidate, |current| current.min(candidate)));
        }
        // A card feeding its ▶ tab pulls far faster than its heartbeat, so
        // its gap has to reach the UI timer — otherwise the feed would run
        // at heartbeat pace and the tab would show a 2 s slideshow.
        if let Some(feed) = self.card_feed_due_in(now) {
            delay = Some(delay.map_or(feed, |current| current.min(feed)));
        }
        delay.unwrap_or_else(|| RefreshCadence::default().interval())
    }

    /// The effective minimum gap between passive pulls on the lens session:
    /// the kind cadence, tightened by a post-apply verdict chase, stretched
    /// by that session's failure backoff.
    fn lens_refresh_gap(&self, session: &crate::RuntimeSession) -> Duration {
        let gap = session.cadence_interval();
        let gap = match self.project.verdict_chase_interval() {
            Some(chase) => gap.min(chase),
            None => gap,
        };
        gap.saturating_add(session.backoff_delay())
    }

    /// Whether the lens session's next passive pull is due. Early ticks (the
    /// UI timer racing a slow pull) bounce off this without a wire op.
    fn passive_refresh_due(&self) -> bool {
        let now = (self.now_secs)();
        match self.pool.lens_session() {
            Some(session) => session.refresh_due(now, self.lens_refresh_gap(session)),
            None => true,
        }
    }

    /// Push the lens session's runtime kind into the project controller so
    /// probe policy (visual probe resolution, product-subscription node
    /// scope) tracks the lens.
    fn sync_lens_probe_policy(&mut self) {
        let kind = self.pool.lens_session().map(crate::RuntimeSession::kind);
        self.project.set_lens_runtime_kind(kind);
        // The lens device's reported build, for the add-node picker's gate.
        // Only a Ready device link answers; a sim lens leaves it `None` and
        // the picker offers everything.
        let features = self
            .pool
            .lens_session()
            .and_then(crate::RuntimeSession::device_state)
            .and_then(|state| state.hello().map(|hello| hello.build.features.clone()));
        self.project.set_lens_device_features(features);
    }

    /// Stamp the lens session's pull-completion time: the next passive pull
    /// becomes due one cadence gap after this moment, not one gap after the
    /// pull started.
    pub fn note_passive_refresh_completed(&mut self) {
        let now = (self.now_secs)();
        if let Ok(session) = self.pool.lens_session_mut() {
            session.mark_refresh_complete(now);
        }
    }

    /// Record a passive project-refresh outcome on the LENS session's
    /// backoff (only the lens runs the fallible project pull).
    pub fn record_passive_refresh_success(&mut self) {
        if let Ok(session) = self.pool.lens_session_mut() {
            session.record_refresh_success();
        }
    }

    /// See [`Self::record_passive_refresh_success`].
    pub fn record_passive_refresh_failure(&mut self) {
        if let Ok(session) = self.pool.lens_session_mut() {
            session.record_refresh_failure();
        }
    }

    /// The lens session's current passive-refresh backoff delay (zero
    /// while healthy or with no lens session).
    pub fn passive_refresh_backoff(&self) -> Duration {
        self.pool
            .lens_session()
            .map(crate::RuntimeSession::backoff_delay)
            .unwrap_or(Duration::ZERO)
    }

    /// Run the slow status heartbeat on every DEVICE session — and every
    /// DETACHED sim session (P3: a sim without the lens has no project
    /// pull draining its client, so the heartbeat keeps its buffered wire
    /// logs from accumulating unboundedly) — whose interval elapsed:
    /// drain the session's buffered wire and console log lines into the
    /// session's own console tail (D42 — the per-device console; the
    /// global ring no longer carries session streams) and surface
    /// device-state changes through the change gate. No wire operation
    /// rides a heartbeat — the session's background monitor /
    /// self-ticking worker fills the buffers — so a tick that fans into
    /// lens-refresh + heartbeats still issues at most one wire op per
    /// session.
    pub fn run_due_heartbeats(&mut self) {
        let now = (self.now_secs)();
        let lens = self.pool.lens();
        let mut stamped = Vec::new();
        let mut changed = false;
        for session in self.pool.sessions_mut() {
            let lens_bound = Some(session.id()) == lens;
            if (session.is_sim() && lens_bound) || !session.heartbeat_due(now) {
                continue;
            }
            session.mark_heartbeat(now);
            let mut drained = session.take_pending_logs();
            drained.extend(session.take_device_console_logs());
            if !drained.is_empty() {
                stamped.clear();
                stamped.extend(drained.into_iter().map(|draft| draft.stamp(now)));
                // the devtools mirror still sees every session line (the
                // hook is a field read — disjoint from the pool borrow)
                if let Some(hook) = &self.on_entry {
                    for entry in &stamped {
                        hook(entry);
                    }
                }
                session.push_console_tail(stamped.drain(..));
                changed = true;
            }
            changed |= session.note_device_state_change();
        }
        if changed {
            self.mark_dirty();
        }
    }

    // ---------------------------------------------------------------
    // Device card frame feed (honest-device preview P2)
    // ---------------------------------------------------------------

    /// The tab a card is EFFECTIVELY showing: the persisted choice, else
    /// the default a fresh card opens on.
    ///
    /// The ONE place that answers the question, so the renderer's tab body
    /// and the frame feed's gate can never disagree about which tab is up —
    /// a feed running behind a hidden tab would be a wire op nobody asked
    /// for, and a ▶ tab with no feed would be an empty promise. P3's
    /// default-when-connected rule belongs here, not in a second table.
    fn effective_card_tab(&self, card_key: &str) -> crate::DeviceCardTab {
        match self.card_ui.get(card_key) {
            // An explicit choice is sticky, always. Nothing about the link
            // coming back should move a tab the user put there.
            Some(state) => state.tab,
            None => self.default_card_tab(card_key),
        }
    }

    /// What a card with no saved choice opens on: ▶ when there is a live
    /// picture to open on, the front door otherwise (P3's
    /// default-when-connected rule).
    ///
    /// "A live picture" is a session that is ANSWERING and running a project
    /// — the same pair the renderer reads off the built card
    /// (`card.project.is_some()` on a Ready link) to decide the ▶ tab
    /// exists. Deriving it from the pool here rather than from the card
    /// keeps the rule where the feed can consult it: `card_feed_active`
    /// asks this question before any card is built.
    ///
    /// Landing it HERE and not in a second table at the renderer is the
    /// point — a default that disagreed with the feed's gate would either
    /// pull frames for a hidden tab or open a ▶ tab nothing feeds.
    fn default_card_tab(&self, card_key: &str) -> crate::DeviceCardTab {
        let has_picture = if card_key == crate::SIM_CARD_KEY {
            // The sim's ▶ is its own re-simulation (it IS the simulator, so
            // re-simulating is honest there) — a loaded project is all it
            // needs.
            self.pool
                .sim_session()
                .is_some_and(|session| session.sim_loaded_project().is_some())
        } else {
            self.device_id_for_card_key(card_key)
                .and_then(|id| self.pool.device_session(id))
                .is_some_and(|session| {
                    matches!(session.device_state(), Some(DeviceState::Ready { .. }))
                        && session.device_sync().is_some_and(|sync| {
                            matches!(
                                sync.content,
                                DeviceContent::Known { .. } | DeviceContent::Adopted { .. }
                            )
                        })
                })
        };
        if has_picture {
            crate::DeviceCardTab::Play
        } else {
            crate::DeviceCardTab::Details
        }
    }

    /// The card-identity key a session's card wears: the sim card's fixed
    /// key for the sim session, the device cascade otherwise.
    fn card_key_for_session(session: &crate::RuntimeSession) -> String {
        if session.is_sim() {
            crate::SIM_CARD_KEY.to_string()
        } else {
            Self::card_key_for_device_session(session)
        }
    }

    /// The card-identity key a live DEVICE session's card wears, mirroring
    /// [`crate::UiDeviceCard::identity_key`]'s cascade (uid first, so the
    /// key survives session replaces; the session id while anonymous).
    fn card_key_for_device_session(session: &crate::RuntimeSession) -> String {
        session
            .device_uid()
            .or_else(|| {
                session
                    .device_sync()
                    .and_then(|sync| sync.identity.as_ref())
                    .map(|identity| identity.uid.clone())
            })
            .unwrap_or_else(|| session.id().to_string())
    }

    /// Whether a session's frame feed should be pulling (Q3): a session
    /// that is answering and running a project, whose card is showing the
    /// ▶ tab. Nothing else earns a frame read.
    ///
    /// For a device that means a **Ready** board; for the sim it means a
    /// loaded project (G1 ruling 3 — the sim ▶ rides this same feed, so
    /// the card shows the sim engine's OWN published frames, exactly like
    /// hardware; the in-proc wire makes the bandwidth caveats moot but the
    /// completion-gap cadence still paces it).
    ///
    /// Tab selection is the visibility signal, deliberately. A card on
    /// another tab, a gallery scrolled away, or a backgrounded browser tab
    /// all stop producing reads either here or through the throttled UI
    /// timer, and the completion-gap absorbs whatever the throttle does to
    /// the cadence. There is no separate "surface visible" flag in core to
    /// consult, and inventing one to gate a picture would be the wrong
    /// order of work.
    fn card_feed_active(&self, session: &crate::RuntimeSession) -> bool {
        let answering = if session.is_sim() {
            session.sim_loaded_project().is_some()
        } else {
            matches!(session.device_state(), Some(DeviceState::Ready { .. }))
        };
        if !answering {
            return false;
        }
        let key = Self::card_key_for_session(session);
        self.effective_card_tab(&key) == crate::DeviceCardTab::Play
    }

    /// Time until the earliest due card-feed pull, for the actor's
    /// min-over-sessions delay. `None` when no session is feeding — the
    /// common case, where the UI timer keeps its calm heartbeat pace.
    fn card_feed_due_in(&self, now: f64) -> Option<Duration> {
        self.pool
            .sessions()
            .filter(|session| self.card_feed_active(session))
            .map(|session| {
                session
                    .card_feed()
                    .due_in(now, session.card_feed_interval())
            })
            .min()
    }

    /// Pull one published frame per feeding session whose completion gap
    /// elapsed (the ▶ tab's cadence).
    ///
    /// This is a distinct pull class from the lens refresh: it runs on
    /// NON-lens device sessions, which otherwise issue no wire op between
    /// heartbeats, and it declares [`crate::DEVICE_CARD_FEED_CLASS`] — it
    /// preempts nothing and is cancelled at the next frame boundary when a
    /// user gesture arrives.
    ///
    /// Returns whether a due feed was skipped or cut short by cancellation,
    /// so the actor can count this run toward its starvation floor (a live
    /// control's write stream must not freeze a card's ▶ tab either).
    pub async fn run_due_card_feeds<MakeTimer, Timer, Cancel>(
        &mut self,
        make_timer: MakeTimer,
        cancel: &Cancel,
    ) -> bool
    where
        MakeTimer: FnMut(Duration) -> Timer + Clone,
        Timer: Future<Output = ()>,
        Cancel: CancelSignal + ?Sized,
    {
        let now = (self.now_secs)();
        let due: Vec<crate::RuntimeId> = self
            .pool
            .sessions()
            .filter(|session| {
                self.card_feed_active(session)
                    && session.card_feed().due(now, session.card_feed_interval())
            })
            .map(crate::RuntimeSession::id)
            .collect();
        // Only a feed that was actually DUE can be starved: with no card
        // feeding, a cancel flag flipped by the tick's watcher says nothing
        // about this lane, and must not count toward the actor's floor.
        let mut preempted = false;
        for id in due {
            if cancel.is_cancelled() {
                preempted = true;
                break;
            }
            self.run_card_feed(id, make_timer.clone(), cancel).await;
            // A read cut short mid-frame leaves the flag set.
            preempted = cancel.is_cancelled();
        }
        preempted
    }

    /// One session's feed pull: acquire the project handle, read the
    /// published frame, fold it into the session's feed state, and fill in
    /// a refused display layout locally.
    async fn run_card_feed<MakeTimer, Timer, Cancel>(
        &mut self,
        id: crate::RuntimeId,
        make_timer: MakeTimer,
        cancel: &Cancel,
    ) where
        MakeTimer: FnMut(Duration) -> Timer,
        Timer: Future<Output = ()>,
        Cancel: CancelSignal + ?Sized,
    {
        // Record which card this feed feeds BEFORE the wire work: if this
        // pull is the one that discovers the board is gone, the roster
        // still needs to know where the last frame belongs.
        if let Some(session) = self.pool.session_mut(id) {
            let key = Self::card_key_for_session(session);
            session.card_feed_mut().set_card_key(key);
        }
        let Some(handle_id) = self.acquire_card_feed_handle(id).await else {
            // Nothing loaded (or the handle is unknowable right now): stamp
            // the attempt so the ask paces itself instead of spinning.
            let now = (self.now_secs)();
            if let Some(session) = self.pool.session_mut(id) {
                session.card_feed_mut().mark_pull_complete(now);
            }
            return;
        };
        let deadline = ProgressDeadline::new(
            crate::DEVICE_CARD_FEED_CLASS
                .deadline()
                .unwrap_or(crate::PASSIVE_REFRESH_DEADLINE),
            make_timer,
        );
        let (outcome, request_logs) = {
            let Some(session) = self.pool.session_mut(id) else {
                return;
            };
            let request = lpc_wire::ProjectReadRequest {
                since: None,
                queries: Vec::new(),
                // The whole request: one probe, no mirror queries. This is
                // a picture, not a ProjectSync.
                probes: vec![lpc_wire::ProjectProbeRequest::OutputFrame(
                    lpc_wire::OutputFrameProbeRequest {
                        display_layout: session.card_feed().display_layout_read(),
                    },
                )],
            };
            let Ok(server) = session.client_mut() else {
                return;
            };
            match server
                .project_read_gated(handle_id, request, deadline, cancel)
                .await
            {
                Ok(crate::StudioProjectReadOutcome::Completed(read)) => {
                    (Some(read.events), read.logs)
                }
                // Preempted: keep the old completion stamp so the redo is
                // prompt, exactly like the lens pull.
                Ok(crate::StudioProjectReadOutcome::Cancelled) => return,
                Ok(crate::StudioProjectReadOutcome::TimedOut) => (None, Vec::new()),
                Err(error) => (
                    None,
                    vec![UiLogDraft::new(
                        UiLogLevel::Debug,
                        UiLogOrigin::Studio,
                        format!("device card frame read failed: {error}"),
                    )],
                ),
            }
        };
        self.record_session_logs(id, request_logs);
        let now = (self.now_secs)();
        let mut new_frame = false;
        if let Some(session) = self.pool.session_mut(id) {
            session.card_feed_mut().mark_pull_complete(now);
            match outcome {
                Some(events) => {
                    let outputs = output_frame_entries(&events);
                    // Every entry folds in: the card composes ALL published
                    // outputs into one picture (the small dome's two boxes).
                    let applied = session.card_feed_mut().apply(&outputs, now);
                    new_frame = applied.new_frame;
                }
                // A read that timed out or errored says nothing about the
                // handle staying valid — a device-side reload retires it —
                // so drop the connection-scoped half and re-acquire next
                // tick. The last frame stays on screen, aging honestly.
                None => session.card_feed_mut().invalidate_connection(),
            }
        }
        if new_frame {
            self.mark_dirty();
        }
    }

    /// The device's loaded-project handle for the feed, acquired once per
    /// connection.
    ///
    /// The heartbeat already carries one for every loaded project, so the
    /// common path costs nothing; a session that has not seen a heartbeat
    /// yet spends one `ListLoadedProjects` and remembers the answer.
    async fn acquire_card_feed_handle(&mut self, id: crate::RuntimeId) -> Option<u32> {
        let session = self.pool.session(id)?;
        if let Some(handle) = session.card_feed().handle_id() {
            return Some(handle);
        }
        if let Some(handle) = session.heartbeat_project_handle() {
            self.pool
                .session_mut(id)?
                .card_feed_mut()
                .set_handle_id(handle);
            return Some(handle);
        }
        let catalog = self
            .pool
            .session_mut(id)?
            .client_mut()
            .ok()?
            .list_loaded_projects()
            .await
            .ok()?;
        self.record_session_logs(id, catalog.logs);
        let handle = catalog.projects.first()?.handle_id;
        self.pool
            .session_mut(id)?
            .card_feed_mut()
            .set_handle_id(handle);
        Some(handle)
    }

    pub fn actions(&self) -> UiActions {
        UiActions::new(view_actions(&self.view()))
    }

    pub fn view(&self) -> UiStudioView {
        if let Some(home) = self.home_view() {
            return UiStudioView::new(Vec::new(), self.console_view())
                .with_home(Some(home))
                .with_lens(self.lens_runtime())
                .with_device_sync(self.ambient_device_sync().cloned())
                .with_session(self.session_control())
                .with_settings(self.settings.ui_view());
        }
        // gallery-always (D24): home covers every no-project state, so the
        // pane layout exists only for an open project
        let mut project_pane = self.project.view(self.has_lightplayer_state());
        // Decorate every GLSL inline editor with its agent chat DTO (the
        // project walk stays agent-free; chat state lives on this
        // controller's agent sub-state).
        if let UiViewContent::ProjectEditor(editor) = &mut project_pane.body {
            self.agent
                .decorate_editor_view(editor, self.pool.lens(), &self.agent_view_context());
            // Output faces get the facts their node's sections cannot carry:
            // which board the lens device is (registry truth), and the lamp
            // extent feeding the node (the upstream card's produced control
            // product). "No board known" is a first-class state — a device
            // provisioned outside Studio simply has no board id.
            let lens = self.pool.lens_session();
            crate::app::studio::output_face_decoration::decorate_output_faces(
                editor,
                self.lens_board_id(),
                lens.and_then(|session| session.output_wire_status()),
                lens.and_then(|session| session.total_led_budget()),
            );
        }
        // Hoist the project's edit state to the shell: the web edge arms the
        // unload gate from here rather than walking the pane tree.
        let dirty = match &project_pane.body {
            UiViewContent::ProjectEditor(editor) => editor.dirty,
            _ => crate::DirtySummary::clean(),
        };
        // The sidebar bus pane is GONE (P3): bus-as-controls lives on the
        // module face's panel and bus-as-wiring in its drawer, both hung
        // off the module that owns the scope. The pane column is the
        // Project pane alone.
        let panes = vec![project_pane];
        UiStudioView::new(panes, self.console_view())
            .with_lens(self.lens_runtime())
            .with_open_project(
                self.project.active_library_uid(),
                self.project.active_library_display_name(),
            )
            .with_transient(
                self.project.active_is_transient(),
                self.project.active_transient_example(),
                self.project.transient_fork_generation(),
            )
            .with_device_sync(self.ambient_device_sync().cloned())
            .with_lens_card(self.lens_device_card())
            .with_session(self.session_control())
            .with_settings(self.settings.ui_view())
            .with_dirty(dirty)
    }

    /// The header session·project control's ONE session (single-session
    /// policy, module doc), built from the live cards the gallery roster
    /// itself derives — status included, so the control can never wear a
    /// state the gallery would deny.
    ///
    /// The pool can still be holding two sessions while the policy's
    /// other half lands (and in fixtures that install straight into the
    /// pool); roster order decides, which pins the sim first. Deliberately
    /// NOT coupled to the lens: a session the editor has detached from is
    /// still the session this tab runs, and the control is what says so.
    fn session_control(&self) -> Option<crate::UiChromeSessionControl> {
        let card = home_view_builder::live_session_cards(
            self.registry_cards(),
            &self.home_pool_evidence(),
        )
        .into_iter()
        .next()?;
        let session = if card.sim {
            self.pool.sim_session()
        } else {
            card.session_key.as_deref().and_then(|key| {
                self.pool
                    .sessions()
                    .find(|session| session.id().to_string() == key)
            })
        };
        // Two sources, the same two the card's own narration reads
        // (`device_evidence`): the session's in-flight operation (deploy,
        // flash, upgrade) and the card-owned op flow, which outlives it
        // while a recovery write waits for its replug.
        let busy = session.and_then(|session| {
            session
                .operation_label()
                .map(str::to_string)
                .or_else(|| self.card_op_label(session.id()))
        });
        // Best-effort, and honestly absent when nothing is known: the
        // engine's rate once it publishes frames, the transport for
        // hardware. The lamp extent the spike sketched ("… · 217 lamps")
        // has no honest source for the SIM yet — `total_led_budget` is a
        // device hello field — so it waits for one instead of being
        // invented here.
        let mut facts: Vec<String> = Vec::new();
        if let Some(fps) = card.frame_fps {
            facts.push(format!("{} fps", fps.round() as i64));
        }
        if !card.transport.is_empty() {
            facts.push(card.transport.clone());
        }
        Some(crate::UiChromeSessionControl {
            key: card.identity_key().to_string(),
            sim: card.sim,
            // The control renders the sim's board as a suffix, so the
            // name stays the kind; hardware wears its registry name.
            name: if card.sim {
                "Sim".to_string()
            } else {
                card.name.clone()
            },
            // D4: the sim's board is the one it inherited from the
            // project it runs — a device's board is already in its name.
            board: card
                .sim
                .then(|| card.board_id.as_deref().map(crate::board_display_name))
                .flatten(),
            status: home_view_builder::chip_status(&card.state),
            busy,
            stat_line: (!facts.is_empty()).then(|| facts.join(" · ")),
        })
    }

    /// The card-owned op flow's label for a session, when one is running
    /// on it (the flow outlives `operation_label` across the replug an
    /// awaiting op is waiting for).
    fn card_op_label(&self, id: crate::RuntimeId) -> Option<String> {
        Some(self.device_card_ops.get(&id)?.borrow().label.clone())
    }

    /// The gallery's registry rows as cards — the roster derivation's
    /// other input, empty until the library hydrates.
    fn registry_cards(&self) -> &[crate::app::home::UiDeviceCard] {
        self.home_inputs
            .as_ref()
            .map(|inputs| inputs.devices.as_slice())
            .unwrap_or(&[])
    }

    /// The board the LENS runtime is known to be — for a device,
    /// `RegisteredDevice.board_id`, stamped at provisioning (board-selection
    /// M5) and cached with the gallery's registry rows; for the SIM, the
    /// board it inherited from the project it runs (vision D4).
    ///
    /// `None` is ORDINARY, not exceptional: no lens, an unidentified board,
    /// a device provisioned outside Studio, or a sim running an untargeted
    /// project. The wire's `HardwareFacts.board_id` is not a fallback — it
    /// is always `None` today, so the registry is the device's only source.
    fn lens_board_id(&self) -> Option<&str> {
        let session = self.pool.lens_session()?;
        if session.is_sim() {
            // the sim is still not a device (D22): it has no registry row,
            // and its board is the session's own advisory identity
            return session.sim_board_id();
        }
        let uid = session.device_uid().or_else(|| {
            session
                .device_sync()
                .and_then(|sync| sync.identity.as_ref())
                .map(|identity| identity.uid.clone())
        })?;
        self.home_inputs
            .as_ref()?
            .registered
            .iter()
            .find(|device| device.uid == uid)
            .and_then(|device| device.board_id.as_deref())
    }

    /// The settings-derived slice the agent view decoration needs:
    /// availability (Ready ⇔ the selected provider is sufficiently
    /// configured), the provider's setup guidance while it is not, the
    /// cost rates for the usage estimate, and the model-chip slice
    /// (effective model + P8 discovered options — lifted from the same
    /// settings view the popover renders, so the chip and the popover can
    /// never disagree).
    fn agent_view_context(&self) -> crate::AgentViewContext {
        let ready = self.settings.agent_ready();
        let agent_settings = self.settings.ui_view().agent;
        crate::AgentViewContext {
            availability: if ready {
                crate::UiAgentAvailability::Ready
            } else {
                crate::UiAgentAvailability::NeedsKey
            },
            setup: (!ready).then(|| crate::provider_guidance(self.settings.agent_provider())),
            cost_rates: self.settings.agent_cost_rates(),
            model: crate::UiAgentModelView {
                effective: agent_settings.model_effective,
                options: agent_settings.model_options,
                loading: agent_settings.models_loading,
            },
        }
    }

    /// The LENS session's card (D43): the grown control panel the editor
    /// docks as its right-side pane. The same construction the gallery
    /// roster uses — one derivation, both scales.
    ///
    /// TOTAL over a live lens session: whenever the pool has a lens the
    /// editor gets a card. The shell has no other device surface, so a
    /// `None` here is a hole in the right column — which is exactly how
    /// the retired step-stack pane stayed reachable (defect
    /// `docs/defects/2026-07-28-retired-device-pane-still-reachable.md`).
    /// Unplugging therefore FADES the card ("Seen …/Reconnect") instead of
    /// removing it; the lens and the project are left alone, so a flaky
    /// cable never yanks anyone out of their editor.
    fn lens_device_card(&self) -> Option<crate::UiDeviceCard> {
        let session = self.pool.lens_session()?;
        let evidence = self.home_pool_evidence();
        let card = if session.is_sim() {
            // the sim's evidence exists exactly while its session does
            evidence
                .sim
                .as_ref()
                .map(crate::app::home::home_view_builder::sim_card)
        } else {
            // THE LENS session's entry, not the first (M3): with several
            // boards attached the editor may be a lens on any of them.
            let lens_key = session.id().to_string();
            evidence
                .devices
                .iter()
                .find(|entry| entry.session_key.as_deref() == Some(lens_key.as_str()))
                .map(crate::app::home::home_view_builder::device_card_from_live_evidence)
                .map(|card| self.with_remembered_sighting(card))
        };
        card.map(|card| self.overlay_card_ui(card))
    }

    /// Live evidence carries no registry entry, so an Offline derivation
    /// from it has no sighting ("Not seen yet" — wrong for a board that
    /// answered a second ago). Borrow the remembered card's `last_seen_at`
    /// so the unplugged lens reads "Seen …", the same words the gallery
    /// uses for the same device.
    fn with_remembered_sighting(&self, mut card: crate::UiDeviceCard) -> crate::UiDeviceCard {
        let crate::RosterCardState::Offline {
            last_seen_at: None, ..
        } = &card.state
        else {
            return card;
        };
        let Some(uid) = card.uid.clone() else {
            return card;
        };
        let seen = self.home_inputs.as_ref().and_then(|inputs| {
            inputs
                .devices
                .iter()
                .find(|remembered| remembered.uid.as_deref() == Some(uid.as_str()))
                .and_then(|remembered| match remembered.state {
                    crate::RosterCardState::Offline { last_seen_at } => last_seen_at,
                    _ => None,
                })
        });
        if seen.is_some() {
            card.state = crate::RosterCardState::Offline { last_seen_at: seen };
        }
        card
    }

    /// The lens's runtime binding for the view (SDI: the URL is the
    /// focused document — the web shell's D37 route reconciliation binds
    /// to this). A device session's `dev` uid prefers the wire hello and
    /// falls back to the connect-as-pull identity.
    fn lens_runtime(&self) -> Option<crate::UiLensRuntime> {
        self.pool.lens_session().map(|session| {
            if session.is_sim() {
                // the session's loaded-project record (not the library
                // binding) is the key: it survives detach, so re-attach
                // flows address the same document
                crate::UiLensRuntime::Sim {
                    project_uid: session
                        .sim_loaded_project()
                        .map(|project| project.uid.clone()),
                }
            } else {
                let uid = session.device_uid().or_else(|| {
                    session
                        .device_sync()
                        .and_then(|sync| sync.identity.as_ref())
                        .map(|identity| identity.uid.clone())
                });
                crate::UiLensRuntime::Device { uid }
            }
        })
    }

    /// The home gallery: shown whenever NO project is open — always
    /// (D24; the M4 transitional bridge and its home-only-when-link-idle
    /// rule are gone). Connected devices are cards, not a pane takeover;
    /// link trouble surfaces as a gallery issue chip.
    fn home_view(&self) -> Option<UiHomeView> {
        if self.project_is_loaded() {
            return None;
        }
        let opening = self.pending_open.as_ref();
        let issue = match self.device.flow_state() {
            ConnectFlowState::SelectingProvider { issue, .. } => issue.clone(),
            ConnectFlowState::Failed { issue } => Some(issue.clone()),
            _ => None,
        };
        let mut view = home_view_builder::build_home_view(
            self.home_inputs.as_ref(),
            opening.map(|pending| pending.card_key().to_string()),
            issue,
            &self.home_pool_evidence(),
        );
        // Overlay each card's persisted UI view-state + live op (the
        // builder leaves `ui` default; identity keys the overlay).
        view.devices = view
            .devices
            .into_iter()
            .map(|card| self.overlay_card_ui(card))
            .collect();
        view.backup = self.device_backup.clone();
        // The open flow (if any) rides the bound device's own card as a
        // body takeover (G2 ruling, 2026-08-05). Resolving it here — after
        // the roster exists — is what lets the takeover follow the card's
        // identity key while the flow runs, and pins that card first.
        let takeover =
            home_view_builder::pin_setup_card(&mut view.devices, self.setup_binding().as_deref());
        view.setup = self.setup_view(takeover);
        Some(view)
    }

    /// The session an open setup flow is bound to, as the roster names it
    /// — `Some` exactly while the flow should be rendering as a device
    /// card's body.
    ///
    /// `None` covers the cases where it must not: no flow at all; the
    /// pre-device states (nothing granted yet, so there is no card to be
    /// the body of); the PRE-VERDICT states (see below); and
    /// DEVICE_HOME/CLOSED, where the handoff is done and the card's own
    /// body returns. The hardware binding is the bound `RuntimeId` rather
    /// than the session's stored card key, so a port release un-binds the
    /// takeover the moment the session goes.
    ///
    /// **The takeover binds at the VERDICT** (G2 follow-up, 2026-08-05).
    /// Between the port grant and the probe's answer the live card is
    /// anonymous, so a board the registry already knows would render
    /// twice — the remembered row plus an un-mergeable anonymous card.
    /// The verdict is the recognition moment: from there the live card
    /// carries the probed uid ([`Self::setup_recognised_uid`]) and merges
    /// with its registry row, and there is exactly one card to ride.
    fn setup_binding(&self) -> Option<String> {
        if !self.setup_flow_running() {
            return None;
        }
        let session = self.setup.as_ref()?;
        if !session.state().kind().has_verdict() {
            return None;
        }
        if session.sim {
            // The sim's key is known from the start, but no sim card
            // exists until the session does — so this resolves to nothing
            // (a standalone card) for the whole sim path up to the start.
            session.card_key.clone()
        } else {
            self.setup_device.map(|id| id.to_string())
        }
    }

    /// Whether a setup flow is still WORKING. DEVICE_HOME is terminal (the
    /// reducer's own words: "the card owns the surface from here"), so a
    /// finished flow lingering in `setup` must not keep standing anything
    /// down.
    fn setup_flow_running(&self) -> bool {
        self.setup.as_ref().is_some_and(|session| {
            !matches!(
                session.state().kind(),
                crate::SetupStateKind::DeviceHome | crate::SetupStateKind::Closed
            )
        })
    }

    /// The bound session whose roster row STANDS DOWN — the pre-verdict
    /// window only (port granted, probe not yet answered).
    ///
    /// This is the one scoped suppression the model keeps, and it exists
    /// because of the KNOWN board: its registry row is already on the
    /// grid, the just-granted session has no identity to merge with it
    /// yet, and two rows for one board is exactly what this model is for.
    /// No evidence is lost — the wizard's own PORT_PICKING/PROBING body
    /// is the narration for precisely this window, and the row returns
    /// (as the takeover's card) the instant the verdict lands.
    fn setup_pre_verdict_session(&self) -> Option<crate::RuntimeId> {
        if !self.setup_flow_running() {
            return None;
        }
        if self.setup.as_ref()?.state().kind().has_verdict() {
            return None;
        }
        self.setup_device
    }

    /// The uid the PROBE anchored, once a verdict carries one — the
    /// recognition the live card wears until its own identity read lands.
    ///
    /// Fed to the bound row as `pending_uid`, which is the roster's
    /// existing "this live evidence belongs to THAT remembered card"
    /// channel (built for the reconnect-transient-twin defect, and this
    /// is the same problem arriving from the probe instead of a click).
    /// It is what makes the live card and the registry row ONE card from
    /// the verdict on.
    fn setup_recognised_uid(&self) -> Option<String> {
        let session = self.setup.as_ref()?;
        if !self.setup_flow_running() {
            return None;
        }
        session.state().probe()?.hardware_uid.clone()
    }

    /// The runtime pool's roster evidence (P4 → multi-device M3): one
    /// evidence bundle per DEVICE session — reconcile state reads each
    /// device session, never the lens, which may be on the sim (P2
    /// coexistence) — plus the SIM session's evidence while it lives
    /// (D36: the sim card exists exactly as long as the session does).
    ///
    /// The card-owned op flow is per-session (M4) — each entry carries
    /// its own board's `op_in_flight`. The connect FLOW is still
    /// app-singular (M5 makes it targeted): its narration rides the
    /// OLDEST device entry, or a session-less entry ("evidence of work,
    /// not of a session") while nothing is attached. Recorded in the
    /// multi-device ADR so M5 inherits the attribution decision.
    fn home_pool_evidence(&self) -> crate::app::home::HomePoolEvidence {
        // Every session gets its evidence — INCLUDING the one an open setup
        // flow drives, from the verdict on. That card is not a rival to the
        // wizard; it IS the wizard's card (G2 ruling, 2026-08-05), and
        // standing it down wholesale was what made one board wear two
        // representations. The ONE window that still stands down is
        // pre-verdict, where the row cannot yet be merged with the
        // remembered card of a board we already know
        // ([`Self::setup_pre_verdict_session`]).
        let pre_verdict = self.setup_pre_verdict_session();
        // …and during the port request itself the flow's session exists
        // BEFORE the bind can land (`open_provider` is a long await that
        // renders while the session installs — the connect-window card
        // flash). Any session absent from the request's snapshot is the
        // flow's own and stands down with it.
        let unclaimed_newborn = |id: crate::RuntimeId| {
            self.setup_flow_running()
                && self
                    .setup_port_snapshot
                    .as_ref()
                    .is_some_and(|snapshot| !snapshot.contains(&id))
        };
        let mut devices: Vec<crate::app::home::HomeDeviceEvidence> = self
            .pool
            .device_sessions()
            .filter(|session| Some(session.id()) != pre_verdict && !unclaimed_newborn(session.id()))
            .map(|session| self.device_evidence(session))
            .collect();
        // The connect NARRATION does still stand down while a flow is
        // open: the wizard's PORT_PICKING body says "the browser is asking
        // which port…" itself, and the app-singular connect evidence would
        // otherwise spawn a session-less "Connecting…" card beside the
        // wizard — the very twin this model exists to delete.
        let flow_connect = if self.setup_flow_running() {
            crate::ConnectEvidence::Idle
        } else {
            self.gallery_connect_evidence()
        };
        let pending_uid = self.device.pending_reconnect_uid().map(str::to_string);
        match devices.first_mut() {
            Some(first) => {
                // An in-flight operation on the session owns its card's
                // narration (set in `device_evidence`); the connect flow
                // narrates otherwise.
                if first.connect == crate::ConnectEvidence::Idle {
                    first.connect = flow_connect;
                }
                first.pending_uid = pending_uid;
            }
            None => {
                // No session, so no op to pin: `op_in_flight` stays false
                // here. It is evidence of work on A SESSION, and this
                // entry is evidence of work with none.
                devices.push(crate::app::home::HomeDeviceEvidence {
                    connect: flow_connect,
                    pending_uid,
                    ..Default::default()
                });
            }
        }
        // The probe's recognition, on the bound row: it makes the live
        // card adopt the remembered board's uid and name, which is what
        // lets the roster's twin filter drop the registry row. Applied
        // after the connect-flow overlay above so the more specific
        // attribution wins on that row.
        if let (Some(device_id), Some(uid)) = (self.setup_device, self.setup_recognised_uid()) {
            let key = device_id.to_string();
            if let Some(bound) = devices
                .iter_mut()
                .find(|entry| entry.session_key.as_deref() == Some(key.as_str()))
            {
                bound.pending_uid = Some(uid);
            }
        }
        let now = (self.now_secs)();
        let sim = self
            .pool
            .sim_session()
            .map(|session| crate::app::home::HomeSimEvidence {
                project: session
                    .sim_loaded_project()
                    .map(|project| crate::UiDeviceProjectChip {
                        uid: project.uid.clone(),
                        name: project.name.clone(),
                    }),
                // The sim ▶ rides the SAME feed as a device card (G1
                // ruling 3) — these are the sim engine's own published
                // frames, never a browser re-simulation.
                frame: session.card_feed().frame().cloned(),
                frame_age_secs: session.card_feed().frame_age_secs(now),
                fps: session.engine_fps(),
                board_id: session.sim_board_id().map(str::to_string),
                console_tail: session.console_tail().iter().cloned().collect(),
            });
        crate::app::home::HomePoolEvidence { devices, sim }
    }

    /// One device session's roster evidence — every field reads THIS
    /// session (M3). The app-singular connect-flow/card-op narration is
    /// overlaid by [`Self::home_pool_evidence`], not here.
    fn device_evidence(
        &self,
        session: &crate::RuntimeSession,
    ) -> crate::app::home::HomeDeviceEvidence {
        let (observed_version, head_version) = session.device_versions();
        let (local_saved_at, pushed_at) = session.device_drift_times();
        // A long-running operation (flash / erase / push — the same flag
        // that blocks pool replaces) owns this card's narration.
        let connect = match session.operation_label() {
            Some(label) => crate::ConnectEvidence::OperationInFlight {
                label: label.to_string(),
                percent: None,
            },
            None => crate::ConnectEvidence::Idle,
        };
        let now = (self.now_secs)();
        crate::app::home::HomeDeviceEvidence {
            session_key: Some(session.id().to_string()),
            // The ▶ tab's live frame, aged at build time. The feed only
            // runs while that tab is up (see `run_due_card_feeds`), but
            // what it produced stays on the session — so switching tabs
            // freezes the picture rather than dropping it, and the age
            // keeps telling the truth about how old it is.
            frame: session.card_feed().frame().cloned(),
            frame_age_secs: session.card_feed().frame_age_secs(now),
            frame_card_key: session.card_feed().card_key().map(str::to_string),
            fps: session.engine_fps(),
            // THIS session's card-owned op (M4) — the pin that keeps the
            // card alive through a `Gone` link belongs to the board the
            // op runs on, never to whichever board attached first.
            op_in_flight: self.device_card_ops.contains_key(&session.id()),
            sync: session.device_sync().cloned(),
            link: session.device_state(),
            connect,
            transport: session
                .payload()
                .link_session()
                .and_then(|link| link.provider_kind.transport_label())
                .map(str::to_string),
            observed_version,
            head_version,
            local_saved_at,
            pushed_at,
            pending_uid: None,
            console_tail: session.console_tail().iter().cloned().collect(),
            recovery: session.recovery_status().cloned(),
            // `DeviceSnapshot` also carries `probed_mac` (the flash
            // preflight's efuse read, acquisition rule A2) — consumed by
            // the identity resolver (`places/identity_resolution.rs`),
            // not surfaced on the card yet.
            detected_chip: session
                .hardware_session()
                .and_then(|hardware| hardware.snapshot().detected_chip),
            port_label: session.endpoint_label().map(|label| {
                // Suffix the grant's short id ("port-2") so two identical
                // VID:PID labels still read as different grants.
                match session.payload().link_session().and_then(|link| {
                    short_endpoint_id(link.endpoint_id.as_str()).map(str::to_string)
                }) {
                    Some(short) => format!("{label} · {short}"),
                    None => label.to_string(),
                }
            }),
        }
    }

    /// The connect flow narrated as roster evidence: a hardware provider
    /// mid-discovery/connect pulses the live card ("Connecting…"), the
    /// ladder's second rung pulses "Resetting…" (M6), a held port shows
    /// the In-use-elsewhere card, and an exhausted ladder the honest
    /// Not-responding card. The sim's flow never reaches the roster (the
    /// sim is not a device, D22); `Failed` — an ERROR ending, not a
    /// ladder ending — stays the gallery issue chip.
    fn gallery_connect_evidence(&self) -> crate::ConnectEvidence {
        let provider_id = match self.device.flow_state() {
            ConnectFlowState::DiscoveringEndpoints { provider_id, .. } => *provider_id,
            ConnectFlowState::Connecting { endpoint, .. } => endpoint.provider_id,
            ConnectFlowState::Retrying { endpoint, .. } => {
                if endpoint.provider_id.transport_label().is_some() {
                    return crate::ConnectEvidence::Connecting {
                        phase: crate::ConnectPhase::Resetting,
                    };
                }
                return crate::ConnectEvidence::Idle;
            }
            ConnectFlowState::PortHeld { endpoint } => {
                if endpoint.provider_id.transport_label().is_some() {
                    return crate::ConnectEvidence::PortHeldElsewhere;
                }
                return crate::ConnectEvidence::Idle;
            }
            ConnectFlowState::Unresponsive { endpoint } => {
                if endpoint.provider_id.transport_label().is_some() {
                    return crate::ConnectEvidence::Failed;
                }
                return crate::ConnectEvidence::Idle;
            }
            // SelectingEndpoint is a parked picker, not work in flight
            _ => return crate::ConnectEvidence::Idle,
        };
        if provider_id.transport_label().is_some() {
            crate::ConnectEvidence::Connecting {
                phase: crate::ConnectPhase::Connecting,
            }
        } else {
            crate::ConnectEvidence::Idle
        }
    }

    /// The console slice of the view: ring entries passing the display
    /// filter, plus the hidden count and the filter state for the toolbar.
    /// Carries the connected server's last-requested log level (or `None`
    /// while disconnected) for the device-level selector.
    fn console_view(&self) -> UiConsoleView {
        let mut console = UiConsoleView::from_ring(&self.logs, &self.log_filter);
        console.device_log_level = self
            .pool
            .lens_session()
            .and_then(crate::RuntimeSession::requested_log_level);
        console
    }

    /// The current project revision, or `None` before any sync.
    fn current_revision(&self) -> Option<i64> {
        self.project.snapshot().sync.map(|sync| sync.revision)
    }

    /// Rebuild and return a view **only if something changed** since the last
    /// gate. Returns `None` when neither the applied revision advanced nor a
    /// local mutation is pending, so the actor skips a redundant snapshot after
    /// a quiet (empty / unchanged) pull.
    ///
    /// Calling this records the observed revision and clears the dirty flag, so
    /// the next quiet tick gates out.
    pub fn view_if_changed(&mut self) -> Option<UiStudioView> {
        // Feed running agents their engine status first: the write targets
        // a shared cell the spawned run polls, so it must happen whether or
        // not the change gate emits a snapshot this batch.
        self.refresh_agent_engine_status();
        let revision = self.current_revision();
        let advanced = revision != self.applied_revision;
        if !self.dirty && !advanced {
            if !self.logs_dirty {
                return None;
            }
            // Log-only churn: publish on a throttle so a per-line stream
            // (device console, verbose sessions) cannot force full-view
            // rebuilds at stream rate. Pending lines are never lost — they
            // sit in the tails/ring and ride the next emitted view.
            let now = (self.now_secs)();
            if now - self.last_log_only_publish < LOG_ONLY_PUBLISH_MIN_GAP_SECS {
                return None;
            }
            self.last_log_only_publish = now;
        }
        self.applied_revision = revision;
        self.dirty = false;
        self.logs_dirty = false;
        Some(self.view())
    }

    /// Mark local (non-network) state as changed so the next
    /// [`Self::view_if_changed`] emits.
    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Mark streamed log lines as pending so the next
    /// [`Self::view_if_changed`] emits — throttled, unlike [`Self::mark_dirty`].
    fn mark_logs_dirty(&mut self) {
        self.logs_dirty = true;
    }

    /// Stamp one draft with the injected clock, append it to the bounded log
    /// ring, and mark the view dirty.
    ///
    /// The actor routes action-outcome, error, and drained `log::` sink logs
    /// here so the cap lives in core (Q5) and stamping happens in exactly one
    /// place (P1). The mirror hook (see [`Self::set_on_entry`]) fires for the
    /// stamped entry.
    pub fn push_log(&mut self, draft: UiLogDraft) {
        let entry = draft.stamp((self.now_secs)());
        self.notify_entry(&entry);
        self.logs.push(entry);
        self.mark_dirty();
    }

    /// Stamp a batch of one SESSION's drained lines into that session's
    /// console tail (D42 — the per-device console's recording point).
    /// The devtools mirror still sees every line; the global ring does
    /// not carry session streams anymore. Falls back to the ring when
    /// the session is already gone (its card is gone too).
    fn record_session_logs(&mut self, id: crate::RuntimeId, drafts: Vec<UiLogDraft>) {
        if drafts.is_empty() {
            return;
        }
        if self.pool.session_mut(id).is_none() {
            self.record_logs(drafts);
            return;
        }
        let timestamp = (self.now_secs)();
        let stamped: Vec<UiLogEntry> = drafts
            .into_iter()
            .map(|draft| draft.stamp(timestamp))
            .collect();
        for entry in &stamped {
            self.notify_entry(entry);
        }
        if let Some(session) = self.pool.session_mut(id) {
            session.push_console_tail(stamped);
        }
        self.mark_logs_dirty();
    }

    /// Stamp a batch of producer drafts (all with one clock read — they
    /// arrived together) into the ring and mark streamed logs pending
    /// (throttled publish). No-op for an empty batch so a quiet passive
    /// refresh stays change-gated out. The mirror hook fires once per
    /// stamped entry.
    fn record_logs(&mut self, drafts: Vec<UiLogDraft>) {
        if drafts.is_empty() {
            return;
        }
        let timestamp = (self.now_secs)();
        for draft in drafts {
            let entry = draft.stamp(timestamp);
            self.notify_entry(&entry);
            self.logs.push(entry);
        }
        self.mark_logs_dirty();
    }

    /// Apply a console command (from [`StudioCommand::Console`]): mutate the
    /// display filter or clear the ring, and mark the view dirty so the next
    /// gate emits the reshaped console.
    /// Install the injected library host into the home gallery and the
    /// project flows (load-as-push / save-as-pull — roadmap M3/M4b).
    /// Schedules the first gallery hydration; the actor (or the next
    /// dispatch) drains it.
    pub fn attach_library(&mut self, host: Rc<dyn LibraryHost>) {
        let clock = std::rc::Rc::clone(&self.now_secs);
        let random = std::rc::Rc::clone(&self.random);
        self.library_host = Some(Rc::clone(&host));
        self.project.set_library(host, clock, random);
        self.request_library_refresh();
    }

    /// Note that the gallery's cached inputs are stale. Cheap; the actual
    /// re-hydration happens in [`Self::refresh_library_if_pending`].
    pub fn request_library_refresh(&mut self) {
        if self.library_host.is_some() {
            self.library_refresh_pending = true;
        }
    }

    /// Re-hydrate the cached gallery inputs when a refresh is due, and
    /// release any project locks whose projects closed since the last
    /// settle. Called at the end of every dispatch and by the actor after
    /// each command batch, so host futures always get driven even when a
    /// close happened on a synchronous path.
    pub async fn settle_library(&mut self) {
        self.project.release_closed_library_projects().await;
        if !self.library_refresh_pending {
            return;
        }
        self.library_refresh_pending = false;
        let Some(host) = self.library_host.clone() else {
            return;
        };
        let open_elsewhere = host.open_elsewhere_uids().await;
        match host.catalog_snapshot().await {
            Ok(fs) => {
                let inputs = home_view_builder::hydrate_home_inputs(fs, &open_elsewhere);
                // The add-node picker's import source is derived from the
                // same walk (P5) — one snapshot read feeds both the
                // gallery and the picker.
                self.project
                    .set_import_patterns(home_view_builder::importable_patterns(&inputs));
                self.home_inputs = Some(inputs);
            }
            Err(error) => {
                log::warn!("library snapshot failed: {error}");
                self.home_inputs = Some(HomeInputs {
                    issue: Some(crate::UiIssue::new(format!(
                        "Your projects could not be listed: {error}"
                    ))),
                    ..HomeInputs::default()
                });
            }
        }
        self.mark_dirty();
    }

    /// What device `id` holds (connect-as-pull result), for the pane and
    /// cards. `None` when that session carries no reconcile bundle (the sim
    /// never does — D22). The bundle lives on the DEVICE session, wherever
    /// the lens is (P2 coexistence).
    pub fn device_sync_for(&self, id: crate::RuntimeId) -> Option<&DeviceSyncState> {
        self.pool
            .device_session(id)
            .and_then(crate::RuntimeSession::device_sync)
    }

    /// Carry a card-owned op flow across a same-endpoint session replace.
    ///
    /// The flow is keyed by session (M4), and the replug that ENDS a
    /// recovery write mints a new session for the same physical board —
    /// so without this the "unplug the board and plug it back in"
    /// instruction would vanish at the exact moment the user obeyed it
    /// (the shape of the 2026-07-31 bench regression). The endpoint is
    /// the board's continuity across a replug; the `RuntimeId` is not.
    fn migrate_card_op(&mut self, replaced: Option<crate::RuntimeId>, installed: crate::RuntimeId) {
        let Some(replaced) = replaced.filter(|old| *old != installed) else {
            return;
        };
        if let Some(flow) = self.device_card_ops.remove(&replaced) {
            self.device_card_ops.insert(installed, flow);
        }
    }

    /// Carry a card's persisted UI view-state across the anonymous →
    /// identified key flip.
    ///
    /// `card_ui` is keyed by `UiDeviceCard::identity_key()`, which is the
    /// session key while a board is anonymous and its `dev…` uid the
    /// moment identity resolves. The flip is the same wart
    /// [`Self::migrate_card_op`] exists for (2026-08-02): state the user
    /// built on the anonymous card — the open tab, the open sheet —
    /// orphans under the old key seconds after it was set. Identity
    /// arrives EARLIER now (at the hello, not at a stamp mid-provision),
    /// which narrows the window but does not close it.
    ///
    /// The uid's own entry wins when it already has one: a remembered
    /// board's saved state outranks whatever the pre-identity card
    /// accumulated.
    fn migrate_card_ui(&mut self, session_key: &str, uid: &str) {
        if session_key == uid || self.card_ui.contains_key(uid) {
            return;
        }
        if let Some(state) = self.card_ui.remove(session_key) {
            self.card_ui.insert(uid.to_string(), state);
        }
    }

    /// The live device session a CARD KEY names, if any.
    ///
    /// One vocabulary for op targeting (M4): `UiDeviceCard::identity_key()`
    /// is a stamped device's `dev…` uid, or an anonymous board's session
    /// key — and this resolves both, session key first, because that is
    /// the key an unstamped board wears. A registry (offline) card's uid
    /// resolves to nothing, which is correct: there is no session to
    /// operate on.
    fn device_id_for_card_key(&self, card_key: &str) -> Option<crate::RuntimeId> {
        self.pool
            .device_sessions()
            .find(|session| session.id().to_string() == card_key)
            .or_else(|| {
                self.pool.device_sessions().find(|session| {
                    session
                        .device_sync()
                        .and_then(|sync| sync.identity.as_ref())
                        .is_some_and(|identity| identity.uid == card_key)
                        || session.device_uid().as_deref() == Some(card_key)
                })
            })
            .map(crate::RuntimeSession::id)
    }

    /// The session a device operation acts on.
    ///
    /// An unresolvable target REFUSES (M4). It never falls back to "the"
    /// device: falling back is exactly how an operation reaches a board
    /// nobody named, and the worst thing this can produce — flashing the
    /// wrong board — is silent when it happens.
    fn resolve_device_target(
        &self,
        target: &crate::DeviceTarget,
    ) -> Result<crate::RuntimeId, UiError> {
        match target {
            crate::DeviceTarget::Card(card_key) => {
                self.device_id_for_card_key(card_key).ok_or_else(|| {
                    UiError::MissingSession(format!("\"{card_key}\" is not a connected device"))
                })
            }
            crate::DeviceTarget::Ambient => self
                .ambient_device_id()
                .ok_or_else(|| UiError::MissingSession("no device is connected".to_string())),
        }
    }

    /// The DEVICE session `id`, mutably — `None` for a missing session or
    /// a non-device id (device flows never land on the sim).
    fn device_session_by_id(&mut self, id: crate::RuntimeId) -> Option<&mut crate::RuntimeSession> {
        self.pool
            .session_mut(id)
            .filter(|session| session.is_device())
    }

    /// Connect-is-a-pull (D8) targeting session `id` (multi-device M3:
    /// attaching a second board pulls THAT board): pull the device's copy,
    /// classify it against the library, persist per the M4b locking model,
    /// refresh the registry, and cache the result on that session. Never
    /// fails the connect — errors are logged and leave the state `None`
    /// (flash/erase must stay reachable on a device we can't read).
    pub(crate) async fn refresh_device_sync_for(&mut self, id: crate::RuntimeId) {
        if let Some(session) = self.device_session_by_id(id) {
            session.clear_reconcile();
        }
        let pulled = {
            let Some(session) = self.device_session_by_id(id) else {
                return;
            };
            let Ok(server) = session.client_mut() else {
                return;
            };
            match device_session::pull_device_copy(
                server,
                crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID,
            )
            .await
            {
                Ok(pulled) => pulled,
                Err(error) => {
                    self.push_log(UiLogDraft::new(
                        UiLogLevel::Warn,
                        UiLogOrigin::Studio,
                        format!("device pull failed: {error}"),
                    ));
                    // an actionable state, never an eternal "Checking…":
                    // the dialog shows the unreadable note; flash/erase
                    // stay reachable
                    if let Some(session) = self.device_session_by_id(id) {
                        session.set_device_sync(Some(DeviceSyncState {
                            identity: None,
                            content: DeviceContent::Unreadable {
                                detail: format!("could not read the device: {error}"),
                            },
                        }));
                    }
                    self.record_device_event(
                        Some(&id.to_string()),
                        None,
                        DeviceEventKind::Sync {
                            content: "unreadable".to_string(),
                        },
                    );
                    self.mark_dirty();
                    return;
                }
            }
        };
        if let Some(session) = self.device_session_by_id(id) {
            session.set_device_storage_id(Some(pulled.storage_id.clone()));
        }
        match self.absorb_device_pull(id, pulled).await {
            Ok(state) => {
                self.record_device_event(
                    Some(&id.to_string()),
                    None,
                    DeviceEventKind::Sync {
                        content: device_content_label(&state.content).to_string(),
                    },
                );
                if let Some(session) = self.device_session_by_id(id) {
                    session.set_device_sync(Some(state));
                }
                self.mark_dirty();
            }
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!("device state could not be recorded: {error}"),
                ));
            }
        }
    }

    /// Classify a pulled device copy and persist what the locking model
    /// allows: the active project's observation goes through this tab's
    /// own handle; other projects' observations and adoptions run as
    /// catalog transactions; a project open in ANOTHER tab is classified
    /// but not banked (that tab owns the history subtree).
    async fn absorb_device_pull(
        &mut self,
        device_id: crate::RuntimeId,
        mut pulled: device_session::PulledDeviceCopy,
    ) -> Result<DeviceSyncState, UiError> {
        self.record_logs(core::mem::take(&mut pulled.logs));
        let now = (self.now_secs)();
        let resolved = self
            .resolve_session_identity(device_id, pulled.identity.clone())
            .await;
        let identity = resolved.as_ref().map(|resolved| resolved.identity.clone());

        // A sighting alone never registers a board (design §4 step 4): a
        // MAC-identified stranger has a uid the moment it says hello, but
        // the registry only remembers boards we were told about — one the
        // library already knows, or one carrying a legacy stamp. An
        // unknown board's row is created by ADOPTION (below) or by
        // provisioning, never by having been seen.
        if let Some(resolved) = &resolved
            && (resolved.registered || pulled.identity.is_some())
        {
            self.upsert_device_entry(device_id, &resolved.identity, now)
                .await;
        }

        // a content read/hash failure on an IDENTIFIED device: partial
        // knowledge survives — the identity above was already reconciled
        // and the sighting recorded, so the card keeps its name and
        // dedups against the registry; only classification is unknown.
        // Checked BEFORE the empty-content branch (a failed read has no
        // files, but that must never classify as Empty).
        if let Some(detail) = &pulled.read_error {
            self.push_log(UiLogDraft::new(
                UiLogLevel::Warn,
                UiLogOrigin::Studio,
                format!("device content read failed: {detail}"),
            ));
            return Ok(DeviceSyncState {
                identity,
                content: DeviceContent::Unreadable {
                    detail: format!("could not read the device: {detail}"),
                },
            });
        }

        // a device with no project files — or only `.lp/*` metadata (a
        // freshly stamped board) — is EMPTY, not unreadable
        let has_project_content = pulled
            .files
            .iter()
            .any(|(path, _)| !path.starts_with(".lp/"));
        if !has_project_content {
            return Ok(DeviceSyncState {
                identity,
                content: DeviceContent::Empty,
            });
        }
        if !pulled.has_manifest {
            return Ok(DeviceSyncState {
                identity,
                content: DeviceContent::Unreadable {
                    detail: "project files present but no readable manifest".to_string(),
                },
            });
        }

        // What FORMAT the board's project states (P5). Read here, before
        // any relation talk, because the firmware refuses to load anything
        // but the current format: a stale-format board is not "running an
        // older version", it is not running at all. The banking below
        // still happens — the board's bytes reach the library either way
        // (D8) — but the CARD is told the truth by the classification at
        // the end of each branch.
        let format = device_session::classify_device_project(&pulled.files);
        let stale_format = !matches!(format, lpa_upgrade::FormatClass::Current);

        // resolve the manifest uid against the library
        let local = match (&pulled.manifest_uid, self.library_host()) {
            (Some(uid), Ok(host)) => match host.catalog_snapshot().await {
                Ok(fs) => {
                    let store = crate::app::library::LibraryStore::read_only(fs);
                    // The device's last-push marker (§3c-1): what WE last
                    // pushed to this board, per its registry association —
                    // the auto-fast-forward base, read off the same
                    // snapshot.
                    let association = identity.as_ref().and_then(|id| {
                        crate::app::places::DeviceRegistry::new(store.fs_handle())
                            .list()
                            .ok()?
                            .into_iter()
                            .find(|entry| entry.uid == id.uid)?
                            .association
                    });
                    match store.list() {
                        Ok(summaries) => summaries
                            .into_iter()
                            .find(|summary| summary.uid.to_string() == *uid)
                            .map(|summary| {
                                // relation + line version numbers (the
                                // roster's "Running vN"/"Push vN" evidence)
                                // in one handle open
                                let (relation, versions, head, head_saved_at) = store
                                    .open(summary.uid)
                                    .map(|handle| {
                                        let history = &handle.history;
                                        let head = history.head();
                                        (
                                            history.classify(pulled.observed),
                                            (
                                                history.version_number(pulled.observed),
                                                head.and_then(|head| history.version_number(head)),
                                            ),
                                            head,
                                            head.and_then(|head| history.saved_at(head)),
                                        )
                                    })
                                    .unwrap_or((
                                        lpc_history::SyncRelation::Diverged,
                                        (None, None),
                                        None,
                                        None,
                                    ));
                                (
                                    summary,
                                    relation,
                                    versions,
                                    head,
                                    head_saved_at,
                                    association,
                                )
                            }),
                        Err(_) => None,
                    }
                }
                Err(_) => None,
            },
            _ => None,
        };

        if let Some((summary, relation, versions, head, head_saved_at, association)) = local {
            // §3c-3 drift wall-clock: the local head's save time + when we
            // last pushed to THIS board (its association) — the
            // Edited-on-device card's plain-words comparison.
            let pushed_at = association
                .as_ref()
                .filter(|assoc| assoc.project == summary.uid)
                .map(|assoc| assoc.at);
            if let Ok(session) = self.pool.device_session_mut(device_id) {
                session.set_device_versions(versions);
                session.set_device_drift_times((head_saved_at, pushed_at));
            }
            let Some(identity_value) = identity.clone() else {
                // anonymous hardware (rule A4): classification only —
                // nothing is banked for a board that can name neither
                // its silicon nor a stamp; naming re-runs this.
                let content = device_content_for_format(
                    &format,
                    Some(summary.uid.to_string()),
                    Some(summary.slug.clone()),
                    pulled.observed,
                )
                .unwrap_or(DeviceContent::Known {
                    project_uid: summary.uid.to_string(),
                    slug: summary.slug.clone(),
                    observed: pulled.observed,
                    relation,
                });
                return Ok(DeviceSyncState { identity, content });
            };
            let device_uid: lpc_history::PrefixedUid = identity_value.uid.parse().map_err(|e| {
                UiError::MissingSession(format!("device uid {:?}: {e}", identity_value.uid))
            })?;
            let handled = self.project.record_device_observation_on_active(
                &summary.uid.to_string(),
                device_uid,
                pulled.observed,
                &pulled.files,
                now,
            )?;
            let banked = if handled {
                true
            } else {
                let host = self.library_host()?;
                let op = CatalogOp::RecordDeviceObservation {
                    project_uid: summary.uid.to_string(),
                    device: self.registry_entry_for_session(device_id, &identity_value, now),
                    observed: pulled.observed,
                    files: pulled.files.clone(),
                };
                match host.catalog(op).await {
                    Ok(_) => true,
                    Err(error) => {
                        // open in another tab (or busy): that tab owns the
                        // history — classify only, don't bank from here
                        self.push_log(UiLogDraft::new(
                            UiLogLevel::Info,
                            UiLogOrigin::Studio,
                            format!(
                                "device observation for {} not banked: {error}",
                                summary.slug
                            ),
                        ));
                        false
                    }
                }
            };

            // AUTO FAST-FORWARD (§3c-1): the board's copy diverges, but
            // what we last pushed to THIS board is still our head — local
            // never moved, so the banked copy is a pure extension of it.
            // Adopt it without asking (the old head stays in history);
            // Edited-on-device is reserved for genuine forks. Skipped
            // when banking didn't happen (another tab owns the history) —
            // the card then asks, which is always safe.
            // NEVER for a stale-format copy: fast-forwarding would move
            // the library head BACKWARD onto bytes this build cannot open,
            // silently undoing an upgrade the user already did.
            let mut relation = relation;
            if relation == lpc_history::SyncRelation::Diverged
                && !stale_format
                && banked
                && head.is_some()
                && association.as_ref().is_some_and(|assoc| {
                    assoc.project == summary.uid && Some(assoc.version) == head
                })
            {
                let host = self.library_host()?;
                match host
                    .catalog(CatalogOp::AdoptObservedVersion {
                        project_uid: summary.uid.to_string(),
                        observed: pulled.observed,
                    })
                    .await
                {
                    Ok(_) => {
                        relation = lpc_history::SyncRelation::AtHead;
                        // adopt appended the observed copy as the new head
                        let new_head = versions.1.map(|n| n + 1);
                        if let Ok(session) = self.pool.device_session_mut(device_id) {
                            session.set_device_versions((new_head, new_head));
                        }
                        self.push_log(UiLogDraft::new(
                            UiLogLevel::Info,
                            UiLogOrigin::Studio,
                            format!(
                                "Pulled your edits from {} — {} is up to date",
                                identity_value.name, summary.slug
                            ),
                        ));
                    }
                    Err(error) => {
                        // stays Diverged: the card asks, nothing is lost
                        self.push_log(UiLogDraft::new(
                            UiLogLevel::Warn,
                            UiLogOrigin::Studio,
                            format!("auto fast-forward for {} failed: {error}", summary.slug),
                        ));
                    }
                }
            }

            let content = device_content_for_format(
                &format,
                Some(summary.uid.to_string()),
                Some(summary.slug.clone()),
                pulled.observed,
            )
            .unwrap_or(DeviceContent::Known {
                project_uid: summary.uid.to_string(),
                slug: summary.slug.clone(),
                observed: pulled.observed,
                relation,
            });
            self.request_library_refresh();
            return Ok(DeviceSyncState { identity, content });
        }

        // unknown project: adopt when the device has an identity to
        // attribute it to. A MAC board always does (adoption runs here,
        // at connect); rule A4's anonymous board is what still waits.
        //
        // The format is NOT reported here: naming that board comes first
        // either way (M8′ order — nothing can be pushed to an unnamed
        // board), and naming re-runs this whole classification, which is
        // where the format card lands if there is one.
        let Some(identity_value) = &identity else {
            return Ok(DeviceSyncState {
                identity,
                content: DeviceContent::PendingIdentity {
                    observed: pulled.observed,
                },
            });
        };
        let host = self.library_host()?;
        let outcome = host
            .catalog(CatalogOp::AdoptDevicePackage {
                device: self.registry_entry_for_session(device_id, identity_value, now),
                files: pulled.files.clone(),
            })
            .await
            .map_err(UiError::from)?;
        self.request_library_refresh();
        let summary = outcome.summary.ok_or_else(|| {
            UiError::MissingSession("device adoption produced no package".to_string())
        })?;
        self.push_log(UiLogDraft::new(
            UiLogLevel::Info,
            UiLogOrigin::Studio,
            format!("Adopted \"{}\" from {}", summary.slug, identity_value.name),
        ));
        // The adoption is byte-faithful BY DESIGN (a pull is not an
        // import), so a stale-format board becomes a stale-format library
        // package — listed honestly by its own format card (P3) and named
        // by the device card here.
        let content = device_content_for_format(
            &format,
            Some(summary.uid.to_string()),
            Some(summary.slug.clone()),
            pulled.observed,
        )
        .unwrap_or(DeviceContent::Adopted {
            project_uid: summary.uid.to_string(),
            slug: summary.slug,
            observed: pulled.observed,
        });
        Ok(DeviceSyncState { identity, content })
    }

    /// Resolve THIS session's identity from its own evidence (device
    /// identity design §3, rules A1–A4), migrate the registry row a
    /// legacy stamp left behind, and name the result from the registry.
    ///
    /// The order is the contract: silicon (the hello's efuse MAC, then a
    /// download-mode read banked on this session) outranks the stamped
    /// `/.lp/device.json`, because the stamp dies with a flash erase and
    /// the MAC does not. `None` is rule A4 — the board stays
    /// session-scoped, exactly today's unstamped behavior.
    async fn resolve_session_identity(
        &mut self,
        device_id: crate::RuntimeId,
        file_identity: Option<crate::app::places::DeviceIdentity>,
    ) -> Option<SessionIdentity> {
        let evidence = self.identity_evidence_for(device_id, file_identity.as_ref());
        let resolved = crate::app::places::resolve_identity(&evidence)?;
        let uid = resolved.uid.to_string();

        // D5 (clones): two live boards claiming one MAC. The newcomer
        // stays anonymous rather than sharing a key — two cards under one
        // `identity_key()` is a keyed-list duplicate, which panics Dioxus
        // (the 2026-07-15 crash class), and remembering the wrong board
        // under a remembered row is worse than not remembering it.
        if self.another_live_session_wears(device_id, &uid) {
            self.push_log(UiLogDraft::from_notice(UiNotice::warning(format!(
                "Two connected boards report the same hardware id ({}) — \
                 the second one stays unnamed until one is unplugged.",
                resolved.hardware_id
            ))));
            if let Ok(session) = self.pool.device_session_mut(device_id) {
                session.set_hardware_id(None);
            }
            return None;
        }

        // Lazy re-key (design §4): this board was remembered under the
        // uid a stamp gave it. Move the row BEFORE the sighting upsert so
        // name, board, and association all land on the derived key.
        if let Some(old_uid) = resolved.rekey_from
            && let Ok(host) = self.library_host()
            && let Err(error) = host
                .catalog(CatalogOp::RekeyRegisteredDevice {
                    old_uid: old_uid.to_string(),
                    new_uid: uid.clone(),
                    hardware_id: resolved.hardware_id.to_string(),
                })
                .await
        {
            log::warn!("device registry re-key failed: {error}");
        }

        let registered = self.registered_device(&uid).await;
        // D34: the registry is the naming truth. A row's name wins; a
        // board with no row falls back to whatever its legacy file said,
        // and an empty name renders through the card's existing cascade.
        let name = registered
            .as_ref()
            .map(|entry| entry.name.clone())
            .filter(|name| !name.is_empty())
            .or_else(|| file_identity.as_ref().map(|identity| identity.name.clone()))
            .unwrap_or_default();
        let identity = crate::app::places::DeviceIdentity {
            uid: uid.clone(),
            name,
        };

        // The D34 write-back survives only where the file is still a
        // store: host-class and legacy boards that ANSWERED with one. An
        // ESP-class board's name lives in the registry alone (design §5),
        // so a rename never writes its filesystem again.
        if let (crate::app::places::HardwareId::Minted { .. }, Some(file_identity)) =
            (&resolved.hardware_id, &file_identity)
            && !identity.name.is_empty()
            && identity.name != file_identity.name
        {
            self.write_identity_name_to_device(device_id, &identity)
                .await;
        }

        // The anonymous → identified key flip (design §6): the card's
        // `identity_key()` moves from the session key to the uid the
        // instant identity resolves, and persisted card UI state keyed by
        // the old key would orphan — the 2026-08-02 wart `migrate_card_op`
        // already handles for op flows. Same move, same reason.
        self.migrate_card_ui(&device_id.to_string(), &uid);

        if let Ok(session) = self.pool.device_session_mut(device_id) {
            session.set_hardware_id(Some(resolved.hardware_id));
        }
        Some(SessionIdentity {
            identity,
            registered: registered.is_some(),
        })
    }

    /// The identity evidence THIS session offers (design §3): the hello's
    /// efuse MAC (A1), the base MAC a flash preflight read in download
    /// mode (A2, already normalized by `lpa_link::normalize_base_mac`),
    /// and the stamped uid (A3) — the legacy file when it exists, else
    /// the uid the hello carries.
    fn identity_evidence_for(
        &self,
        device_id: crate::RuntimeId,
        file_identity: Option<&crate::app::places::DeviceIdentity>,
    ) -> crate::app::places::IdentityEvidence {
        let session = self.pool.device_session(device_id);
        crate::app::places::IdentityEvidence {
            hello_base_mac: session.and_then(|session| match session.device_state() {
                Some(DeviceState::Ready { hello }) => hello.hardware.base_mac,
                _ => None,
            }),
            probed_mac: session
                .and_then(crate::RuntimeSession::hardware_session)
                .and_then(|hardware| hardware.snapshot().probed_mac),
            stamped_uid: file_identity
                .map(|identity| identity.uid.clone())
                .or_else(|| session.and_then(crate::RuntimeSession::device_uid)),
            file_name: file_identity.map(|identity| identity.name.clone()),
        }
    }

    /// Is ANOTHER live device session already wearing `uid`? The D5 clone
    /// guard's question — asked of the pool, because a duplicate MAC is
    /// only a problem while both boards are attached.
    fn another_live_session_wears(&self, device_id: crate::RuntimeId, uid: &str) -> bool {
        self.pool.device_sessions().any(|session| {
            session.id() != device_id
                && session
                    .device_sync()
                    .and_then(|sync| sync.identity.as_ref())
                    .is_some_and(|identity| identity.uid == uid)
        })
    }

    /// The registry row for `uid`, from a fresh catalog snapshot.
    async fn registered_device(
        &mut self,
        uid: &str,
    ) -> Option<crate::app::places::RegisteredDevice> {
        let fs = self.library_host().ok()?.catalog_snapshot().await.ok()?;
        crate::app::places::DeviceRegistry::new(fs)
            .list()
            .unwrap_or_default()
            .into_iter()
            .find(|entry| entry.uid == uid)
    }

    /// Write the registry name back into a legacy board's
    /// `/.lp/device.json`. A failed write only logs: the next connect
    /// retries, and the registry keeps winning in the meantime
    /// (`upsert_device_merged`).
    async fn write_identity_name_to_device(
        &mut self,
        device_id: crate::RuntimeId,
        identity: &crate::app::places::DeviceIdentity,
    ) {
        use lpc_model::AsLpPath;
        match self
            .pool
            .device_session_mut(device_id)
            .and_then(crate::RuntimeSession::client_mut)
        {
            Ok(server) => match server
                .fs_write(
                    crate::app::places::DEVICE_IDENTITY_PATH.as_path(),
                    &identity.to_json_bytes(),
                )
                .await
            {
                Ok(logs) => self.record_logs(logs),
                Err(error) => log::warn!("device rename write-back failed: {error}"),
            },
            Err(_) => log::warn!("device rename write-back skipped: no live server"),
        }
    }

    /// Record the device sighting in the registry (merge semantics: an
    /// association survives sight-only upserts).
    async fn upsert_device_entry(
        &mut self,
        device_id: crate::RuntimeId,
        identity: &crate::app::places::DeviceIdentity,
        now: f64,
    ) {
        let Ok(host) = self.library_host() else {
            return;
        };
        let entry = self.registry_entry_for_session(device_id, identity, now);
        if let Err(error) = host.catalog(CatalogOp::UpsertRegisteredDevice(entry)).await {
            log::warn!("device registry upsert failed: {error}");
        }
        self.request_library_refresh();
    }

    /// The registry row a session's write targets: the pull's identity
    /// plus the two facts only the live session knows — its transport
    /// label and the identity SOURCE it resolved (design §4's
    /// `hardware_id` column).
    fn registry_entry_for_session(
        &self,
        device_id: crate::RuntimeId,
        identity: &crate::app::places::DeviceIdentity,
        now: f64,
    ) -> crate::app::places::RegisteredDevice {
        let mut entry = device_session::registry_entry_for(
            identity,
            self.transport_label_for(device_id).unwrap_or_default(),
            now,
        );
        entry.hardware_id = self
            .pool
            .device_session(device_id)
            .and_then(crate::RuntimeSession::hardware_id)
            .map(|hardware_id| hardware_id.to_string());
        entry
    }

    /// Where the open project usually lives: the registered device whose
    /// association points at it, for the pane's disconnected state (D23).
    /// Resolve a slug-or-uid key to a concrete push target from a fresh
    /// library snapshot.
    async fn resolve_deploy_target(&mut self, key: &str) -> Result<DeployTarget, UiError> {
        let host = self.library_host()?;
        let fs = host.catalog_snapshot().await.map_err(UiError::from)?;
        let store = crate::app::library::LibraryStore::read_only(fs);
        let uid = store
            .resolve_key(key)
            .map_err(|e| UiError::MissingSession(format!("library: {e}")))?;
        let handle = store
            .open(uid)
            .map_err(|e| UiError::MissingSession(format!("library: {e}")))?;
        let head = handle
            .content_hash()
            .map_err(|e| UiError::MissingSession(format!("library: {e}")))?;
        Ok(DeployTarget {
            project_uid: uid.to_string(),
            slug: handle.slug.clone(),
            head,
            version_number: handle.history.version_number(head),
        })
    }

    /// Every file of one library package, read through a fresh read-only
    /// snapshot. No lock: the source of a vendoring is somebody else's
    /// project, possibly open in another tab, and reading it must never
    /// contend with them ([`Self::resolve_deploy_target`]'s precedent).
    async fn read_library_package_files(
        &mut self,
        key: &str,
    ) -> Result<Vec<(String, Vec<u8>)>, UiError> {
        let host = self.library_host()?;
        let fs = host.catalog_snapshot().await.map_err(UiError::from)?;
        let store = crate::app::library::LibraryStore::read_only(fs);
        let uid = store
            .resolve_key(key)
            .map_err(|e| UiError::MissingSession(format!("library: {e}")))?;
        store
            .open(uid)
            .map_err(|e| UiError::MissingSession(format!("library: {e}")))?
            .read_all_files()
            .map_err(|e| UiError::MissingSession(format!("library: {e}")))
    }

    async fn execute_deploy_op(&mut self, op: DeployOp, updates: UxUpdateSink) -> UiResult {
        // Every deploy verb acts on ONE board: the card the gesture came
        // from (M4). Push rows, drift sheets and the Danger tab all live
        // on a card, so every variant carries its target.
        let device_id = self.resolve_device_target(match &op {
            DeployOp::PushProject { target, .. }
            | DeployOp::AdoptDeviceCopy { target }
            | DeployOp::KeepBothFork { target }
            | DeployOp::UpgradeDeviceProject { target }
            | DeployOp::EraseDevice { target } => target,
        })?;
        match op {
            DeployOp::PushProject { key, .. } => {
                // The direct push (M5; since M8′ the ONLY push): the
                // dispatching gesture is the D11 consent. The device must
                // carry a NAMED identity (the Running-family states and
                // the picker's Connected-empty guarantee it; unnamed
                // boards go through the name sheet first). A
                // MAC-identified board has a uid from its first hello
                // (device identity design §3) — the name is what the
                // gently-insist-on-a-name flow is still waiting for, so
                // the gate reads the name, not the uid.
                let device = self
                    .device_sync_for(device_id)
                    .and_then(|sync| sync.identity.clone())
                    .filter(|identity| !identity.name.is_empty())
                    .ok_or_else(|| {
                        UiError::MissingSession("no named device is connected".to_string())
                    })?;
                let target = self.resolve_deploy_target(&key).await?;
                let label = match target.version_number {
                    Some(version) => format!("Pushing v{version}"),
                    None => format!("Pushing {}", target.slug),
                };
                // The session's in-flight operation both blocks pool
                // replaces (DQ-A) and narrates the card's
                // Operation-in-flight state; the progressive view emit
                // below puts that state on screen while the push runs.
                self.pool
                    .device_session_mut(device_id)?
                    .set_operation(Some(label));
                self.mark_dirty();
                updates.emit(UxUpdate::View(self.view()));
                let result = self.run_device_push(device_id, &device, &target).await;
                if let Ok(session) = self.pool.device_session_mut(device_id) {
                    session.set_operation(None);
                }
                self.mark_dirty();
                result?;
                self.request_library_refresh();
                Ok(UiNotices::new().with_notice(UiNotice::info(format!(
                    "Pushed {} to {}",
                    target.slug, device.name
                ))))
            }
            DeployOp::AdoptDeviceCopy { .. } => {
                let (project_uid, observed) = self.diverged_device_copy(device_id)?;
                let host = self.library_host()?;
                host.catalog(CatalogOp::AdoptObservedVersion {
                    project_uid,
                    observed,
                })
                .await
                .map_err(|error| self.library_error_with_name(error))?;
                self.request_library_refresh();
                self.refresh_device_sync_for(device_id).await;
                Ok(UiNotices::new()
                    .with_notice(UiNotice::info("The device's version is now the newest")))
            }
            DeployOp::KeepBothFork { .. } => {
                let (project_uid, observed) = self.diverged_device_copy(device_id)?;
                // the fork's name: the live session's stamped identity
                // (the D30 sheet is the one entry since M8′)
                let device_name = self
                    .device_sync_for(device_id)
                    .and_then(|sync| sync.identity.as_ref())
                    .map(|identity| identity.name.clone())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| "device".to_string());
                let host = self.library_host()?;
                let outcome = host
                    .catalog(CatalogOp::ForkObservedVersion {
                        project_uid,
                        observed,
                        device_name,
                    })
                    .await
                    .map_err(|error| self.library_error_with_name(error))?;
                self.request_library_refresh();
                let slug = outcome
                    .summary
                    .map(|summary| summary.slug)
                    .unwrap_or_default();
                Ok(UiNotices::new()
                    .with_notice(UiNotice::info(format!("Saved the device's copy as {slug}"))))
            }
            DeployOp::UpgradeDeviceProject { .. } => {
                self.run_device_project_upgrade(device_id, updates).await
            }
            DeployOp::EraseDevice { .. } => {
                // from the card's Danger tab, behind the D41 confirm sheet
                self.reset_to_blank(device_id, updates).await
            }
        }
    }

    /// The roster's Upgrade verb (P5): make the board's old-format project
    /// runnable again.
    ///
    /// The device is NEVER upgraded in place (D14 / ADR 2026-07-05
    /// decision 5). The migrated bytes are born in the library and travel
    /// back over the ordinary hash-checked push, so the board's copy and
    /// the library's stay one thing.
    ///
    /// **Which bytes get migrated** — the choice this flow turns on. When
    /// the board's project resolves to a library package (the common case:
    /// connect-is-a-pull already adopted or matched it), THAT package is
    /// the migration subject, not the pulled copy. The board's bytes were
    /// banked at connect, so nothing is lost either way, and taking the
    /// library's line means the verb can never overwrite a newer local
    /// head with an older board copy — that is `Use board copy`'s job, and
    /// it is a decision the user makes, not one an upgrade makes for them.
    /// Only when the library has NO copy at all (adoption did not run) is
    /// the pulled copy migrated and adopted, which is the same
    /// adopt-then-push shape connect uses.
    async fn run_device_project_upgrade(
        &mut self,
        device_id: crate::RuntimeId,
        updates: UxUpdateSink,
    ) -> UiResult {
        let sync = self
            .device_sync_for(device_id)
            .cloned()
            .ok_or_else(|| UiError::MissingSession("this device has not been read".to_string()))?;
        let DeviceContent::OldFormat {
            project_uid, class, ..
        } = &sync.content
        else {
            return Err(UiError::UnsupportedAction(
                "This board's project is not waiting for a format upgrade".to_string(),
            ));
        };
        // Below the floor / from a newer LightPlayer: the card never
        // offers the verb, and a stray dispatch says why rather than
        // failing halfway through a migration that cannot run.
        if !class.is_upgradable() {
            return Err(UiError::UnsupportedAction(class.describe()));
        }
        // The push needs a stamped board (same requirement as any push);
        // an unnamed board is asked for its name first.
        let device = sync
            .identity
            .clone()
            .ok_or_else(|| UiError::MissingSession("no named device is connected".to_string()))?;
        let project_uid = project_uid.clone();

        self.pool
            .device_session_mut(device_id)?
            .set_operation(Some("Upgrading".to_string()));
        self.mark_dirty();
        updates.emit(UxUpdate::View(self.view()));
        let result = self
            .upgrade_and_push_device_project(device_id, &device, project_uid)
            .await;
        if let Ok(session) = self.pool.device_session_mut(device_id) {
            session.set_operation(None);
        }
        self.mark_dirty();
        let (slug, upgraded_from) = result?;
        self.request_library_refresh();
        Ok(
            UiNotices::new().with_notice(UiNotice::info(match upgraded_from {
                Some(from) => format!(
                    "Upgraded {slug} from format {from} to {} and put it back on {}",
                    lpc_model::PROJECT_FORMAT_VERSION,
                    device.name
                ),
                // The library copy was already current — the board was the
                // only stale one. Say what actually happened.
                None => format!("Pushed {slug} to {} — it was already upgraded", device.name),
            })),
        )
    }

    /// The Upgrade verb's body: land current-format bytes in the library,
    /// then push them. Returns the pushed project's slug and the format it
    /// was migrated from (`None` when the library copy needed no work).
    async fn upgrade_and_push_device_project(
        &mut self,
        device_id: crate::RuntimeId,
        device: &crate::app::places::DeviceIdentity,
        project_uid: Option<String>,
    ) -> Result<(String, Option<u32>), UiError> {
        let (project_uid, upgraded_from) = match project_uid {
            // The library's copy may already be current — the user opened
            // it in the editor (which migrates on open, P3) and only the
            // board was left behind. Then there is nothing to migrate and
            // the push alone fixes it. Checked off a lock-free snapshot
            // first, because the catalog op would take the project lock
            // and refuse for a project open in this very tab.
            Some(project_uid) if self.library_package_is_current(&project_uid).await => {
                (project_uid, None)
            }
            Some(project_uid) => {
                let host = self.library_host()?;
                let outcome = host
                    .catalog(CatalogOp::UpgradePackageFormat {
                        project_uid: project_uid.clone(),
                    })
                    .await
                    .map_err(|error| self.library_error_with_name(error))?;
                (project_uid, outcome.upgraded_from)
            }
            None => self.adopt_upgraded_device_copy(device_id, device).await?,
        };
        let target = self.resolve_deploy_target(&project_uid).await?;
        self.run_device_push(device_id, device, &target).await?;
        Ok((target.slug, upgraded_from))
    }

    /// Whether the library's copy of `project_uid` opens as it stands
    /// (current format, readable manifest) — read off a lock-free
    /// snapshot. `false` for a package that needs migrating, is blocked,
    /// or cannot be found at all: each of those is the caller's next
    /// question, and none of them may be answered "already fine".
    async fn library_package_is_current(&mut self, project_uid: &str) -> bool {
        let Ok(host) = self.library_host() else {
            return false;
        };
        let Ok(fs) = host.catalog_snapshot().await else {
            return false;
        };
        crate::app::library::LibraryStore::read_only(fs)
            .list()
            .unwrap_or_default()
            .into_iter()
            .find(|summary| summary.uid.to_string() == project_uid)
            .is_some_and(|summary| summary.health == crate::app::library::PackageHealth::Ready)
    }

    /// The unadopted board's half of the Upgrade verb: pull the board's
    /// copy, migrate it, and adopt the RESULT as a new library package —
    /// so even here the bytes that land on disk are current-format ones,
    /// born in the library (D14).
    async fn adopt_upgraded_device_copy(
        &mut self,
        device_id: crate::RuntimeId,
        device: &crate::app::places::DeviceIdentity,
    ) -> Result<(String, Option<u32>), UiError> {
        let default_storage_id = self
            .pool
            .device_session(device_id)
            .and_then(|session| session.device_storage_id().map(str::to_string))
            .unwrap_or_else(|| {
                crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID.to_string()
            });
        let mut pulled = {
            let server = self.pool.device_session_mut(device_id)?.client_mut()?;
            device_session::pull_device_copy(server, &default_storage_id).await?
        };
        self.record_logs(core::mem::take(&mut pulled.logs));
        if let Some(detail) = &pulled.read_error {
            return Err(UiError::MissingSession(format!(
                "could not read the device: {detail}"
            )));
        }
        let mut files = device_session::device_project_files(&pulled.files);
        let report = lpa_upgrade::upgrade_to_current(&mut files)
            .map_err(|error| UiError::UnsupportedAction(error.to_string()))?;

        let now = (self.now_secs)();
        let host = self.library_host()?;
        let outcome = host
            .catalog(CatalogOp::AdoptDevicePackage {
                device: device_session::registry_entry_for(
                    device,
                    self.transport_label_for(device_id).unwrap_or_default(),
                    now,
                ),
                files: files.into_pairs(),
            })
            .await
            .map_err(|error| self.library_error_with_name(error))?;
        let summary = outcome.summary.ok_or_else(|| {
            UiError::MissingSession("upgrading produced no library package".to_string())
        })?;
        Ok((summary.uid.to_string(), Some(report.from)))
    }

    /// The diverged copy an adopt/keep-both verb targets: the live
    /// device session's sync evidence (the D30 card sheet is the one
    /// entry since M8′ — there is no dialog).
    fn diverged_device_copy(
        &mut self,
        device_id: crate::RuntimeId,
    ) -> Result<(String, lpc_history::ContentHash), UiError> {
        match self.device_sync_for(device_id).map(|sync| &sync.content) {
            Some(DeviceContent::Known {
                project_uid,
                observed,
                relation: lpc_history::SyncRelation::Diverged,
                ..
            }) => Ok((project_uid.clone(), *observed)),
            _ => Err(UiError::UnsupportedAction(
                "The device's copy is not diverged".to_string(),
            )),
        }
    }

    /// Name the connected device (the one naming path: the Needs-a-name
    /// card's form and the setup form's post-flash name both land here).
    ///
    /// A board whose identity is its silicon takes a **registry write**
    /// (design §5): the name goes on the row keyed by its derived uid and
    /// nothing at all is written to the board, because a name kept on an
    /// erasable filesystem was never the truth. Only a session with no
    /// silicon to anchor to — a host-class embedder, or a board that
    /// reported no MAC — falls back to the legacy stamp.
    async fn run_device_naming(
        &mut self,
        device_id: crate::RuntimeId,
        name: String,
    ) -> Result<crate::app::places::DeviceIdentity, UiError> {
        match self
            .pool
            .device_session(device_id)
            .and_then(crate::RuntimeSession::hardware_id)
        {
            Some(hardware_id @ crate::app::places::HardwareId::EspEfuse { .. }) => {
                self.write_name_to_registry(device_id, hardware_id, name)
                    .await
            }
            _ => {
                // Current firmware always reports its efuse MAC in the
                // hello, so an ESP-class session reaching the fallback
                // means the silicon half went missing somewhere. Say so:
                // the stamp about to run writes a uid onto a filesystem
                // the next erase takes with it.
                if self
                    .hardware_session_for(device_id)
                    .and_then(|session| session.snapshot().detected_chip)
                    .is_some()
                {
                    log::warn!(
                        "naming an ESP board through the legacy identity stamp: \
                         this session resolved no efuse MAC"
                    );
                }
                self.run_identity_stamp(device_id, name).await
            }
        }
    }

    /// The registry half of naming (design §5): the chosen name lands on
    /// the row keyed by the board's DERIVED uid, carrying the identity
    /// source with it, and the live card wears the name immediately. The
    /// merge keeps what earlier sightings recorded (board choice, push
    /// association) — and no re-pull is needed, because the identity was
    /// known at attach and adoption already ran there.
    async fn write_name_to_registry(
        &mut self,
        device_id: crate::RuntimeId,
        hardware_id: crate::app::places::HardwareId,
        name: String,
    ) -> Result<crate::app::places::DeviceIdentity, UiError> {
        let identity = crate::app::places::DeviceIdentity {
            uid: hardware_id.device_uid().to_string(),
            name,
        };
        let now = (self.now_secs)();
        let entry = self.registry_entry_for_session(device_id, &identity, now);
        let host = self.library_host()?;
        host.catalog(CatalogOp::UpsertRegisteredDevice(entry))
            .await
            .map_err(|error| self.library_error_with_name(error))?;
        if let Some(sync) = self
            .pool
            .device_session_mut(device_id)
            .ok()
            .and_then(crate::RuntimeSession::device_sync_mut)
        {
            sync.identity = Some(identity.clone());
        }
        self.request_library_refresh();
        self.mark_dirty();
        Ok(identity)
    }

    /// The naming FALLBACK for sessions with no silicon identity (D3/D6):
    /// host-class embedders and boards whose hello carries no efuse MAC.
    /// ESP-class provisioning writes the registry only — see
    /// [`Self::run_device_naming`].
    ///
    /// Mint the uid, write `/.lp/device.json` at the device's fs ROOT over
    /// the wire (identity is device-scoped, outside every project storage
    /// dir), register the device, and re-pull (adoption may now run for
    /// previously-anonymous content).
    async fn run_identity_stamp(
        &mut self,
        device_id: crate::RuntimeId,
        name: String,
    ) -> Result<crate::app::places::DeviceIdentity, UiError> {
        use lpc_model::AsLpPath;
        let identity = crate::app::places::DeviceIdentity {
            uid: lpc_history::PrefixedUid::mint(lpc_history::UidPrefix::Device, &(self.random)())
                .to_string(),
            name,
        };
        {
            let server = self.pool.device_session_mut(device_id)?.client_mut()?;
            let logs = server
                .fs_write(
                    crate::app::places::DEVICE_IDENTITY_PATH.as_path(),
                    &identity.to_json_bytes(),
                )
                .await?;
            self.record_logs(logs);
        }
        let now = (self.now_secs)();
        self.upsert_device_entry(device_id, &identity, now).await;
        self.refresh_device_sync_for(device_id).await;
        Ok(identity)
    }

    /// Write the chosen board's runtime manifest to the device's
    /// `/hardware.json` (board-selection D4): fs ROOT, same wire as the
    /// identity stamp above. The firmware's loader reads it at boot, so
    /// the pin map takes effect on the device's NEXT restart. The picker
    /// only offers boards with a checked-in runtime manifest; a display-
    /// only id reaching here degrades honestly instead of writing junk.
    async fn run_hardware_stamp(
        &mut self,
        device_id: crate::RuntimeId,
        board_id: &str,
    ) -> Result<(), UiError> {
        use lpc_model::AsLpPath;
        let manifest_json = lpa_boards::runtime_manifest_json(board_id).ok_or_else(|| {
            UiError::UnsupportedAction(format!(
                "board {board_id} has no checked-in runtime manifest"
            ))
        })?;
        {
            let server = self.pool.device_session_mut(device_id)?.client_mut()?;
            let logs = server
                .fs_write(
                    crate::app::places::DEVICE_HARDWARE_MANIFEST_PATH.as_path(),
                    manifest_json.as_bytes(),
                )
                .await?;
            self.record_logs(logs);
        }
        // The gallery remembers the board (roster cache, M6 reads it for
        // card art). Only an identified device has a registry row.
        if let Some(identity) = self
            .device_sync_for(device_id)
            .and_then(|sync| sync.identity.clone())
        {
            let now = (self.now_secs)();
            if let Ok(host) = self.library_host() {
                let mut entry = self.registry_entry_for_session(device_id, &identity, now);
                entry.board_id = Some(board_id.to_string());
                if let Err(error) = host.catalog(CatalogOp::UpsertRegisteredDevice(entry)).await {
                    log::warn!("device registry board upsert failed: {error}");
                }
                self.request_library_refresh();
            }
        }
        Ok(())
    }

    /// Push a library head to the device: hash-verified replace-and-load,
    /// then the push event + association. Identity lives at the device's
    /// fs root, so the storage-dir replace never touches it. The library
    /// side prefers the active handle (this tab owns it); otherwise a
    /// snapshot read + catalog transaction.
    async fn run_device_push(
        &mut self,
        device_id: crate::RuntimeId,
        device: &crate::app::places::DeviceIdentity,
        target: &DeployTarget,
    ) -> Result<(), UiError> {
        // 1. payload: live handle when the project is open here
        let payload = self.project.active_package_payload(&target.project_uid)?;
        let (files, local_hash) = match payload {
            Some(payload) => payload,
            None => {
                let host = self.library_host()?;
                let fs = host.catalog_snapshot().await.map_err(UiError::from)?;
                let store = crate::app::library::LibraryStore::read_only(fs);
                let uid = target
                    .project_uid
                    .parse()
                    .map_err(|e| UiError::MissingSession(format!("project uid: {e}")))?;
                let handle = store
                    .open(uid)
                    .map_err(|e| UiError::MissingSession(format!("library: {e}")))?;
                (
                    handle
                        .read_all_files()
                        .map_err(|e| UiError::MissingSession(format!("library: {e}")))?,
                    handle
                        .content_hash()
                        .map_err(|e| UiError::MissingSession(format!("library: {e}")))?,
                )
            }
        };

        // 2. hash-verified replace + load (the device runs it immediately)
        // into the storage dir the device actually uses, so one project
        // dir replaces in place (CLI uploads use dirs other than the
        // sim's default slot)
        {
            let storage_id = self
                .pool
                .device_session(device_id)
                .and_then(|session| session.device_storage_id().map(str::to_string))
                .unwrap_or_else(|| {
                    crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID.to_string()
                });
            let server = self.pool.device_session_mut(device_id)?.client_mut()?;
            let loaded = server
                .open_library_project(&storage_id, &files, &local_hash.to_string())
                .await?;
            self.record_logs(loaded.logs);
        }

        // 3. the push event + association (active handle first — M4b)
        let now = (self.now_secs)();
        let device_uid: lpc_history::PrefixedUid = device
            .uid
            .parse()
            .map_err(|e| UiError::MissingSession(format!("device uid: {e}")))?;
        let recorded_on_active =
            self.project
                .record_push_on_active(&target.project_uid, device_uid, local_hash, now)?;
        let host = self.library_host()?;
        if recorded_on_active {
            // association still goes through the registry (store root)
            let mut entry = self.registry_entry_for_session(device_id, device, now);
            entry.association = Some(lpc_history::DeviceAssociation {
                device: device_uid,
                project: target
                    .project_uid
                    .parse()
                    .map_err(|e| UiError::MissingSession(format!("project uid: {e}")))?,
                version: local_hash,
                at: now,
            });
            host.catalog(CatalogOp::UpsertRegisteredDevice(entry))
                .await
                .map_err(UiError::from)?;
        } else {
            host.catalog(CatalogOp::RecordPush {
                project_uid: target.project_uid.clone(),
                device: self.registry_entry_for_session(device_id, device, now),
                version: local_hash,
            })
            .await
            .map_err(|error| self.library_error_with_name(error))?;
        }

        // 4. the device now runs the pushed head
        self.refresh_device_sync_for(device_id).await;
        Ok(())
    }

    pub fn apply_console_command(&mut self, command: ConsoleCommand) {
        match command {
            ConsoleCommand::SetMinLevel(level) => self.log_filter.min_level = level,
            ConsoleCommand::SetOriginEnabled(origin, enabled) => {
                self.log_filter.set_origin_enabled(origin, enabled);
            }
            ConsoleCommand::Clear => self.logs.clear(),
            // Converted into a `DeviceOp::SetLogLevel` action at actor intake
            // (`CommandPlan::from_batch`); a stray direct call is a no-op
            // rather than a panic.
            ConsoleCommand::SetDeviceLogLevel(_) => return,
        }
        self.mark_dirty();
    }

    /// The current bounded log entries (unfiltered), oldest-first. Exposed for
    /// the actor and tests; the view carries the filtered console slice.
    pub fn logs(&self) -> Vec<UiLogEntry> {
        self.logs.to_vec()
    }

    pub async fn dispatch(&mut self, action: UiAction) -> UiResult {
        self.dispatch_with_updates(action, UxUpdateSink::noop())
            .await
    }

    pub async fn dispatch_with_updates(
        &mut self,
        action: UiAction,
        updates: UxUpdateSink,
    ) -> UiResult {
        updates.emit(UxUpdate::View(self.view()));
        let result = self.dispatch_inner(action, updates.clone()).await;
        // Device console lines observed by the session's event sink during
        // the action join the ring.
        let device_logs = self.device.take_pending_device_logs();
        self.record_logs(device_logs);
        // Release closed projects' locks and re-hydrate the gallery when
        // the action made either due (open/close/save/home ops).
        self.settle_library().await;
        // A dispatched action changes local state (project/device state, focus,
        // logs, or an error to surface), so the actor's next gate must emit.
        self.mark_dirty();
        updates.emit(UxUpdate::View(self.view()));
        result
    }

    /// A passive refresh tick driven under a progress deadline and cancel signal
    /// (the actor's passive-pull path).
    ///
    /// `Ok(None)` when there is nothing to refresh (no loaded project / no
    /// LightPlayer). Otherwise the [`ProjectRefreshOutcome`] tells the actor
    /// whether the read completed, was cancelled by a preempting command, or hit
    /// the quiet-gap deadline — so the actor can apply backoff or resume ticking
    /// without treating a clean cancel as a failure.
    pub async fn refresh_loaded_project_tick_gated<MakeTimer, Timer, Cancel>(
        &mut self,
        deadline: ProgressDeadline<MakeTimer, Timer>,
        cancel: &Cancel,
    ) -> Result<Option<ProjectRefreshOutcome>, UiError>
    where
        MakeTimer: FnMut(Duration) -> Timer,
        Timer: Future<Output = ()>,
        Cancel: CancelSignal + ?Sized,
    {
        if !self.project_is_loaded() || !self.has_lightplayer_state() {
            return Ok(None);
        }
        if !self.passive_refresh_due() {
            return Ok(Some(ProjectRefreshOutcome::NotDue));
        }
        self.sync_lens_probe_policy();
        let outcome = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project
                .refresh_project_gated(server, deadline, cancel)
                .await?
        };
        if let ProjectRefreshOutcome::Synced(sync) = &outcome {
            self.record_project_sync_run(sync);
        }
        // Device console lines pumped during the pull join the ring (the
        // actor path does not pass through the dispatch wrapper).
        let device_logs = self.device.take_pending_device_logs();
        self.record_logs(device_logs);
        Ok(Some(outcome))
    }

    pub fn mark_passive_project_refresh_failed(&mut self, message: impl Into<String>) {
        self.project.mark_project_sync_failed(message);
        // A sync failure changes the project pane's status even if the revision
        // did not move, so the next change gate must emit it.
        self.mark_dirty();
    }

    async fn dispatch_inner(&mut self, action: UiAction, updates: UxUpdateSink) -> UiResult {
        // Actions can move the lens (connect/open flows) or trigger reads;
        // keep the project controller's probe policy tracking the lens.
        self.sync_lens_probe_policy();
        let node_id = action.node_id().clone();
        let device_node_id = self.device.node_id();
        let project_node_id = self.project.node_id();

        if node_id.as_str() == HOME_NODE_ID {
            let op = action.into_op::<HomeOp>()?;
            return self.execute_home_op(op, updates).await;
        }
        if node_id.as_str() == crate::AgentController::NODE_ID {
            let op = action.into_op::<crate::AgentOp>()?;
            return self.execute_agent_op(op).await;
        }
        if node_id.as_str() == DEPLOY_NODE_ID {
            let op = action.into_op::<DeployOp>()?;
            return self.execute_deploy_op(op, updates).await;
        }
        if node_id == device_node_id {
            let op = action.into_op::<DeviceOp>()?;
            return self.execute_device_op(op, updates).await;
        }
        if node_id == project_node_id {
            // Slot edits and node-level reverts target the project node too
            // (the op carries the full slot/node address), so route by op
            // type before the ProjectOp downcast.
            if action.op_as::<SlotEditOp>().is_some() {
                let op = action.into_op::<SlotEditOp>()?;
                return self.execute_slot_edit_op(op).await;
            }
            if action.op_as::<AssetEditOp>().is_some() {
                let op = action.into_op::<AssetEditOp>()?;
                return self.execute_asset_edit_op(op).await;
            }
            if action.op_as::<AssetContentFetchOp>().is_some() {
                let op = action.into_op::<AssetContentFetchOp>()?;
                return self.execute_asset_content_fetch(op).await;
            }
            if action.op_as::<crate::PatchVerbOp>().is_some() {
                let op = action.into_op::<crate::PatchVerbOp>()?;
                return self.execute_patch_verb_op(op).await;
            }
            if action.op_as::<crate::EditorMetaOp>().is_some() {
                let op = action.into_op::<crate::EditorMetaOp>()?;
                return self.execute_editor_meta_op(op).await;
            }
            if action.op_as::<crate::EditorMetaFetchOp>().is_some() {
                let op = action.into_op::<crate::EditorMetaFetchOp>()?;
                return self.execute_editor_meta_fetch(op).await;
            }
            if action.op_as::<NodeRevertOp>().is_some() {
                let op = action.into_op::<NodeRevertOp>()?;
                return self.execute_node_revert_op(op).await;
            }
            if action.op_as::<NodeClearDebugOp>().is_some() {
                let op = action.into_op::<NodeClearDebugOp>()?;
                return self.execute_node_clear_debug_op(op).await;
            }
            if action.op_as::<PatchPulseOp>().is_some() {
                let op = action.into_op::<PatchPulseOp>()?;
                return self.execute_patch_pulse_op(op).await;
            }
            if action.op_as::<PlaylistActivateOp>().is_some() {
                let op = action.into_op::<PlaylistActivateOp>()?;
                return self.execute_playlist_activate_op(op).await;
            }
            if action.op_as::<PanelWriteOp>().is_some() {
                let op = action.into_op::<PanelWriteOp>()?;
                return self.execute_panel_write_op(op).await;
            }
            if action.op_as::<PanelClearOp>().is_some() {
                let op = action.into_op::<PanelClearOp>()?;
                return self.execute_panel_clear_op(op).await;
            }
            if action.op_as::<PanelAutoSaveOp>().is_some() {
                let op = action.into_op::<PanelAutoSaveOp>()?;
                return self.execute_panel_auto_save_op(op).await;
            }
            if action.op_as::<NodeCreateOp>().is_some() {
                let op = action.into_op::<NodeCreateOp>()?;
                return self.execute_node_create_op(op).await;
            }
            if action.op_as::<NodeRemoveOp>().is_some() {
                let op = action.into_op::<NodeRemoveOp>()?;
                return self.execute_node_remove_op(op).await;
            }
            if action.op_as::<NodeCopyOp>().is_some() {
                let op = action.into_op::<NodeCopyOp>()?;
                return self.execute_node_copy_op(op).await;
            }
            if action.op_as::<ModuleExportOp>().is_some() {
                let op = action.into_op::<ModuleExportOp>()?;
                return self.execute_module_export_op(op).await;
            }
            if action.op_as::<NodePasteOp>().is_some() {
                let op = action.into_op::<NodePasteOp>()?;
                return self.execute_node_paste_op(op).await;
            }
            if action.op_as::<NodeImportOp>().is_some() {
                let op = action.into_op::<NodeImportOp>()?;
                return self.execute_node_import_op(op).await;
            }
            let op = action.into_op::<ProjectOp>()?;
            return self.execute_project_op(op, updates).await;
        }
        if node_id.is_descendant_of(&project_node_id) {
            // Editor actions (currently only `Focus`) are local-only: they
            // complete synchronously in the controller. The old bolt-on
            // `refresh_project` network round-trip after every editor action is
            // gone (P3); the next passive `RefreshTick` picks up the changed
            // probe set, which is already focus-scoped via
            // `node_subscribes_products`. This keeps focus off the network path.
            let outcome = self
                .project
                .dispatch_editor_action(action, updates.clone())
                .await?;
            updates.emit(UxUpdate::View(self.view()));
            return Ok(outcome);
        }
        Err(crate::UiError::UnsupportedAction(format!(
            "unknown UX node {node_id}",
        )))
    }

    async fn execute_device_op(&mut self, op: DeviceOp, updates: UxUpdateSink) -> UiResult {
        match op {
            DeviceOp::DisconnectDevice { target } => {
                let id = self.resolve_device_target(&target)?;
                self.disconnect_device(id).await
            }
            DeviceOp::StopSimulator => self.stop_simulator().await,
            DeviceOp::DisconnectLightPlayer { target } => {
                self.disconnect_lightplayer(&target).await
            }
            // NOT a card op and NOT device-targeted: the console's
            // selector shows the LENS session's level, so the request goes
            // to the runtime whose console the user is looking at — which
            // may be the sim. Nothing to target (D1a, 2026-08-03).
            DeviceOp::SetLogLevel { level } => self.set_device_log_level(level).await,
            DeviceOp::ResetDevice { target } => {
                let id = self.resolve_device_target(&target)?;
                self.reset_device(id, updates).await
            }
            DeviceOp::ConnectLightPlayer { target } => {
                // The lens fallback is the sim's reconnect: `Ambient`
                // resolves no device when only the sim is attached, and
                // the lens is then the runtime to attach to.
                let id = match self.resolve_device_target(&target) {
                    Ok(id) => id,
                    Err(error) => self.pool.lens().ok_or(error)?,
                };
                self.connect_server_from_link(id, updates).await
            }
            DeviceOp::ProvisionFirmware {
                target,
                setup_name,
                board_id,
            } => {
                let id = self.resolve_device_target(&target)?;
                self.provision_firmware(id, updates, setup_name, board_id)
                    .await
            }
            DeviceOp::WipeProject { target } => {
                let id = self.resolve_device_target(&target)?;
                self.wipe_project(id).await
            }
            DeviceOp::ResetToBlank { target } => {
                let id = self.resolve_device_target(&target)?;
                self.reset_to_blank(id, updates).await
            }
            DeviceOp::BootSafeOnce { target } => {
                let id = self.resolve_device_target(&target)?;
                self.boot_safe_once(id, updates).await
            }
            DeviceOp::BackUpFilesystem { target } => {
                let id = self.resolve_device_target(&target)?;
                self.back_up_filesystem(id, updates).await
            }
            DeviceOp::ProbeBootloaderMode { card_key, flow } => {
                let id = self.resolve_device_target(&crate::DeviceTarget::card(&card_key))?;
                self.probe_bootloader_mode(id, card_key, flow).await
            }
            DeviceOp::RefreshConnections => {
                // Drop the session (no provider close) + catalog refresh.
                self.device.refresh_provider_catalog();
                self.pool.clear();
                self.project.reset();
                Ok(UiNotices::new().with_notice(UiNotice::info("Connection catalog refreshed")))
            }
            DeviceOp::OpenProviderForRecovery { provider_id } => {
                self.open_provider_link_only(provider_id, updates).await
            }
            DeviceOp::OpenProvider { provider_id } => {
                let outcome = self.device.open_provider(provider_id).await;
                self.settle_connect_outcome(runtime_kind_for(provider_id), outcome, updates)
                    .await
            }
            DeviceOp::ConnectEndpoint {
                provider_id,
                endpoint_id,
            } => {
                let outcome = self.device.connect_endpoint(provider_id, endpoint_id).await;
                self.settle_connect_outcome(runtime_kind_for(provider_id), outcome, updates)
                    .await
            }
            // One-click reconnect (M1): no activity chip up front — the flow
            // may fall back to the browser's port chooser, which blocks like
            // the browser-serial OpenProvider path.
            DeviceOp::ReconnectDevice { uid } => {
                let outcome = self.device.reconnect_granted_device(uid).await;
                self.settle_connect_outcome(crate::RuntimeKind::Device, outcome, updates)
                    .await
            }
            DeviceOp::AutoConnect => self.run_auto_connect(updates).await,
        }
    }

    /// D32 auto-connect (M6): the attach sweep dispatched at app load and
    /// on the serial hotplug event. Attach + pull + show, nothing else —
    /// and strictly idempotent: a live device session (the `#/device/…`
    /// route may already have connected) or a busy connect flow makes it
    /// a silent no-op. Card attribution targets the most-recently-seen
    /// remembered device (best effort; the hello reconciles identity).
    async fn run_auto_connect(&mut self, updates: UxUpdateSink) -> UiResult {
        if self.pool.oldest_device_session().is_some() {
            self.record_device_event(
                None,
                None,
                DeviceEventKind::Sweep {
                    disposition: "skipped-device-attached".to_string(),
                },
            );
            return Ok(UiNotices::new());
        }
        if matches!(
            self.device.flow_state(),
            ConnectFlowState::DiscoveringEndpoints { .. }
                | ConnectFlowState::Connecting { .. }
                | ConnectFlowState::Retrying { .. }
        ) {
            self.record_device_event(
                None,
                None,
                DeviceEventKind::Sweep {
                    disposition: "skipped-flow-busy".to_string(),
                },
            );
            return Ok(UiNotices::new());
        }
        self.record_device_event(
            None,
            None,
            DeviceEventKind::Sweep {
                disposition: "ran".to_string(),
            },
        );
        let pending_uid = self.most_recently_seen_device_uid();
        let outcome = self.device.auto_connect_granted(pending_uid).await;
        self.settle_connect_outcome(crate::RuntimeKind::Device, outcome, updates)
            .await
    }

    /// The remembered device the auto-connect sweep attributes its
    /// narration to: the hydrated gallery's most recently seen card.
    fn most_recently_seen_device_uid(&self) -> Option<String> {
        let inputs = self.home_inputs.as_ref()?;
        inputs
            .devices
            .iter()
            .max_by(|a, b| {
                let key = |card: &crate::UiDeviceCard| match &card.state {
                    crate::RosterCardState::Offline { last_seen_at } => {
                        last_seen_at.unwrap_or(f64::NEG_INFINITY)
                    }
                    _ => f64::INFINITY,
                };
                key(a)
                    .partial_cmp(&key(b))
                    .unwrap_or(core::cmp::Ordering::Equal)
            })
            .and_then(|card| card.uid.clone())
    }

    /// The quiet periodic retry for a port held elsewhere (D32): runs on
    /// the tick cadence, re-attempting the granted attach at most every
    /// [`PORT_HELD_RETRY_SECS`]. Leaves every other flow state alone.
    pub async fn run_due_connect_retry(&mut self) {
        if !matches!(self.device.flow_state(), ConnectFlowState::PortHeld { .. }) {
            self.port_held_retry_at = None;
            return;
        }
        let now = (self.now_secs)();
        match self.port_held_retry_at {
            None => self.port_held_retry_at = Some(now + PORT_HELD_RETRY_SECS),
            Some(due) if now >= due => {
                self.port_held_retry_at = Some(now + PORT_HELD_RETRY_SECS);
                let _ = self.run_auto_connect(UxUpdateSink::noop()).await;
                self.mark_dirty();
            }
            Some(_) => {}
        }
    }

    /// Sim crash detection + guarded auto-reboot, riding the tick cadence
    /// like [`Self::run_due_connect_retry`] (poisoned-instance defect: a
    /// panic escaping the worker's panic=abort wasm instance condemns it;
    /// the link layer reports it as a sticky per-session fatal message).
    ///
    /// Detection is edge-triggered — a sim session not yet marked
    /// [`ServerFailureKind::SimCrashed`] whose connector reports a fatal —
    /// so the recovery decision runs once per crash. When the flap guard
    /// allows, the dead session is torn down (the Worker terminates) and
    /// the recorded [`SimLoadedProject`](crate::SimLoadedProject) is
    /// reopened through the normal open flow; otherwise the session stays
    /// Failed and the card offers manual restart (the open flow tears a
    /// crashed session down itself, see `open_from_home_inner`).
    pub async fn run_due_sim_crash_recovery(&mut self) {
        let Some(sim_id) = self.detect_sim_crash() else {
            return;
        };
        let now = (self.now_secs)();
        if !sim_crash_reboot_allowed(self.sim_crash_reboot_at, now, SIM_CRASH_REBOOT_GUARD_SECS) {
            self.push_log(UiLogDraft::new(
                UiLogLevel::Error,
                UiLogOrigin::Studio,
                "Simulator keeps crashing; not restarting automatically. \
                 Reopen the project to try again.",
            ));
            self.mark_dirty();
            return;
        }
        self.sim_crash_reboot_at = Some(now);
        let loaded = self
            .pool
            .session(sim_id)
            .and_then(|session| session.sim_loaded_project().cloned());
        self.teardown_crashed_sim(sim_id).await;
        // The crashed session's project still holds its host-side tab lock
        // (quiesce parks it for the settle points, which run at batch end —
        // too late for this same-batch reopen). Release it now so the
        // reopen can lock the project again.
        self.project.release_closed_library_projects().await;
        self.push_log(UiLogDraft::new(
            UiLogLevel::Warn,
            UiLogOrigin::Studio,
            "Simulator crashed and was restarted; unsaved changes may be lost.",
        ));
        if let Some(project) = loaded {
            // Reboot with the last-known project: the library head — the
            // crashed instance held the applied-but-unsaved overlay, which
            // died with it either way.
            if let Err(error) = self
                .open_from_home(PendingOpen::Package(project.uid), UxUpdateSink::noop())
                .await
            {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Error,
                    UiLogOrigin::Studio,
                    format!("simulator restart failed: {error}"),
                ));
            }
        }
        self.mark_dirty();
    }

    /// Edge-detect a crashed sim worker: the sim session's connector
    /// reports a sticky instance-fatal message and the session is not yet
    /// marked crashed. Marks it Failed with
    /// [`ServerFailureKind::SimCrashed`], surfaces the primary panic on
    /// the console, and returns the session id.
    fn detect_sim_crash(&mut self) -> Option<crate::RuntimeId> {
        let session = self.pool.sim_session()?;
        if matches!(
            session.server_state(),
            ServerState::Failed {
                kind: ServerFailureKind::SimCrashed,
                ..
            }
        ) {
            return None;
        }
        let RuntimePayload::Sim(sim) = session.payload() else {
            return None;
        };
        let message = sim.connector.session_fatal(&sim.session.id)?;
        let sim_id = session.id();
        self.push_log(UiLogDraft::new(
            UiLogLevel::Error,
            UiLogOrigin::Studio,
            format!("Simulator crashed: {message}"),
        ));
        if let Some(session) = self.pool.session_mut(sim_id) {
            session.fail_with_kind(message, ServerFailureKind::SimCrashed);
        }
        self.mark_dirty();
        Some(sim_id)
    }

    /// Tear down a crashed sim session: quiesce the editor lens when it
    /// sits on it, take the session out of the pool, and close its payload
    /// (terminating the dead Worker). Close errors are logged, not fatal —
    /// the worker is already dead.
    async fn teardown_crashed_sim(&mut self, sim_id: crate::RuntimeId) {
        if self.pool.lens() == Some(sim_id) {
            self.quiesce_lens();
        }
        if let Some(session) = self.pool.remove_kind(crate::RuntimeKind::Sim) {
            if let Err(error) = self.device.disconnect(Some(session.into_payload())).await {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!("crashed simulator teardown: {error}"),
                ));
            }
        }
    }

    /// Land a connect flow's outcome in the pool. P2 capacity semantics:
    /// only the KIND being connected is touched — `Opened`, `Cancelled`,
    /// and failures clear that kind's slot (matching the retired
    /// empty-slot endings), a live payload installs kind-aware (the other
    /// kind's session stays attached).
    async fn settle_connect_outcome(
        &mut self,
        kind: crate::RuntimeKind,
        outcome: Result<DeviceOpenOutcome, UiError>,
        updates: UxUpdateSink,
    ) -> UiResult {
        // Multi-device M3: a DEVICE connect that ends without a session no
        // longer clears the kind's slot — with several boards attachable,
        // opening the picker (or a cancelled/failed attempt at an
        // ADDITIONAL board) must not tear down a live session. The ≤1-era
        // "empty-slot ending" semantics survive for the sim (capacity 1),
        // and a RECONNECT of an existing endpoint still replaces its own
        // session at install time (the pool's per-endpoint rule).
        let clear_on_empty_ending = kind != crate::RuntimeKind::Device;
        match outcome {
            Ok(DeviceOpenOutcome::Opened) => {
                if clear_on_empty_ending {
                    self.clear_connect_slot(kind);
                }
                Ok(UiNotices::new())
            }
            Ok(DeviceOpenOutcome::Cancelled { message }) => {
                if clear_on_empty_ending {
                    self.clear_connect_slot(kind);
                }
                Ok(UiNotices::new().with_notice(UiNotice::info(message)))
            }
            Ok(DeviceOpenOutcome::Connected { payload, logs }) => {
                // Chrome's chooser cannot say which port is already
                // connected (identical VID:PID, no OS path), so mis-picks
                // are routine with several boards. The pool's per-endpoint
                // rule makes the re-pick a clean reconnect; this notice
                // makes it a VISIBLE one, so "I meant to add the other
                // board" is diagnosable at a glance.
                let repicked_live = kind == crate::RuntimeKind::Device
                    && payload.link_session().is_some_and(|link| {
                        self.pool.device_sessions().any(|session| {
                            session
                                .payload()
                                .link_session()
                                .is_some_and(|existing| existing.endpoint_id == link.endpoint_id)
                        })
                    });
                let id = self.install_session(payload).await?;
                // The connect's own link lines belong to the session it
                // just opened (D42), so they are recorded AFTER the install
                // that gives them a session to land on — the wizard's
                // terminal reads that tail, and the ring it used to reach
                // is not on any card.
                self.record_session_logs(id, logs);
                let outcome = self.attach_runtime(id, updates).await?;
                if repicked_live {
                    Ok(outcome.with_notice(UiNotice::info(
                        "That port was already connected — reconnected it. To add a \
                         different board, pick another port in the chooser.",
                    )))
                } else {
                    Ok(outcome)
                }
            }
            Ok(DeviceOpenOutcome::SoftFailed) => {
                // M6: the ladder's honest ending lives on the CARD
                // (PortHeld/Unresponsive flow states → card evidence);
                // nothing toasts, nothing errors.
                if clear_on_empty_ending {
                    self.clear_connect_slot(kind);
                }
                self.mark_dirty();
                Ok(UiNotices::new())
            }
            Err(error) => {
                if clear_on_empty_ending {
                    self.clear_connect_slot(kind);
                }
                Err(error)
            }
        }
    }

    /// Clear a kind's pool slot after a connect ended without a session.
    ///
    /// `remove_kind` alone drops the lens ID with the session but leaves
    /// the MIRROR dressed — `project.state` stays `Ready`, so the shell
    /// keeps rendering panes for an editor with nothing behind it and no
    /// lens card. Quiescing first is the pairing every other teardown
    /// path uses (`teardown_crashed_sim`, `detach_lens`,
    /// `open_provider_link_only`); it returns the user to the gallery,
    /// which is the honest reading of "the connect ended with no
    /// session". Defect
    /// `docs/defects/2026-07-28-retired-device-pane-still-reachable.md`.
    fn clear_connect_slot(&mut self, kind: crate::RuntimeKind) {
        if self
            .pool
            .lens_session()
            .is_some_and(|session| session.kind() == kind)
        {
            self.quiesce_lens();
        }
        if let Some(session) = self.pool.remove_kind(kind) {
            self.record_device_event(
                Some(&session.id().to_string()),
                None,
                DeviceEventKind::Pool {
                    action: "clear-slot".to_string(),
                    detail: format!("{kind:?}"),
                },
            );
        }
    }

    /// Install a connected payload into the pool under the capacity
    /// policy. A refusal (an operation is still in flight on the session
    /// that would be replaced — DQ-A swap semantics) closes the fresh
    /// payload so its session doesn't leak, then surfaces the refusal.
    ///
    /// When the install replaces the session the lens is on (a same-kind
    /// re-connect under an open editor), the mirror quiesces first — the
    /// replacement inherits the lens with a clean slate. A lens on the
    /// OTHER kind is untouched (install observes, P3).
    async fn install_session(
        &mut self,
        payload: crate::RuntimePayload,
    ) -> Result<crate::RuntimeId, UiError> {
        let install_detail = format!("{:?}", payload.kind());
        let install_endpoint = payload
            .link_session()
            .map(|session| session.endpoint_id.as_str().to_string());
        // Which session (if any) this install is about to REPLACE at the
        // same endpoint — read before the install, because that is the
        // only moment both ids exist. A card-owned op flow rides across
        // (see `migrate_card_op`).
        let replaced = payload
            .link_session()
            .and_then(|link| self.pool.endpoint_session(&link.endpoint_id));
        // The single-session policy (module doc), at the funnel every
        // open and connect converges on: everything except the session
        // the pool is about to replace at this endpoint goes first, and
        // an operation in flight refuses the whole install — ending the
        // same way the pool's own refusal does, with the fresh payload
        // closed rather than leaked.
        if let Err(error) = self.enforce_single_session(replaced).await {
            close_runtime_payload(payload).await;
            return Err(error);
        }
        // Read AFTER the teardown: the question is whether THIS install
        // replaces the session the editor is a lens on, and the policy
        // may just have taken that session away (in which case the
        // teardown already reset the mirror).
        let lens_replaced = self
            .pool
            .lens_session()
            .is_some_and(|session| session.kind() == payload.kind());
        match self.pool.install(payload) {
            Ok(id) => {
                self.migrate_card_op(replaced, id);
                self.record_device_event(
                    Some(&id.to_string()),
                    install_endpoint.as_deref(),
                    DeviceEventKind::Pool {
                        action: "install".to_string(),
                        detail: install_detail,
                    },
                );
                if lens_replaced {
                    self.project.reset();
                }
                Ok(id)
            }
            Err(refusal) => {
                let message = refusal.message;
                close_runtime_payload(refusal.payload).await;
                Err(UiError::UnsupportedAction(message))
            }
        }
    }

    /// The single-session policy's gate (module doc), run at every seam
    /// that would otherwise leave the tab with a second runtime: the
    /// install funnel above, and the sim-reuse open that never reaches
    /// it.
    ///
    /// `keep` is the session this gesture means to END UP on — the sim
    /// an open is about to reuse, or the same-endpoint session the pool
    /// itself is about to replace. That second case is why the pool's
    /// replace is left alone rather than pre-empted here: a replug comes
    /// back on its own endpoint carrying a card-owned op flow that has
    /// to ride across (`migrate_card_op`), and closing the outgoing
    /// session ourselves would drop the "plug it back in" instruction at
    /// the moment the user obeyed it.
    ///
    /// An operation in flight anywhere refuses the whole gesture and
    /// names it: teardown mid-flash is not a thing that can be done
    /// honestly, so the user finishes or cancels it first.
    async fn enforce_single_session(
        &mut self,
        keep: Option<crate::RuntimeId>,
    ) -> Result<(), UiError> {
        if self.multi_session_allowed() {
            return Ok(());
        }
        if let Some(session) = self.pool.sessions().find(|session| session.op_in_flight()) {
            let label = session
                .operation_label()
                .unwrap_or("A device operation")
                .to_string();
            let name = self.session_display_name(session);
            return Err(UiError::UnsupportedAction(format!(
                "{label} in progress on {name} — finish or cancel it first."
            )));
        }
        let doomed: Vec<crate::RuntimeId> = self
            .pool
            .sessions()
            .map(crate::RuntimeSession::id)
            .filter(|id| Some(*id) != keep)
            .collect();
        for id in doomed {
            self.close_session_for_policy(id).await;
        }
        Ok(())
    }

    /// Whether this controller may hold several sessions at once. Never,
    /// in the shipped app — see [`Self::multi_session_for_test`].
    #[cfg(not(test))]
    fn multi_session_allowed(&self) -> bool {
        false
    }

    #[cfg(test)]
    fn multi_session_allowed(&self) -> bool {
        self.multi_session_for_test
    }

    /// Tear ONE session down for the policy above: the teardown halves of
    /// [`Self::stop_simulator`] and [`Self::disconnect_device`] without
    /// their connect-flow epilogue.
    ///
    /// That epilogue is why this is a helper rather than a call to either
    /// verb. Both hand the connect flow back to the provider catalog when
    /// they leave the pool empty, and this teardown runs INSIDE an
    /// arriving connect whose flow is already `Connected`
    /// (`DeviceController::connect_endpoint` sets it before the payload
    /// ever reaches the pool) — resetting the catalog here would throw
    /// the arriving session's own connect state away and leave the
    /// gallery showing a provider picker over a live board.
    async fn close_session_for_policy(&mut self, id: crate::RuntimeId) {
        // The mirror quiesces when the editor was a lens on this session:
        // pending logs drained onto it, project reset, lens released.
        if self.pool.lens() == Some(id) {
            self.quiesce_lens();
        }
        let Some(mut session) = self.pool.remove_session(id) else {
            return;
        };
        let kind = session.kind();
        let pending = session.take_pending_logs();
        self.record_logs(pending);
        self.record_device_event(
            Some(&id.to_string()),
            None,
            DeviceEventKind::Pool {
                action: "single-session".to_string(),
                detail: format!("{kind:?}"),
            },
        );
        // A failed close still loses the session — the pool is the truth
        // about attachment — and the failure lands in the ring as a
        // warning, exactly as stop-sim's does.
        match session.into_payload() {
            crate::RuntimePayload::Sim(sim) => {
                if let Err(error) = sim.connector.close(&sim.session.id).await {
                    self.push_log(UiLogDraft::new(
                        UiLogLevel::Warn,
                        UiLogOrigin::Studio,
                        format!("simulator session close reported: {error}"),
                    ));
                }
            }
            crate::RuntimePayload::Device(handle) => {
                if let Err(error) = handle.close().await {
                    self.push_log(UiLogDraft::new(
                        UiLogLevel::Warn,
                        UiLogOrigin::Link,
                        format!("device session close reported: {}", error.message()),
                    ));
                }
            }
        }
        self.mark_dirty();
    }

    /// A session's human name for policy copy: the simulator says so, a
    /// board wears its stamped name, then its registry row's — and a
    /// board nobody has named yet is still a board, so the fallback is a
    /// noun rather than an id the user has never seen.
    fn session_display_name(&self, session: &crate::RuntimeSession) -> String {
        if session.is_sim() {
            return "the simulator".to_string();
        }
        let stamped = session
            .device_sync()
            .and_then(|sync| sync.identity.as_ref())
            .map(|identity| identity.name.clone())
            .filter(|name| !name.is_empty());
        stamped
            .or_else(|| {
                let uid = session.device_uid()?;
                self.home_inputs
                    .as_ref()?
                    .registered
                    .iter()
                    .find(|device| device.uid == uid)
                    .map(|device| device.name.clone())
                    .filter(|name| !name.is_empty())
            })
            .unwrap_or_else(|| "the connected board".to_string())
    }

    async fn execute_home_op(&mut self, op: HomeOp, updates: UxUpdateSink) -> UiResult {
        match op {
            HomeOp::OpenPackage { key } => {
                return self
                    .open_from_home(PendingOpen::Package(key), updates)
                    .await;
            }
            HomeOp::OpenExample { id } => {
                return self.open_from_home(PendingOpen::Example(id), updates).await;
            }
            HomeOp::OpenSharedTransient {
                uid,
                name,
                package_files,
                history_files,
            } => {
                return self
                    .open_from_home(
                        PendingOpen::SharedTransient {
                            uid,
                            name,
                            package_files,
                            history_files,
                        },
                        updates,
                    )
                    .await;
            }
            HomeOp::CreateProject { template } => {
                // Create-and-open (the D17 deviation, 2026-07-27): the
                // package lands in the library — slugged/dated/deduped by
                // the store from the template's label; rename lives on the
                // card kebab — then opens like any card, so the user lands
                // in the editor with something to do next. `Blank` sends no
                // files at all, so the historical path is untouched.
                let files = crate::app::home::template_project_files(template)?;
                let outcome = self
                    .run_catalog_op(CatalogOp::Create {
                        name: template.default_project_name().to_string(),
                        files,
                    })
                    .await?;
                let created = outcome.summary.ok_or_else(|| {
                    UiError::MissingSession("create produced no package".to_string())
                })?;
                return self
                    .open_from_home(PendingOpen::Package(created.uid.to_string()), updates)
                    .await;
            }
            HomeOp::CreateFromPattern { uid, export, name } => {
                // Read the SOURCE through a read-only snapshot (no lock —
                // it may be open in another tab), compose the workbench
                // around its export, then create-and-open like any other
                // template. One catalog transaction, at the end.
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Err(UiError::UnsupportedAction(
                        "a project name cannot be empty".to_string(),
                    ));
                }
                let source_files = self.read_library_package_files(&uid).await?;
                let files =
                    crate::app::home::project_files_from_export(&source_files, &export, &name)?;
                let outcome = self
                    .run_catalog_op(CatalogOp::Create {
                        name,
                        files: Some(files),
                    })
                    .await?;
                let created = outcome.summary.ok_or_else(|| {
                    UiError::MissingSession("create produced no package".to_string())
                })?;
                return self
                    .open_from_home(PendingOpen::Package(created.uid.to_string()), updates)
                    .await;
            }
            HomeOp::RenamePackage { uid, name } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(UiError::UnsupportedAction(
                        "a project name cannot be empty".to_string(),
                    ));
                }
                let outcome = self
                    .run_catalog_op(CatalogOp::Rename {
                        uid,
                        new_slug: name.to_string(),
                    })
                    .await?;
                let renamed = outcome
                    .summary
                    .map(|summary| summary.slug)
                    .unwrap_or_else(|| name.to_string());
                Ok(UiNotices::new().with_notice(UiNotice::info(format!("Renamed to {renamed}"))))
            }
            HomeOp::DuplicatePackage { uid } => {
                let outcome = self.run_catalog_op(CatalogOp::Duplicate { uid }).await?;
                let copy = outcome
                    .summary
                    .map(|summary| summary.slug)
                    .unwrap_or_default();
                Ok(UiNotices::new().with_notice(UiNotice::info(format!("Duplicated as {copy}"))))
            }
            HomeOp::DeletePackage { uid } => {
                self.run_catalog_op(CatalogOp::Delete { uid }).await?;
                Ok(UiNotices::new().with_notice(UiNotice::info("Project deleted")))
            }
            HomeOp::ImportZip { file_name, bytes } => {
                let outcome = self
                    .run_catalog_op(CatalogOp::ImportZip {
                        file_name,
                        bytes: bytes.0,
                    })
                    .await?;
                Ok(UiNotices::new()
                    .with_notice(UiNotice::info(import_message("Imported", &outcome))))
            }
            HomeOp::ImportJson { text } => {
                let outcome = self.run_catalog_op(CatalogOp::ImportJson { text }).await?;
                Ok(
                    UiNotices::new()
                        .with_notice(UiNotice::info(import_message("Pasted", &outcome))),
                )
            }
            HomeOp::RenameDevice { uid, name } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Err(UiError::UnsupportedAction(
                        "a device name cannot be empty".to_string(),
                    ));
                }
                // registry first — it is the naming truth (D34); a failed
                // live write-back below heals on the next connect
                self.run_catalog_op(CatalogOp::RenameRegisteredDevice {
                    uid: uid.clone(),
                    name: name.clone(),
                })
                .await?;
                self.write_back_live_identity_name(&uid, &name).await?;
                Ok(UiNotices::new()
                    .with_notice(UiNotice::info(format!("This device is now \"{name}\""))))
            }
            HomeOp::ForgetDevice { uid } => self.forget_device(uid).await,
            HomeOp::NameDevice { target, name } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Err(UiError::UnsupportedAction(
                        "a device name cannot be empty".to_string(),
                    ));
                }
                // the Needs-a-name card's inline form (D14 gently insists
                // upstream): the name lands on the registry row keyed by
                // the board's own derived uid — nothing is written to the
                // board (design §5) — with the legacy stamp standing in
                // only where there is no silicon to anchor to
                let device_id = self.resolve_device_target(&target)?;
                let identity = self.run_device_naming(device_id, name).await?;
                Ok(UiNotices::new().with_notice(UiNotice::info(format!(
                    "This device is now \"{}\"",
                    identity.name
                ))))
            }
            HomeOp::StartSetup { sim } => self.start_setup(sim, updates).await,
            HomeOp::Setup(gesture) => self.advance_setup(gesture, updates).await,
            HomeOp::CardUi(op) => {
                self.apply_card_ui_op(op);
                Ok(UiNotices::new())
            }
        }
    }

    /// Forget a device: revoke the browser's persistent access to the
    /// board, then delete its registry row.
    ///
    /// The revocation is what makes forgetting STICK (G2 walk,
    /// 2026-08-05). Deleting the row alone was silently undone by the next
    /// page load: the Web Serial grant outlives the page, so the app
    /// re-enumerated the granted port, auto-probed it, re-derived the same
    /// `dev` uid from its efuse MAC (identity design §3 — the uid is
    /// silicon, not a stored fact), and the sighting write recreated the
    /// row the user had just deleted.
    ///
    /// A grant can only be revoked through the endpoint that holds it, and
    /// only a LIVE session names its endpoint — which physical board a
    /// grant belongs to is unknowable until something connects through it.
    /// So an offline device is registry-only: its row goes, and any grant
    /// it may still hold survives (harmless on its own — a grant with no
    /// row re-registers only if the user reconnects that board). Forgetting
    /// the board in front of you does the whole job.
    async fn forget_device(&mut self, uid: String) -> UiResult {
        // Capture the link BEFORE the disconnect tears the session down.
        let live = self.device_id_for_card_key(&uid).and_then(|id| {
            let session = self.pool.device_session(id)?.hardware_session()?;
            Some((id, session.connector(), session.session().endpoint_id))
        });
        let mut notices = UiNotices::new();
        if let Some((id, connector, endpoint_id)) = live {
            // Order matters: the grant cannot be revoked out from under an
            // open port, and `forget()` on a live SerialPort is exactly the
            // shape that leaves a wedged reader behind.
            self.disconnect_device(id).await?;
            // A FAILED revocation keeps the registry row: the row is the
            // only visible trace of a device the app can still reach, and
            // deleting it here would stage exactly the disappear-then-
            // reappear the fix exists to end. The user sees the device
            // still listed — which is the truth — and can try again.
            let revoked = connector
                .forget_endpoint(&endpoint_id)
                .await
                .map_err(|error| {
                    UiError::Link(format!(
                        "couldn't release this device's port, so it stays in the list: {error}"
                    ))
                })?;
            if !revoked && connector.kind() == LinkProviderKind::BrowserSerialEsp32 {
                // A browser that will NEVER give the grant back (no
                // `SerialPort.forget()` before Chrome 103) is different
                // from a revocation that failed: retrying cannot help, so
                // the row goes and the warning names the manual way out.
                notices = notices.with_notice(UiNotice::warning(
                    "This browser cannot release its serial-port permission — \
                     revoke it in the browser's site settings, or this device \
                     reappears when you reload",
                ));
            }
        }
        self.run_catalog_op(CatalogOp::ForgetRegisteredDevice { uid })
            .await?;
        Ok(notices.with_notice(UiNotice::info("Device forgotten")))
    }

    // ---- the setup wizard (P06 over the P11 machine) -------------------
    //
    // Design: `docs/design/device-setup-flow.md`. Everything below runs
    // commands the REDUCER produced; nothing here decides a transition.
    // `dispatch_for` (§8) says which machinery performs each command, and
    // this is where that machinery is actually awaited.

    /// Open the wizard on a target — the two entry cards' gesture.
    async fn start_setup(&mut self, sim: bool, updates: UxUpdateSink) -> UiResult {
        let stamp = (self.local_stamp)();
        let taken = self.registered_device_names().await;
        self.setup_device = None;
        self.setup = Some(if sim {
            crate::SetupSession::simulator(
                crate::SimulatorSetupTarget::default().card_key,
                stamp,
                taken,
            )
        } else {
            crate::SetupSession::hardware(stamp, taken)
        });
        self.mark_dirty();
        updates.emit(UxUpdate::View(self.view()));
        Ok(UiNotices::new())
    }

    /// Apply one wizard gesture, then run whatever the reducer asked for.
    async fn advance_setup(
        &mut self,
        gesture: crate::SetupGesture,
        updates: UxUpdateSink,
    ) -> UiResult {
        let Some(session) = self.setup.as_mut() else {
            // A click that arrived after the card went away. Inert, like
            // every other stale gesture in this flow (§2 cross-cutting).
            return Ok(UiNotices::new());
        };
        let commands = session.handle(gesture);
        self.run_setup_commands(commands, updates).await
    }

    /// Run a command list to quiescence: each command's outcome may feed
    /// the machine another event, whose commands join the queue. The loop
    /// is flat on purpose — a flash that fails, retries, and fails again
    /// must not grow the stack.
    async fn run_setup_commands(
        &mut self,
        commands: Vec<crate::SetupCommand>,
        updates: UxUpdateSink,
    ) -> UiResult {
        let mut queue: std::collections::VecDeque<crate::SetupCommand> = commands.into();
        let mut notices = UiNotices::new();
        while let Some(command) = queue.pop_front() {
            if self.setup.is_none() {
                break;
            }
            let (outcome, event) = self.run_setup_command(&command, updates.clone()).await;
            notices.notices.extend(outcome.notices);
            self.mark_dirty();
            updates.emit(UxUpdate::View(self.view()));
            if let Some(event) = event
                && let Some(session) = self.setup.as_mut()
            {
                queue.extend(session.flow.handle(event));
            }
        }
        // A closed flow is dropped outright. DEVICE_HOME needs no cleanup
        // here: it is terminal, and `setup_view` already stopped drawing
        // it — the bound card is on the grid wearing its own body.
        if self
            .setup
            .as_ref()
            .is_some_and(|session| session.is_closed())
        {
            self.setup = None;
            self.setup_device = None;
        }
        self.mark_dirty();
        updates.emit(UxUpdate::View(self.view()));
        Ok(notices)
    }

    /// One command → the machinery `dispatch_for` names, plus the event
    /// (if any) its outcome reports back to the machine.
    async fn run_setup_command(
        &mut self,
        command: &crate::SetupCommand,
        updates: UxUpdateSink,
    ) -> (UiNotices, Option<crate::SetupEvent>) {
        use crate::{SetupCommand, SetupDispatch, SetupEvent};

        // Provisioning WRITES TO THE BOARD, so the board has to be back
        // first. The flash's own reattach normally leaves it Ready and
        // absorbed, but a board still finishing its boot (or a reattach
        // that landed a beat late) would otherwise be addressed with no
        // identity at all — which is how a typed name reached nothing and
        // the push refused "no named device is connected".
        if matches!(
            command,
            SetupCommand::WriteRegistry { .. } | SetupCommand::PushProject { .. }
        ) {
            self.settle_setup_device(&updates).await;
        }
        let context = self.setup_executor_context();
        match crate::dispatch_for(command, &context) {
            SetupDispatch::Device(DeviceOp::OpenProvider { provider_id }) => {
                // The D7 strategy and board hint live on the COMMAND (the
                // op only names the machinery); the board hint resolves to
                // its declared usb_bridge VID:PID here, where the catalog
                // is in reach.
                let (strategy, board_hint) = match command {
                    SetupCommand::RequestPort {
                        strategy,
                        board_hint,
                    } => (*strategy, board_hint.clone()),
                    _ => (crate::PortRequestStrategy::AutoAdopt, None),
                };
                let board_usb = board_hint
                    .as_deref()
                    .and_then(lpa_boards::board_by_id)
                    .and_then(|board| board.usb_bridge)
                    .map(lpa_boards::UsbBridge::vid_pid);
                self.run_setup_port_request(provider_id, strategy, board_usb, updates)
                    .await
            }
            SetupDispatch::Device(DeviceOp::ConnectEndpoint {
                provider_id,
                endpoint_id,
            }) => {
                // A grant the user picked off the in-app list (D7): no
                // chooser phase — the grant is the permission, and the
                // wizard is already narrating CONNECTING.
                self.run_setup_grant_connect(provider_id, endpoint_id, updates)
                    .await
            }
            SetupDispatch::ReadProbe => {
                // Before AND after: the connect's lines are what PROBING
                // has to show while it reads, and the escalation (a ROM
                // reset) adds its own.
                self.pump_setup_console(&updates);
                let probe = self.read_setup_probe().await;
                self.pump_setup_console(&updates);
                (UiNotices::new(), Some(SetupEvent::ProbeCompleted { probe }))
            }
            SetupDispatch::Device(op @ DeviceOp::ProvisionFirmware { .. }) => {
                self.run_setup_flash(op, updates).await
            }
            SetupDispatch::Device(op) => {
                // ReleasePort. A port that is already gone is not an
                // error: the flow asked for it to be released, and it is.
                let _ = self.execute_device_op(op, updates).await;
                self.setup_device = None;
                (UiNotices::new(), None)
            }
            SetupDispatch::Catalog(op) => match (command, self.run_catalog_op(op).await) {
                (SetupCommand::GenerateProject { .. }, Ok(outcome)) => {
                    match outcome.summary.map(|summary| summary.uid.to_string()) {
                        Some(project_uid) => (
                            UiNotices::new(),
                            Some(SetupEvent::ProjectGenerated { project_uid }),
                        ),
                        None => {
                            self.fail_setup("the generator installed no project");
                            (UiNotices::new(), None)
                        }
                    }
                }
                (SetupCommand::GenerateProject { .. }, Err(error)) => {
                    self.fail_setup(error.to_string());
                    (UiNotices::new(), None)
                }
                // The registry writes (name at provision, sighting at
                // adopt). The name is what makes the push's identity gate
                // pass, so the session's identity is re-read here.
                (_, result) => {
                    if let Err(error) = result {
                        self.fail_setup(error.to_string());
                        return (UiNotices::new(), None);
                    }
                    if let Some(device_id) = self.setup_device {
                        // Held across the refresh on purpose: the refresh
                        // CLEARS the reconcile state before it re-reads
                        // (`clear_reconcile`), and a re-read that cannot
                        // run leaves the session with no identity at all.
                        let previous = self.device_sync_for(device_id).cloned();
                        self.refresh_device_sync_for(device_id).await;
                        // The row we just wrote IS the naming truth (D34),
                        // and the push gate reads the SESSION's copy of
                        // it. The pull above re-resolves everything and
                        // normally lands the same name — but it needs the
                        // wire, and a pull that cannot run (or fails, and
                        // then records `identity: None`) would leave the
                        // board nameless one command before the push
                        // demands a name. So the write is applied to the
                        // session directly, from what we know we wrote.
                        if let SetupCommand::WriteRegistry {
                            name, hardware_uid, ..
                        } = command
                        {
                            // The SAME uid the executor addressed the row
                            // with — read off the context built BEFORE the
                            // dispatch, because a pull that failed just
                            // above has by now recorded `identity: None`
                            // and re-reading it would stamp nothing.
                            let uid = context
                                .resolved_uid
                                .clone()
                                .or_else(|| hardware_uid.clone());
                            if let Some(uid) = uid {
                                self.stamp_setup_identity(device_id, &uid, name, previous);
                            }
                        }
                    }
                    (UiNotices::new(), None)
                }
            },
            SetupDispatch::Deploy(op) => self.run_setup_push(op, updates).await,
            SetupDispatch::AttachLens => {
                let landed = self.open_setup_device_home(updates).await;
                match landed {
                    Ok(notices) => (notices, None),
                    Err(error) => {
                        self.fail_setup(error.to_string());
                        (UiNotices::new(), None)
                    }
                }
            }
            SetupDispatch::MarkIncompleteFlash => {
                self.mark_setup_flash_incomplete();
                (UiNotices::new(), None)
            }
            // Nothing to run, and a reason worth saying out loud: an
            // un-anchored board cannot be remembered, so its name lives
            // only as long as the session does.
            SetupDispatch::Skip { reason } => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!("setup: skipped {} — {reason}", command.label()),
                ));
                (UiNotices::new(), None)
            }
            // `Home` is never produced by `dispatch_for` today; a future
            // command that maps to one lands here rather than silently.
            SetupDispatch::Home(_) => (UiNotices::new(), None),
        }
    }

    /// What the executor needs that a command does not carry: the card key
    /// ops address, and the uid the bound session's identity resolves to
    /// right now (which is what a registry write is addressed with when
    /// the probe anchored none — see `SetupCommand::WriteRegistry`).
    fn setup_executor_context(&self) -> crate::SetupExecutorContext {
        let context = crate::SetupExecutorContext::new((self.now_secs)()).with_resolved_uid(
            self.setup_device.and_then(|id| {
                self.device_sync_for(id)
                    .and_then(|sync| sync.identity.as_ref())
                    .map(|identity| identity.uid.clone())
            }),
        );
        match self
            .setup
            .as_ref()
            .and_then(|session| session.card_key.clone())
        {
            Some(card_key) => context.with_card_key(card_key),
            None => context,
        }
    }

    /// Put the name the setup flow just wrote to the registry onto the
    /// bound session's cached identity, under the uid the row was written
    /// with.
    ///
    /// The registry is the naming truth (D34) and the session's copy is
    /// what every gate downstream reads — including the push's
    /// "carries a NAMED identity". Re-reading it through a pull is the
    /// thorough path and runs first; this is the part that does not need
    /// the wire, so a pull that could not run (or that failed, and
    /// recorded `identity: None` for its trouble) cannot leave a board
    /// nameless one command before the push demands a name.
    ///
    /// `previous` is the sync from BEFORE that refresh, and it is not
    /// belt-and-braces: `refresh_device_sync_for` clears the reconcile
    /// state before it re-reads, so a re-read that could not run (the
    /// board still finishing its reboot, the protocol not attached yet)
    /// leaves the session with NO identity — and the push one command
    /// later refuses a board it can see. The classification in `previous`
    /// is a moment old and nothing on the board changed in between; the
    /// alternative is discarding a device's identity because a read did
    /// not happen.
    ///
    /// Never invents a sync from nothing: a board nobody has ever read
    /// gets no content classification here.
    fn stamp_setup_identity(
        &mut self,
        device_id: crate::RuntimeId,
        uid: &str,
        name: &str,
        previous: Option<DeviceSyncState>,
    ) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let Some(session) = self.device_session_by_id(device_id) else {
            return;
        };
        let Some(mut sync) = session.device_sync().cloned().or(previous) else {
            return;
        };
        sync.identity = Some(crate::app::places::DeviceIdentity {
            uid: uid.to_string(),
            name: name.to_string(),
        });
        session.set_device_sync(Some(sync));
        self.mark_dirty();
    }

    /// Wait for the bound board to be BACK — Ready, and its copy absorbed
    /// — before provisioning writes to it or pushes.
    ///
    /// Bounded by the link layer's own readiness deadline:
    /// [`lpa_link::DeviceSession::wait_ready`] drives the readiness engine
    /// until the state leaves `Booting` or that deadline expires, and is
    /// idempotent outside `Booting` (so on the ordinary path — where the
    /// flash's own reattach already landed — this returns immediately and
    /// costs nothing). A board that never comes back is left to the
    /// push's own refusal rather than a second error voice.
    async fn settle_setup_device(&mut self, updates: &UxUpdateSink) {
        let Some(device_id) = self.setup_device else {
            return;
        };
        let state = match self.hardware_session_for(device_id) {
            Some(session) => Some(session.wait_ready().await),
            None => None,
        };
        if !matches!(state, Some(DeviceState::Ready { .. })) {
            return;
        }
        // Ready, but nothing absorbed: the post-flash pull is what turns
        // a hello into a resolved identity, and the registry write and
        // the push both address the board through it.
        if self
            .device_sync_for(device_id)
            .and_then(|sync| sync.identity.as_ref())
            .is_none()
        {
            self.refresh_device_sync_for(device_id).await;
            self.mark_dirty();
            updates.emit(UxUpdate::View(self.view()));
        }
    }

    /// `RequestPort`: the D7 grant ladder, then (at its bottom) the
    /// browser's own chooser, then a connect. The port grant is what gives
    /// the flow a session — and therefore a card key.
    ///
    /// The two phases are awaited SEPARATELY so the wizard can narrate
    /// them apart (bench, 2026-08-08): the chooser resolving — or a grant
    /// being adopted — is reported as `PortChosen` the moment it happens,
    /// and the several seconds of open/reset/boot/hello that follow are
    /// PORT_PICKING's copy no longer — they are CONNECTING's.
    async fn run_setup_port_request(
        &mut self,
        provider_id: LinkProviderKind,
        strategy: crate::PortRequestStrategy,
        board_usb: Option<(u16, u16)>,
        updates: UxUpdateSink,
    ) -> (UiNotices, Option<crate::SetupEvent>) {
        // The grant sweep runs first and prompts for nothing. Several
        // usable grants end the command here: the wizard renders the
        // in-app list, and the user's row click is a fresh command.
        let plan = if provider_id == LinkProviderKind::BrowserSerialEsp32 {
            self.device.plan_granted_ports(strategy, board_usb).await
        } else {
            crate::GrantPortPlan::Chooser
        };
        if let crate::GrantPortPlan::Offer { ports } = plan {
            let ports = ports
                .into_iter()
                .map(|port| crate::SetupGrantedPort {
                    endpoint_id: port.endpoint_id.as_str().to_string(),
                    label: port.label,
                })
                .collect();
            return (
                UiNotices::new(),
                Some(crate::SetupEvent::GrantedPortsListed { ports }),
            );
        }
        let before: Vec<crate::RuntimeId> = self
            .pool
            .device_sessions()
            .map(crate::RuntimeSession::id)
            .collect();
        // Claim any session born during this request before it can render:
        // the await below emits views while the session installs, and the
        // bind only lands after it returns.
        self.setup_port_snapshot = Some(before.clone());
        let outcome = match plan {
            // A single unambiguous grant: adopt it, saying WHICH port —
            // the label rides `PortChosen` so CONNECTING can name it and
            // offer the way back (D7: visible and reversible, never
            // silent).
            crate::GrantPortPlan::Adopt { endpoint_id, label } => {
                self.note_setup_progress(
                    crate::SetupEvent::PortChosen {
                        via_grant: Some(label),
                    },
                    &updates,
                );
                self.device.connect_endpoint(provider_id, endpoint_id).await
            }
            crate::GrantPortPlan::Chooser => {
                match self.device.choose_provider_endpoint(provider_id).await {
                    // A port is in hand: the flow leaves PORT_PICKING
                    // here, not after the connect below.
                    Ok(crate::PortChoice::Endpoint(endpoint_id)) => {
                        self.note_setup_progress(
                            crate::SetupEvent::PortChosen { via_grant: None },
                            &updates,
                        );
                        self.device.connect_endpoint(provider_id, endpoint_id).await
                    }
                    Ok(crate::PortChoice::Ended(outcome)) => Ok(outcome),
                    Err(error) => Err(error),
                }
            }
            crate::GrantPortPlan::Offer { .. } => {
                unreachable!("offers returned before the snapshot claim")
            }
        };
        self.finish_setup_port_request(provider_id, outcome, before, updates)
            .await
    }

    /// A grant picked off the in-app list (D7): connect that endpoint —
    /// no chooser phase — and land the outcome exactly the way a chooser
    /// grant lands.
    async fn run_setup_grant_connect(
        &mut self,
        provider_id: LinkProviderKind,
        endpoint_id: lpa_link::LinkEndpointId,
        updates: UxUpdateSink,
    ) -> (UiNotices, Option<crate::SetupEvent>) {
        let before: Vec<crate::RuntimeId> = self
            .pool
            .device_sessions()
            .map(crate::RuntimeSession::id)
            .collect();
        self.setup_port_snapshot = Some(before.clone());
        let outcome = self.device.connect_endpoint(provider_id, endpoint_id).await;
        self.finish_setup_port_request(provider_id, outcome, before, updates)
            .await
    }

    /// The shared tail of every setup port request: install the outcome,
    /// bind the born session to the flow, and report `PortGranted` or
    /// `PortPickerCancelled` back to the machine.
    async fn finish_setup_port_request(
        &mut self,
        provider_id: LinkProviderKind,
        outcome: Result<DeviceOpenOutcome, UiError>,
        before: Vec<crate::RuntimeId>,
        updates: UxUpdateSink,
    ) -> (UiNotices, Option<crate::SetupEvent>) {
        // The chooser's own vocabulary: a grant that produced no session
        // is a cancel. Web Serial cannot tell "cancelled" from "the list
        // was empty" — both are one `NotFoundError` — so the flow only
        // ever hears `PortPickerCancelled` here, and the escalation to
        // board-first rides the intro's secondary CTA instead.
        let cancelled = matches!(
            outcome,
            Ok(DeviceOpenOutcome::Cancelled { .. } | DeviceOpenOutcome::Opened)
        );
        let failed = outcome.is_err();
        let settled = self
            .settle_connect_outcome(runtime_kind_for(provider_id), outcome, updates.clone())
            .await;
        let notices = match settled {
            Ok(notices) => notices,
            Err(error) => {
                self.push_log(UiLogDraft::from_notice(UiNotice::warning(
                    error.to_string(),
                )));
                UiNotices::new()
            }
        };
        let granted = self
            .pool
            .device_sessions()
            .map(crate::RuntimeSession::id)
            .find(|id| !before.contains(id))
            // A re-picked port reconnects the SAME endpoint rather than
            // adding a session; the flow still holds a port then.
            .or_else(|| {
                if cancelled || failed {
                    None
                } else {
                    self.pool
                        .oldest_device_session()
                        .map(crate::RuntimeSession::id)
                }
            });
        // The bind (or the cancel) supersedes the snapshot claim.
        self.setup_port_snapshot = None;
        match granted {
            Some(device_id) => {
                self.bind_setup_device(device_id);
                // Everything the connect narrated belongs on the wizard's
                // terminal now that there is a session to hang it on.
                self.pump_setup_console(&updates);
                (notices, Some(crate::SetupEvent::PortGranted))
            }
            None => (notices, Some(crate::SetupEvent::PortPickerCancelled)),
        }
    }

    /// Report an interim outcome to the machine WHILE the command that
    /// produced it is still running.
    ///
    /// [`Self::run_setup_command`] reports one event, at the end. An op
    /// with an honest mid-point — the chooser resolving, seconds before
    /// the connect it starts finishes — has to say so as it happens, or
    /// the wizard narrates the step the user already finished.
    ///
    /// Interim events are STATE-ONLY by construction: there is no queue to
    /// run commands on from inside a command, so a transition that asks
    /// for work does not belong here. The reducer's transition table
    /// asserts exactly that for every interim event it defines.
    fn note_setup_progress(&mut self, event: crate::SetupEvent, updates: &UxUpdateSink) {
        let Some(session) = self.setup.as_mut() else {
            return;
        };
        let commands = session.flow.handle(event);
        debug_assert!(
            commands.is_empty(),
            "an interim setup event cannot ask for work: {commands:?}"
        );
        self.mark_dirty();
        updates.emit(UxUpdate::View(self.view()));
    }

    /// Drain what the link has said into the bound session's console tail,
    /// so the wizard's terminal shows the connect's own lines instead of
    /// only the browser console.
    ///
    /// The heartbeat does this on a tick; a setup op holds the controller
    /// across its awaits, so no tick can interleave and the tail only
    /// advances where the flow asks. Called at the boundaries the flow can
    /// actually observe — after the connect, and around the probe.
    fn pump_setup_console(&mut self, updates: &UxUpdateSink) {
        let Some(device_id) = self.setup_device else {
            return;
        };
        let mut drafts = self.device.take_pending_device_logs();
        if let Some(session) = self.pool.session_mut(device_id) {
            drafts.extend(session.take_pending_logs());
            drafts.extend(session.take_device_console_logs());
        }
        if drafts.is_empty() {
            return;
        }
        self.record_session_logs(device_id, drafts);
        self.mark_dirty();
        updates.emit(UxUpdate::View(self.view()));
    }

    /// Remember which session the flow drives, and give the executor the
    /// card key its ops address (`DeviceTarget::card`).
    fn bind_setup_device(&mut self, device_id: crate::RuntimeId) {
        self.setup_device = Some(device_id);
        if let Some(session) = self.setup.as_mut() {
            session.card_key = Some(device_id.to_string());
        }
    }

    /// `ProbeBoard`: one passive read of what the session already knows,
    /// classified by [`classify_board`](crate::classify_board) against the
    /// registry. Nothing is re-derived here — the evidence is exactly what
    /// the link layer observed.
    async fn read_setup_probe(&mut self) -> crate::BoardProbe {
        let registry = self.registered_devices().await;
        let Some(device_id) = self.setup_device else {
            return crate::classify_board(&crate::ProbeEvidence::default(), &registry);
        };
        let evidence = self.setup_probe_evidence(device_id);
        let probe = crate::classify_board(&evidence, &registry);
        if !matches!(probe.verdict, crate::BoardVerdict::Unresponsive { .. }) {
            return probe;
        }
        // Design §8: escalate ONLY on `Unresponsive`. The sync probe
        // resets the board to talk to its ROM loader, which is why it is
        // never automatic — but a board that said nothing intelligible has
        // nothing to lose, and this is what turns "it's dead" into "it's
        // blank, here are your boards".
        // The probe narrates itself (esptool's own terminal), and the
        // wizard's PROBING terminal is where those seconds belong. The
        // session borrow ends with the block so the lines can be recorded.
        let probe_logs = Rc::new(RefCell::new(Vec::new()));
        let probed = {
            let Some(session) = self.hardware_session_for(device_id) else {
                return probe;
            };
            session
                .probe_link_mode(probe_event_sink(Rc::clone(&probe_logs)))
                .await
        };
        let drafts = core::mem::take(&mut *probe_logs.borrow_mut());
        self.record_session_logs(device_id, drafts);
        let mut escalated = self.setup_probe_evidence(device_id);
        // The escalation's answer lives ONLY in this return value. The
        // probe ends by rebuilding the link, and `DeviceSnapshot::link_mode`
        // is recomputed passively from a boot-line classifier that the
        // rebuild clears — so a re-read alone lands back on `Unresponsive`
        // however well the conversation went (bench, 2026-08-08: a board
        // parked in the esptool stub was told to hold BOOT).
        if let Ok(lpa_link::DeviceLinkMode::Bootloader {
            chip_name,
            evidence: lpa_link::BootloaderEvidence::SyncHandshake,
        }) = &probed
        {
            escalated.bootloader_conversation = true;
            // The probe's chip identity is authoritative and it is what
            // filters the board pick; the passive read has none once the
            // rebuild wiped the classifier.
            if escalated.detected_chip.is_none() {
                escalated.detected_chip.clone_from(chip_name);
            }
        }
        crate::classify_board(&escalated, &registry)
    }

    /// One probe pass's evidence, in the link layer's own vocabulary.
    fn setup_probe_evidence(&self, device_id: crate::RuntimeId) -> crate::ProbeEvidence {
        let session = self.pool.device_session(device_id);
        let device_state = session.and_then(crate::RuntimeSession::device_state);
        let hello = match &device_state {
            Some(DeviceState::Ready { hello }) => Some(hello.clone()),
            _ => None,
        };
        let snapshot = session
            .and_then(crate::RuntimeSession::hardware_session)
            .map(lpa_link::DeviceSession::snapshot);
        // The no-firmware signature has two sources: the session sitting
        // in ROM download mode, and the link's OWN boot-line classifier
        // (`invalid header: 0xffffffff` after a clean ROM banner →
        // `DeviceState::BlankFlash`). The G2 walk's blank C6 was hard-reset
        // OUT of the bootloader before the wizard's read, so only the
        // second source knew — ignoring it turned "blank, pick a board"
        // into "nothing intelligible answered".
        let state_says_blank = matches!(
            device_state,
            Some(DeviceState::BlankFlash | DeviceState::Bootloader)
        );
        crate::ProbeEvidence {
            hello_seen: hello.is_some(),
            // `Incompatible` is the link's own diagnosis of LightPlayer
            // framing with no proto-matching hello — firmware too old for
            // this Studio. Its one affordance is a reflash, which is what
            // the verdict routes to; without this the flow read the board
            // as unresponsive and offered BOOT-hold advice instead.
            stale_lightplayer: matches!(device_state, Some(DeviceState::Incompatible { .. })),
            no_firmware_signature: state_says_blank
                || snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.link_mode.is_bootloader()),
            // This is the PASSIVE read, and no passive evidence can carry a
            // conversation: `snapshot.link_mode` is derived from boot lines
            // alone. Only `read_setup_probe`, holding the escalation's
            // return value, can set this.
            bootloader_conversation: false,
            lines: snapshot
                .as_ref()
                .map(|snapshot| snapshot.recent_lines.clone())
                .unwrap_or_default(),
            detected_chip: snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.detected_chip.clone()),
            base_mac: snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.probed_mac.clone())
                .or_else(|| hello.and_then(|hello| hello.hardware.base_mac)),
        }
    }

    /// `Flash`: the EXISTING provisioning op, unchanged. Its verdict is
    /// read off the card-owned op flow rather than the notices — that flow
    /// is what the refusal path (no image for this chip) writes to, and it
    /// is the same surface the abandon guard marks.
    async fn run_setup_flash(
        &mut self,
        op: DeviceOp,
        updates: UxUpdateSink,
    ) -> (UiNotices, Option<crate::SetupEvent>) {
        let result = self.execute_device_op(op, updates).await;
        let failure = self.setup_flash_failure();
        let notices = match result {
            Ok(notices) => notices,
            Err(error) => {
                return (
                    UiNotices::new(),
                    Some(crate::SetupEvent::FlashFailed {
                        detail: error.to_string(),
                    }),
                );
            }
        };
        match failure {
            Some(detail) => (notices, Some(crate::SetupEvent::FlashFailed { detail })),
            None => (notices, Some(crate::SetupEvent::FlashSucceeded)),
        }
    }

    /// The failure text the card-owned op flow is carrying, if it failed.
    fn setup_flash_failure(&self) -> Option<String> {
        let device_id = self.setup_device?;
        let op = self.device_card_ops.get(&device_id)?.borrow();
        match &op.phase {
            crate::CardOpPhase::Failed { error, .. } => Some(error.clone()),
            _ => None,
        }
    }

    /// `PushProject`. On hardware that is the deploy lane, verbatim. On a
    /// target with no device session — THE simulator — it is the sim's own
    /// load path, reached explicitly here rather than by the open-anything
    /// hardwiring the device-first-creation ADR retired.
    async fn run_setup_push(
        &mut self,
        op: DeployOp,
        updates: UxUpdateSink,
    ) -> (UiNotices, Option<crate::SetupEvent>) {
        let sim = self.setup.as_ref().is_some_and(|session| session.sim);
        let result = if sim {
            let DeployOp::PushProject { key, .. } = &op else {
                return (UiNotices::new(), None);
            };
            self.open_on_simulator(PendingOpen::Package(key.clone()), updates)
                .await
        } else {
            self.execute_deploy_op(op, updates).await
        };
        match result {
            Ok(notices) => (notices, Some(crate::SetupEvent::PushCompleted)),
            Err(error) => {
                self.fail_setup(error.to_string());
                (UiNotices::new(), None)
            }
        }
    }

    /// `OpenDeviceHome`: the editor lensed to this target (vision D17).
    /// The sim path is already lensed by its own load, so this is the
    /// hardware landing.
    async fn open_setup_device_home(&mut self, updates: UxUpdateSink) -> UiResult {
        if self.setup.as_ref().is_some_and(|session| session.sim) {
            return Ok(UiNotices::new());
        }
        let Some(device_id) = self.setup_device else {
            return Ok(UiNotices::new());
        };
        self.attach_lens(device_id, updates).await
    }

    /// `MarkIncompleteFlash`: leave the board's card saying so. An
    /// interrupted image is never trusted, and the card is where the user
    /// will look. The board stays REMEMBERED — identity is anchored in
    /// silicon, so an incomplete flash costs a re-flash, not a name.
    fn mark_setup_flash_incomplete(&mut self) {
        let Some(device_id) = self.setup_device else {
            return;
        };
        self.device_card_ops.insert(
            device_id,
            Rc::new(RefCell::new(crate::CardOp::failed(
                "Flashing firmware",
                "Incomplete flash — the board needs re-flashing before it can run. \
                 Nothing was bricked: its bootloader is untouched.",
                "Back to set up",
            ))),
        );
        self.mark_dirty();
    }

    /// Record a failure the MACHINE has no edge for — a generate or a push
    /// that did not land (design §7.10). The wizard shows it and offers
    /// its ✕; inventing a transition here would be inventing the answer to
    /// "what does a flashed, registered board with no project mean".
    fn fail_setup(&mut self, error: impl Into<String>) {
        let error = error.into();
        self.push_log(UiLogDraft::new(
            UiLogLevel::Error,
            UiLogOrigin::Studio,
            error.clone(),
        ));
        if let Some(session) = self.setup.as_mut() {
            session.error = Some(error);
        }
        self.mark_dirty();
    }

    /// Every remembered device row (the recognition lookup's corpus).
    async fn registered_devices(&mut self) -> Vec<crate::app::places::RegisteredDevice> {
        let Ok(host) = self.library_host() else {
            return Vec::new();
        };
        let Ok(fs) = host.catalog_snapshot().await else {
            return Vec::new();
        };
        crate::app::places::DeviceRegistry::new(fs)
            .list()
            .unwrap_or_default()
    }

    /// Every remembered device's name, for the provision step's collision
    /// suffix (design §3).
    async fn registered_device_names(&mut self) -> Vec<String> {
        self.registered_devices()
            .await
            .into_iter()
            .map(|row| row.name)
            .filter(|name| !name.is_empty())
            .collect()
    }

    /// The wizard, if one is open and still has something to draw: the
    /// machine's state, the card it rides (`takeover_card` — `None` renders
    /// it standalone in the entry-cards slot), and the two things only the
    /// controller can see — the live flash op and the session's console
    /// tail.
    ///
    /// A flow at DEVICE_HOME/CLOSED draws NOTHING: the handoff is a body
    /// swap, so the bound card is already on the grid wearing its own body
    /// (G2 ruling), and a standalone frame here would be the card
    /// appearing that the ruling forbids.
    fn setup_view(&self, takeover_card: Option<String>) -> Option<crate::UiSetupWizard> {
        if !self.setup_flow_running() {
            return None;
        }
        let session = self.setup.as_ref()?;
        let flash = self
            .setup_device
            .and_then(|id| self.device_card_ops.get(&id))
            .map(|slot| slot.borrow().clone());
        let console_tail = self
            .setup_device
            .and_then(|id| self.pool.device_session(id))
            .map(|session| session.console_tail().iter().cloned().collect())
            .unwrap_or_default();
        Some(session.view(takeover_card, flash, console_tail))
    }

    /// Apply a card UI view-state mutation (2026-07-25 re-home): flip the
    /// entry keyed by the card's identity and mark the view dirty. Pure
    /// and synchronous — no wire, no library.
    fn apply_card_ui_op(&mut self, op: crate::CardUiOp) {
        use crate::CardUiOp;
        match op {
            CardUiOp::SelectTab { card, tab } => {
                self.card_ui.entry(card).or_default().tab = tab;
            }
            CardUiOp::OpenSheet { card, sheet } => {
                self.card_ui.entry(card).or_default().sheet = Some(sheet);
            }
            CardUiOp::CloseSheet { card } => {
                self.card_ui.entry(card).or_default().sheet = None;
            }
            // The failed op's ONE exit (model §2 I4): drop THAT card's
            // flow; the card re-derives its honest state on the next view
            // build. Another board's op in flight is untouched.
            CardUiOp::ClearOp { card } => {
                if let Some(id) = self.device_id_for_card_key(&card) {
                    self.device_card_ops.remove(&id);
                }
            }
            CardUiOp::SelectSetupBoard { card, board_id } => {
                self.card_ui.entry(card).or_default().setup_board = board_id;
            }
        }
        self.mark_dirty();
    }

    /// Overlay each card's persisted UI view-state (tab + sheet) and its
    /// live in-place op onto a freshly-built roster card. The builder
    /// leaves `ui` default; identity keys the lookup. The `op` derives
    /// from the attached session's `operation_label` (the same flag the
    /// Operation-in-flight card state reads) so the in-place progress and
    /// the edge treatment never disagree.
    fn overlay_card_ui(&self, mut card: crate::UiDeviceCard) -> crate::UiDeviceCard {
        // The tab the card comes up on is the ONE answer
        // (`effective_card_tab`): the saved choice, else the default rule.
        // Reading it here rather than leaning on `CardUiState::default()`
        // is what makes a fresh connected card open on ▶ — and what keeps
        // the rendered tab and the frame feed's gate the same fact.
        let key = card.identity_key().to_string();
        if let Some(saved) = self.card_ui.get(&key) {
            card.ui = saved.clone();
        } else {
            card.ui.tab = self.default_card_tab(&key);
        }
        // The CARD-OWNED op flow first (model §2, I1): it survives the
        // session the op severed, so it outranks session-derived
        // narration. Targeting is the managed SESSION (M4) — matched
        // through the one shared rule, `takes_card_op`.
        if let Some(slot) = self
            .device_card_ops
            .iter()
            .find(|(id, _)| card.takes_card_op(&id.to_string()))
            .map(|(_, slot)| slot)
        {
            card.ui.op = Some(slot.borrow().clone());
            return card;
        }
        // the live op: the session whose card this is, mid-operation
        let op_label = if card.sim {
            self.pool
                .sim_session()
                .and_then(|session| session.operation_label().map(str::to_string))
        } else {
            // Only the session's OWN card narrates its op — now by
            // simple lookup (M4). What this replaces was an inference:
            // with one session, "is this card the live one?" had to be
            // guessed from the stamped identity, and a bare is_some()
            // check once smeared one device's push across every device
            // card. A card names its session; nothing needs guessing.
            card.session_key
                .as_deref()
                .and_then(|key| self.device_id_for_card_key(key))
                .and_then(|id| self.pool.device_session(id))
                .and_then(|session| session.operation_label().map(str::to_string))
        };
        if let Some(label) = op_label {
            let percent = match &card.state {
                crate::RosterCardState::OperationInFlight { percent, .. } => *percent,
                _ => None,
            };
            card.ui.op = Some(crate::CardOp::new(label, percent));
        }
        card
    }

    /// The live half of a device rename (D34): when the renamed device is
    /// the attached one, update the cached sync state so every surface
    /// shows the new name immediately — and, for a board whose identity
    /// still lives in its filesystem, write `/.lp/device.json` back over
    /// the wire. Offline devices skip both halves; the write-back happens
    /// on the next connect (`resolve_session_identity`).
    ///
    /// The wire half is `Minted`-only (design §5), the same rule the
    /// connect path applies: an ESP-class board's name lives in the
    /// registry alone, so renaming it never touches its filesystem.
    async fn write_back_live_identity_name(
        &mut self,
        uid: &str,
        name: &str,
    ) -> Result<(), UiError> {
        use lpc_model::AsLpPath;
        // The renamed device names itself: `uid` IS the target (M4). It
        // used to resolve "the" device and then check the uid matched —
        // which wrote the new name to the wrong board's
        // `/.lp/device.json` whenever the renamed one was not the oldest
        // attached.
        let Some(device_id) = self.device_id_for_card_key(uid) else {
            return Ok(());
        };
        let identity = crate::app::places::DeviceIdentity {
            uid: uid.to_string(),
            name: name.to_string(),
        };
        let file_is_the_store = matches!(
            self.pool
                .device_session(device_id)
                .and_then(crate::RuntimeSession::hardware_id),
            Some(crate::app::places::HardwareId::Minted { .. })
        );
        if file_is_the_store {
            let logs = self
                .pool
                .device_session_mut(device_id)?
                .client_mut()?
                .fs_write(
                    crate::app::places::DEVICE_IDENTITY_PATH.as_path(),
                    &identity.to_json_bytes(),
                )
                .await?;
            self.record_logs(logs);
        }
        if let Some(sync) = self
            .pool
            .device_session_mut(device_id)
            .ok()
            .and_then(crate::RuntimeSession::device_sync_mut)
        {
            sync.identity = Some(identity);
        }
        Ok(())
    }

    /// Run one catalog transaction through the host and schedule a gallery
    /// re-hydration (the dispatch wrapper drains it).
    async fn run_catalog_op(
        &mut self,
        op: CatalogOp,
    ) -> Result<crate::app::library::CatalogOutcome, UiError> {
        let host = self.library_host()?;
        let result = host
            .catalog(op)
            .await
            .map_err(|error| self.library_error_with_name(error));
        self.request_library_refresh();
        result
    }

    /// The friendly error copy, upgraded with the project's slug when the
    /// cached gallery inputs know it ("2026-07-02-0930-porch-sign is open
    /// in another tab" beats "This project…"). Falls back to the generic
    /// `From` wording otherwise.
    fn library_error_with_name(&self, error: crate::app::library::LibraryHostError) -> UiError {
        if let crate::app::library::LibraryHostError::OpenElsewhere { key } = &error {
            let slug = self.home_inputs.as_ref().and_then(|inputs| {
                inputs
                    .projects
                    .iter()
                    .find(|card| card.uid == *key || card.slug == *key)
                    .map(|card| card.slug.clone())
            });
            if let Some(slug) = slug {
                return UiError::UnsupportedAction(format!(
                    "{slug} is open in another tab — close it there first"
                ));
            }
        }
        UiError::from(error)
    }

    /// The attached library host for home ops, or the error the gallery
    /// surfaces when the local store never mounted.
    fn library_host(&self) -> Result<Rc<dyn LibraryHost>, UiError> {
        self.library_host.clone().ok_or_else(|| {
            UiError::MissingSession("the local project library is unavailable".to_string())
        })
    }

    /// Open a home card: push the package's head to the simulator,
    /// creating or reusing THE sim session (D13: a library card opens in
    /// the sim; the sim is invisible infrastructure). A connected hardware
    /// device simply stays attached and reconciled while the project opens
    /// (P2 coexistence — the old "disconnect the device to open this
    /// project" refusal is gone).
    async fn open_from_home(&mut self, pending: PendingOpen, updates: UxUpdateSink) -> UiResult {
        // Opening a LIBRARY CARD still means the simulator, and now says
        // so: the destination is named at the call site rather than being
        // the one thing `open_from_home_inner` could do (device-first
        // creation ADR — the setup wizard reaches the sim through
        // `open_on_simulator` too, explicitly, and never through here).
        self.open_on_simulator(pending, updates).await
    }

    /// Open a package or example ON THE SIMULATOR: start or reuse THE sim
    /// session, put the lens on it, and load. The explicit sim path.
    ///
    /// This is also where the open's public narration begins and ends
    /// ([`crate::app::open_progress`]): the stage the opening frame reads,
    /// the supersede generation the parked flow checks, and the terminal
    /// failure that replaces the eternal skeleton with an error and a
    /// Retry.
    async fn open_on_simulator(&mut self, pending: PendingOpen, updates: UxUpdateSink) -> UiResult {
        // The click owns the engine while it runs: background preview work
        // (pool boots, new lease deploys, hover-to-play) stops STARTING
        // until this guard drops, on every exit path — success, failure, or
        // the whole future being dropped. See `app::open_priority`.
        let _open_priority = crate::app::open_priority::begin_user_open();
        crate::app::open_progress::note_open_started();
        let retry = pending.retry_action();
        // The missing-library refusal rides the SAME reporting as every
        // other failure below (it used to `?` straight out, which left the
        // opening frame narrating an open that had already given up).
        let result = match self.library_host() {
            Ok(_) => {
                self.pending_open = Some(pending);
                // The card's whole "opening" treatment — the dim, the busy
                // cursor, and the pipeline line that narrates the engine
                // download — rides `home.opening`, which reaches the DOM
                // only inside a published VIEW. The actor is about to park
                // inside this open until it settles, and the dispatch
                // wrapper's snapshots bracket the action (before has no
                // pending open yet; after, it is already over): without
                // this emit, a slow open runs to completion behind a
                // gallery that never acknowledged the click at all (the
                // G1 Q1 residual).
                updates.emit(UxUpdate::View(self.view()));
                let result = self.open_from_home_inner(updates.clone()).await;
                self.pending_open = None;
                result
            }
            Err(error) => Err(error),
        };
        // A superseded open has no verdict to report: the user did not
        // fail at anything, they clicked somewhere else. Its error (if the
        // unwind produced one) is swallowed, and the stage is left to the
        // open that replaced it — which has already started narrating.
        if crate::app::open_progress::open_superseded() {
            return Ok(UiNotices::new());
        }
        match result {
            Ok(notices) => {
                crate::app::open_progress::note_open_settled();
                Ok(notices)
            }
            Err(error) => {
                crate::app::open_progress::note_open_failed(error.message(), retry);
                Err(error)
            }
        }
    }

    async fn open_from_home_inner(&mut self, updates: UxUpdateSink) -> UiResult {
        // A lens on the DEVICE session (the D29 editor) detaches first —
        // quiesce, then open on the sim (P3). The device session stays.
        if self
            .pool
            .lens_session()
            .is_some_and(|session| !session.is_sim())
        {
            self.quiesce_lens();
        }
        // A crashed sim (SimCrashed: its worker's wasm instance is
        // poisoned) cannot be reconnected — the worker never answers
        // again. Tear it down here so the open falls through to a fresh
        // install below; this is also the MANUAL restart path when the
        // auto-reboot flap guard left the session Failed.
        if self.pool.sim_session().is_some_and(|sim| {
            matches!(
                sim.server_state(),
                ServerState::Failed {
                    kind: ServerFailureKind::SimCrashed,
                    ..
                }
            )
        }) {
            let sim_id = self.pool.sim_session().map(crate::RuntimeSession::id);
            if let Some(sim_id) = sim_id {
                self.teardown_crashed_sim(sim_id).await;
                // Free the dead session's project tab lock now — the settle
                // points run after this open, which needs the lock itself.
                self.project.release_closed_library_projects().await;
            }
        }
        // Boundary 1 of 3 (post-teardown): the first real await this open
        // can be outlived at. Nothing project-specific has happened yet —
        // no lock, no worker, no deploy — so a stale open just leaves, and
        // the teardown it did was work the newer open wanted anyway.
        if crate::app::open_progress::open_superseded() {
            return Ok(UiNotices::new());
        }
        // Single-session policy (module doc): a reuse is an open like any
        // other, so a board attached to this tab goes first. The install
        // funnel's gate never runs on this path — the sim is already in
        // the pool — which would have left the reuse branch as the one
        // door the policy did not close.
        if let Some(sim_id) = self.pool.sim_session().map(crate::RuntimeSession::id) {
            self.enforce_single_session(Some(sim_id)).await?;
        }
        // The open targets THE sim session: reuse it when it exists — the
        // lens moves onto it (the editor mirror opens on the sim) — and
        // replace-and-load directly when its server protocol is live, or
        // reconnect its server first when not. A fresh install claims the
        // lens by the pool's lens-less rule (the quiesce above cleared it).
        if let Some(sim) = self.pool.sim_session() {
            let sim_id = sim.id();
            let server_live = matches!(sim.server_state(), ServerState::Connected { .. });
            // D37/M5 (`/p/<slug>-<uid>` — and the project-card click that
            // now rides it): when the sim ALREADY runs the requested project,
            // re-attach the lens instead of pushing the head again — the
            // running session with its server-side overlay IS the document
            // (SDI); a fresh push would discard applied-but-unsaved edits.
            // A different (or no) loaded project keeps the D19 head push.
            let pending_key = match &self.pending_open {
                Some(PendingOpen::Package(key)) => Some(key.as_str()),
                _ => None,
            };
            let already_running = pending_key.is_some_and(|key| {
                sim.sim_loaded_project()
                    .is_some_and(|project| project.uid == key || project.name == key)
            });
            if already_running && server_live {
                let notices = self.attach_lens(sim_id, updates.clone()).await?;
                // A re-attach IS a landed open-in-sim: the user clicked
                // "Open in sim" and the sim now runs that project, board
                // and all. It just landed EARLIER, so nothing loads here
                // and `note_sim_loaded_project` never runs — which left
                // the picker up on exactly this route (G1b ruling 6).
                self.stand_down_sim_setup(updates).await?;
                return Ok(notices);
            }
            self.pool.set_lens(sim_id);
            if server_live {
                return self.open_pending_package(updates).await;
            }
            return self.connect_server_from_link(sim_id, updates).await;
        }
        // No sim yet: start the simulator runtime. A device session stays
        // attached throughout — only the SIM slot is touched on failure.
        let outcome = self
            .device
            .open_provider(LinkProviderKind::BrowserWorker)
            .await;
        match outcome {
            Ok(DeviceOpenOutcome::Connected { payload, logs }) => {
                self.record_logs(logs);
                let id = self.install_session(payload).await?;
                self.attach_runtime(id, updates).await
            }
            Ok(DeviceOpenOutcome::SoftFailed) => {
                // unreachable in practice: the ladder is hardware-only
                self.pool.remove_kind(crate::RuntimeKind::Sim);
                Err(UiError::MissingSession(
                    "the simulator did not connect".to_string(),
                ))
            }
            Ok(DeviceOpenOutcome::Opened) => {
                self.pool.remove_kind(crate::RuntimeKind::Sim);
                Err(UiError::MissingSession(
                    "the simulator opened without connecting".to_string(),
                ))
            }
            Ok(DeviceOpenOutcome::Cancelled { message }) => {
                self.pool.remove_kind(crate::RuntimeKind::Sim);
                Ok(UiNotices::new().with_notice(UiNotice::info(message)))
            }
            Err(error) => {
                self.pool.remove_kind(crate::RuntimeKind::Sim);
                Err(error)
            }
        }
    }

    /// Push the pending package to the connected runtime and load it.
    async fn open_pending_package(&mut self, updates: UxUpdateSink) -> UiResult {
        // Boundary 2 of 3 (post-boot): the engine is up and attached, and
        // that is the expensive part — so the superseded open leaves the
        // SIM SESSION STANDING and only skips its own deploy. The engine
        // binary is identical for every open and projects deploy into a
        // booted worker later (boot protocol), so the click that replaced
        // this one walks straight into `open_pending_package` on the same
        // worker. Terminating it here would make the newest click the
        // slowest one.
        if crate::app::open_progress::open_superseded() {
            return Ok(UiNotices::new());
        }
        let pending = self
            .pending_open
            .clone()
            .ok_or_else(|| UiError::MissingSession("no pending package to open".to_string()))?;
        crate::app::open_progress::note_preparing_project();
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Opening project",
            "Opening",
            "Pushing the project to the simulator",
        );
        let result = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            match &pending {
                PendingOpen::Package(key) => self.project.open_library_package(server, key).await,
                // Examples open as TRANSIENT view sessions (vision D2):
                // no seed, no library entry, no persisted uid. The
                // explicit-save gesture is what installs a copy.
                PendingOpen::Example(id) => self.project.open_example_transient(server, id).await,
                // A View-access share link, likewise (P5): the fetched
                // bytes run the cloud document's own uid; save forks.
                PendingOpen::SharedTransient {
                    uid,
                    name,
                    package_files,
                    history_files,
                } => match uid.parse() {
                    Ok(uid) => {
                        self.project
                            .open_shared_transient(server, uid, name, package_files, history_files)
                            .await
                    }
                    Err(e) => Err(UiError::MissingSession(format!(
                        "shared open: invalid uid {uid:?}: {e}"
                    ))),
                },
            }
        };
        match result {
            Ok(logs) => {
                self.record_logs(logs);
                self.note_sim_loaded_project();
                self.stand_down_sim_setup(updates.clone()).await?;
                // The open path's own notices come FIRST — a format upgrade
                // is the thing the user most needs to read, and it must not
                // be a console-only line (P3).
                let mut notices = UiNotices::new();
                for notice in self.project.take_open_notices() {
                    notices = notices.with_notice(notice);
                }
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(notices.with_notice(project_sync_notice(
                    sync.synced,
                    "Project opened",
                    "Project opened; project sync needs attention",
                )))
            }
            Err(error) => {
                // `from_error`, not a bare Error draft: a superseded open
                // unwinds through here as `Cancelled`, and the user clicking
                // somewhere else is information, not a failure (Q5/P7).
                self.push_log(UiLogDraft::from_error(error.clone()));
                self.project.fail(error.to_string());
                Err(error)
            }
        }
    }

    async fn execute_project_op(&mut self, op: ProjectOp, updates: UxUpdateSink) -> UiResult {
        match op {
            ProjectOp::ConnectRunningProject => self.connect_running_project(updates).await,
            ProjectOp::ConnectLoadedProject { handle_id } => {
                self.connect_loaded_project(handle_id, updates).await
            }
            ProjectOp::LoadDemoProject => self.load_demo_project(updates).await,
            ProjectOp::OpenDocsExample { example_id } => {
                self.open_docs_example(&example_id, updates).await
            }
            ProjectOp::RefreshProject => self.refresh_project(updates).await,
            ProjectOp::ReloadActiveProject => self.reload_active_project(updates).await,
            ProjectOp::DisconnectProject => self.disconnect_project().await,
            ProjectOp::DetachLens => self.detach_lens(),
            ProjectOp::OpenDeviceProject { uid } => self.open_device_project(uid, updates).await,
            ProjectOp::OpenSimProject => {
                let id = self
                    .pool
                    .sim_session()
                    .map(crate::RuntimeSession::id)
                    .ok_or_else(|| {
                        UiError::MissingSession("the simulator is not running".to_string())
                    })?;
                self.attach_lens(id, updates).await
            }
            ProjectOp::SaveOverlay => {
                let run = {
                    let server = self.pool.lens_session_mut()?.client_mut()?;
                    self.project.save_overlay(server).await
                };
                // A save can be the FORK of a transient session (D7), and a
                // shared-view fork changes the active project's identity —
                // re-note so the lens (and therefore the URL) follows the
                // fork. Idempotent for every ordinary save.
                self.note_sim_loaded_project();
                self.record_project_edit_run(run)
            }
            ProjectOp::RevertAllEdits => {
                let run = {
                    let server = self.pool.lens_session_mut()?.client_mut()?;
                    self.project.revert_all_edits(server).await
                };
                self.record_project_edit_run(run)
            }
            ProjectOp::ClearDebugEdits => {
                let run = {
                    let server = self.pool.lens_session_mut()?.client_mut()?;
                    self.project.clear_debug_edits(server).await
                };
                self.record_project_edit_run(run)
            }
        }
    }

    async fn execute_slot_edit_op(&mut self, op: SlotEditOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.apply_slot_edit(server, op).await
        };
        self.record_project_edit_run(run)
    }

    async fn execute_asset_edit_op(&mut self, op: AssetEditOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.apply_asset_edit(server, op).await
        };
        self.record_project_edit_run(run)
    }

    /// One patch-surface verb (D42): kernel-validated document transforms
    /// through the normal ApplyBody path, with the session-local undo
    /// stack. Routed like asset edits — the verb writes assets.
    async fn execute_patch_verb_op(&mut self, op: crate::PatchVerbOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.apply_patch_verb(server, op).await
        };
        self.record_project_edit_run(run)
    }

    /// One Arrange gesture over `editor.json` (unified-editor P2): routed
    /// like the patch verbs — the op writes an asset.
    async fn execute_editor_meta_op(&mut self, op: crate::EditorMetaOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.apply_editor_meta(server, op).await
        };
        self.record_project_edit_run(run)
    }

    /// Settle the `editor.json` loaded-flag: presence via a root listing,
    /// then the normal content fetch — absence is a state, not an error.
    async fn execute_editor_meta_fetch(&mut self, op: crate::EditorMetaFetchOp) -> UiResult {
        let logs = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.fetch_editor_meta(server, &op.artifact).await?
        };
        self.record_logs(logs);
        Ok(UiNotices::new())
    }

    /// Resolve (and cache) an asset's effective editor content so the next
    /// emitted view embeds it. Quiet on success — the refreshed view is the
    /// outcome; server log lines join the ring like any edit run's.
    async fn execute_asset_content_fetch(&mut self, op: AssetContentFetchOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.asset_content(server, &op.artifact).await?
        };
        self.record_logs(run.logs);
        Ok(UiNotices::new())
    }

    /// Agent chat gestures (P5). `Stop` flips the running session's abort
    /// flag; `Send` resolves the shader's run context (source body, fixture
    /// mapping points, binding table) and spawns the agent run — the
    /// dispatch returns immediately, progress arrives as
    /// [`crate::AgentFeedback`] commands.
    async fn execute_agent_op(&mut self, op: crate::AgentOp) -> UiResult {
        match op {
            crate::AgentOp::Stop { artifact } => {
                self.agent.request_stop(self.pool.lens(), &artifact);
                Ok(UiNotices::new())
            }
            crate::AgentOp::Send { artifact, text } => self.agent_send(artifact, text).await,
            crate::AgentOp::RevertToTurn { artifact, turn } => {
                self.agent_revert_to_turn(artifact, turn).await
            }
            crate::AgentOp::ExportDebug { artifact } => {
                let config = self.settings.agent_provider_config();
                self.agent
                    .export_debug(self.pool.lens(), &artifact, config.as_ref())
                    .map_err(UiError::UnsupportedAction)?;
                self.mark_dirty();
                Ok(UiNotices::new())
            }
            crate::AgentOp::UpsertParam {
                artifact,
                seq,
                upsert,
            } => self.agent_upsert_param(artifact, seq, upsert).await,
            crate::AgentOp::DeclareSpace {
                artifact,
                seq,
                declaration,
            } => self.agent_declare_space(artifact, seq, declaration).await,
        }
    }

    /// Execute one history revert: pull the recorded source, restage it
    /// through the SAME `ApplyBody` overlay path a staged agent edit rides
    /// (so dirty state, acks, verdict chasing, and the live sim follow),
    /// then settle the session — bridge `source` mirror + the visible
    /// "reverted to turn N" transcript notice. The next run's
    /// `current_source` reflects the revert through the overlay anyway;
    /// the mirror update keeps the intra-run snapshot coherent too.
    async fn agent_revert_to_turn(
        &mut self,
        artifact: lpc_model::ArtifactLocation,
        turn: u32,
    ) -> UiResult {
        let runtime = self.pool.lens();
        let source = self
            .agent
            .revert_source(runtime, &artifact, turn)
            .map_err(UiError::UnsupportedAction)?;
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project
                .apply_asset_edit(
                    server,
                    crate::AssetEditOp::ApplyBody {
                        artifact: artifact.clone(),
                        bytes: source.as_bytes().to_vec(),
                    },
                )
                .await
        };
        let notices = self.record_project_edit_run(run)?;
        self.agent.record_revert(runtime, &artifact, turn, &source);
        self.mark_dirty();
        Ok(notices)
    }

    /// Execute one agent `upsert_param` dispatch: ONE `PutSlotEdit` batch
    /// on the target node's def artifact through the project controller,
    /// then record the outcome into the session's bridge cell — including
    /// failures, so the awaiting run future reports an actionable host
    /// error instead of timing out.
    async fn agent_upsert_param(
        &mut self,
        artifact: lpc_model::ArtifactLocation,
        seq: u64,
        upsert: lpa_agent::ParamUpsert,
    ) -> UiResult {
        let outcome = match self
            .pool
            .lens_session_mut()
            .and_then(|session| session.client_mut())
        {
            Ok(server) => {
                self.project
                    .upsert_shader_param(server, &artifact, &upsert)
                    .await
            }
            Err(error) => Err(error),
        };
        match outcome {
            Ok((run, rejection)) => {
                self.record_logs(run.logs);
                self.agent.record_write_ack(&artifact, seq, rejection);
                Ok(run.notices)
            }
            Err(error) => {
                self.agent
                    .record_write_ack(&artifact, seq, Some(error.to_string()));
                Err(error)
            }
        }
    }

    /// Execute one agent `declare_space` dispatch — the same batch, ack
    /// and failure-reporting path as [`Self::agent_upsert_param`], over
    /// the space-declaration edit list.
    async fn agent_declare_space(
        &mut self,
        artifact: lpc_model::ArtifactLocation,
        seq: u64,
        declaration: lpa_agent::SpaceDeclaration,
    ) -> UiResult {
        let outcome = match self
            .pool
            .lens_session_mut()
            .and_then(|session| session.client_mut())
        {
            Ok(server) => {
                self.project
                    .declare_shader_space(server, &artifact, &declaration)
                    .await
            }
            Err(error) => Err(error),
        };
        match outcome {
            Ok((run, rejection)) => {
                self.record_logs(run.logs);
                self.agent.record_write_ack(&artifact, seq, rejection);
                Ok(run.notices)
            }
            Err(error) => {
                self.agent
                    .record_write_ack(&artifact, seq, Some(error.to_string()));
                Err(error)
            }
        }
    }

    async fn agent_send(
        &mut self,
        artifact: lpc_model::ArtifactLocation,
        text: String,
    ) -> UiResult {
        let Some(runtime) = self.pool.lens() else {
            return Err(UiError::MissingSession(
                "the agent needs an open project".to_string(),
            ));
        };
        let Some(config) = self.settings.agent_provider_config() else {
            return Err(UiError::UnsupportedFeature(
                "the agent isn't set up yet — configure a provider in Settings (the gear icon)"
                    .to_string(),
            ));
        };
        let Some(provider) = self.agent.build_provider(&config) else {
            return Err(UiError::UnsupportedFeature(
                "the agent provider is not installed in this build".to_string(),
            ));
        };
        let target = self.project.agent_shader_target(&artifact).ok_or_else(|| {
            UiError::UnsupportedAction(format!(
                "no shader node uses {}",
                artifact.file_path().as_str()
            ))
        })?;
        let source = self.agent_asset_text(&artifact).await?.ok_or_else(|| {
            UiError::UnsupportedAction("the shader source is not editable text".to_string())
        })?;
        let (led_points, fixture) = self.agent_fixture_context().await;
        let bindings = target
            .bindings
            .into_iter()
            .map(|binding| lpa_agent::BindingInfo {
                name: binding.name,
                ty: binding.ty,
                value: binding.value.unwrap_or_else(|| "(bus-driven)".to_string()),
            })
            .collect();
        let context = lpa_agent::ShaderContext {
            project_name: self.project.agent_project_name(),
            node_name: target.node_label,
            fixture,
            bindings,
            space: target.space,
        };
        let key = crate::AgentSessionKey::new(runtime, target.node_address);
        self.agent
            .start_run(
                key,
                artifact,
                text,
                provider,
                crate::AgentRunContext {
                    source,
                    led_points,
                    context,
                },
            )
            .map_err(UiError::UnsupportedFeature)?;
        Ok(UiNotices::new())
    }

    /// The overlay-aware text body of `artifact`, fetching and caching the
    /// base body over the lens session when it is not resolved yet.
    async fn agent_asset_text(
        &mut self,
        artifact: &lpc_model::ArtifactLocation,
    ) -> Result<Option<String>, UiError> {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.asset_content(server, artifact).await?
        };
        self.record_logs(run.logs);
        Ok(run.content.text().map(str::to_string))
    }

    /// Gather every fixture's mapping points (union, labeled by fixture
    /// name — plan decision: v1 targets all fixtures) plus the compact
    /// fixture summary for the system prompt. Unreadable or unparsable
    /// fixture defs are skipped; SvgPath mappings contribute no points.
    async fn agent_fixture_context(
        &mut self,
    ) -> (Vec<lps_probe::LedPoint>, Option<lpa_agent::FixtureSummary>) {
        let mut led_points = Vec::new();
        let mut summaries: Vec<lpa_agent::FixtureSummary> = Vec::new();
        for (label, def_artifact) in self.project.agent_fixture_defs() {
            let Ok(Some(body)) = self.agent_asset_text(&def_artifact).await else {
                continue;
            };
            let Ok(def) = lpc_model::NodeDef::from_json_str(&body) else {
                continue;
            };
            let Some(fixture) = def.as_fixture() else {
                continue;
            };
            let mapping = fixture.mapping.value();
            let points = lpc_model::nodes::fixture::generate_mapping_points(mapping, 1, 1);
            summaries.push(lpa_agent::FixtureSummary {
                name: label.clone(),
                led_count: points.len() as u32,
                mapping_kind: mapping_kind_label(mapping).to_string(),
            });
            led_points.extend(points.into_iter().map(|point| lps_probe::LedPoint {
                label: label.clone(),
                channel: point.channel,
                at: point.center,
            }));
        }
        let summary = fold_fixture_summaries(summaries);
        (led_points, summary)
    }

    async fn execute_node_revert_op(&mut self, op: NodeRevertOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.revert_node_edits(server, &op.node).await
        };
        self.record_project_edit_run(run)
    }

    /// The per-node scope of the Clear verb (D7): only this subtree's Debug
    /// overrides go, persisted edits stay.
    async fn execute_node_clear_debug_op(&mut self, op: NodeClearDebugOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.clear_node_debug_edits(server, &op.node).await
        };
        self.record_project_edit_run(run)
    }

    /// Patch-subject pulse (Q27): the selection's lamps blink on the live
    /// sim/hardware via each involved output's `highlight` Debug slot.
    async fn execute_patch_pulse_op(&mut self, op: PatchPulseOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.apply_patch_pulse(server, op).await
        };
        self.record_project_edit_run(run)
    }

    /// Playlist entry-strip click: dispatch the activate-entry runtime
    /// command (the non-overlay command channel). Quiet on acceptance —
    /// the ACTIVE placard follows via the tightened refresh ticks; a
    /// rejection comes back as a warning notice.
    async fn execute_playlist_activate_op(&mut self, op: PlaylistActivateOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.activate_playlist_entry(server, op).await
        };
        self.record_project_edit_run(run)
    }

    /// Panel-control gesture: dispatch the `(scope, channel)` panel write
    /// down the runtime command channel (no overlay, no dirty flag).
    ///
    /// The value is echoed locally FIRST (GV fix 5). A panel-target widget
    /// has no edit buffer behind it, so without the echo its only feedback
    /// was the probe round trip and drags moved at probe cadence; the echo
    /// retires as soon as a snapshot shows the engine holding the writer.
    async fn execute_panel_write_op(&mut self, op: PanelWriteOp) -> UiResult {
        self.project
            .note_panel_write(op.scope, &op.channel, op.value.clone());
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.panel_write(server, op).await
        };
        self.record_project_edit_run(run)
    }

    async fn execute_panel_clear_op(&mut self, op: PanelClearOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.panel_clear(server, op).await
        };
        self.record_project_edit_run(run)
    }

    /// The P11 auto-save switch: project-level, so it takes no scope. Like
    /// the clear path it is quiet on acceptance — the new value comes back
    /// on the next read's `ServerRuntimeStatus`.
    async fn execute_panel_auto_save_op(&mut self, op: PanelAutoSaveOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.set_panel_auto_save(server, op).await
        };
        self.record_project_edit_run(run)
    }

    async fn execute_node_create_op(&mut self, op: NodeCreateOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.create_node(server, op.kind, &op.attach).await
        };
        self.record_project_edit_run(run)
    }

    async fn execute_node_remove_op(&mut self, op: NodeRemoveOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.remove_node(server, &op.node).await
        };
        self.record_project_edit_run(run)
    }

    /// Copy a node to the clipboard. The envelope text goes out through the
    /// injected sink — core never touches `navigator.clipboard`.
    async fn execute_node_copy_op(&mut self, op: NodeCopyOp) -> UiResult {
        let outcome = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.copy_node(server, &op.node).await
        };
        let (run, envelope) = outcome?;
        if let Some(json) = envelope {
            match self.on_copy_text.as_ref() {
                Some(sink) => sink(&json),
                // A host with no clipboard sink installed (tests, a future
                // headless shell) still runs the read and reports it; the
                // text simply has nowhere to go.
                None => log::warn!("copy: no clipboard sink is installed"),
            }
        }
        self.record_project_edit_run(Ok(run))
    }

    /// Export designation (module authoring unit, P3): the manifest patch
    /// runs against the OPEN project's own package handle, so nothing here
    /// touches the catalog lock.
    async fn execute_module_export_op(&mut self, op: ModuleExportOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project
                .set_module_export(server, &op.folder, op.export)
                .await
        };
        self.record_project_edit_run(run)
    }

    /// Vendor a library pattern export into the open project (module
    /// authoring unit, P5). The source bytes come from the library, the
    /// write goes over the wire as an ordinary `CreateNode`.
    async fn execute_node_import_op(&mut self, op: NodeImportOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project
                .import_pattern(server, &op.package_uid, &op.export)
                .await
        };
        // The vendored files landed in the library through the create's
        // save-pull, so the gallery — and the picker's own import list —
        // are stale until the next settle.
        self.request_library_refresh();
        self.record_project_edit_run(run)
    }

    async fn execute_node_paste_op(&mut self, op: NodePasteOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project
                .paste_node(server, &op.envelope, &op.attach)
                .await
        };
        self.record_project_edit_run(run)
    }

    /// Fold an edit run's server log lines into the bounded ring and surface
    /// its notices as the dispatch outcome.
    fn record_project_edit_run(&mut self, run: Result<ProjectEditRun, UiError>) -> UiResult {
        let run = run?;
        self.record_logs(run.logs);
        Ok(run.notices)
    }

    async fn open_provider_link_only(
        &mut self,
        provider_id: LinkProviderKind,
        updates: UxUpdateSink,
    ) -> UiResult {
        self.project.reset();
        // Quiesce the DEVICE slot this recovery open replaces; a sim
        // session is untouched (P2 coexistence). ⚠️ Still the OLDEST
        // board — the recovery open has no session to name yet, so it is
        // the connect flow's problem (M5), not an op-targeting one.
        if let Some(id) = self
            .pool
            .oldest_device_session()
            .map(crate::RuntimeSession::id)
            && let Ok(session) = self.pool.device_session_mut(id)
        {
            session.disconnect_server();
        }
        let outcome = self.device.open_provider(provider_id).await;
        match outcome {
            Ok(DeviceOpenOutcome::Opened) => {
                self.pool.remove_kind(crate::RuntimeKind::Device);
                Ok(UiNotices::new().with_notice(UiNotice::info(
                    "Choose the device endpoint to open for flashing",
                )))
            }
            Ok(DeviceOpenOutcome::Cancelled { message }) => {
                self.pool.remove_kind(crate::RuntimeKind::Device);
                Ok(UiNotices::new().with_notice(UiNotice::info(message)))
            }
            Ok(DeviceOpenOutcome::SoftFailed) => {
                // The ladder exhausted during a recovery open: the card
                // narrates Not-responding; the dialog user gets one calm
                // pointer instead of a raw error.
                self.pool.remove_kind(crate::RuntimeKind::Device);
                Ok(UiNotices::new().with_notice(UiNotice::info(
                    "The device did not respond. Check the cable, or hold BOOT while plugging in.",
                )))
            }
            // Recovery open: the DeviceSession exists (monitor/management
            // reachable; BlankFlash/Bootloader are fine end states) but the
            // app protocol is deliberately NOT attached.
            Ok(DeviceOpenOutcome::Connected { payload, logs }) => {
                self.record_logs(logs);
                self.install_session(payload).await?;
                updates.emit(UxUpdate::View(self.view()));
                Ok(UiNotices::new().with_notice(UiNotice::info("Device opened for flashing")))
            }
            Err(error) => {
                self.pool.remove_kind(crate::RuntimeKind::Device);
                Err(error)
            }
        }
    }

    async fn connect_server_from_link(
        &mut self,
        id: crate::RuntimeId,
        updates: UxUpdateSink,
    ) -> UiResult {
        // A hardware session stuck in a terminal state needs a rebuilt link
        // generation before the server can attach (reconnect-that-rebuilds);
        // Booting/Ready sessions (and the sim) attach directly.
        let needs_reconnect = {
            let Some(session) = self.pool.session(id) else {
                return Err(UiError::MissingSession(
                    "link connection is not open".to_string(),
                ));
            };
            matches!(
                session.device_state(),
                Some(
                    DeviceState::Gone
                        | DeviceState::Incompatible { .. }
                        | DeviceState::Unresponsive { .. }
                        | DeviceState::BlankFlash
                        | DeviceState::Bootloader
                        | DeviceState::ForeignFirmware
                )
            )
        };
        if needs_reconnect {
            // Quiesce the editor only when it is a lens on THIS session —
            // a project open on the sim survives a device reconnect (P2).
            if self.pool.lens() == Some(id) {
                self.project.reset();
            }
            if let Some(session) = self.pool.session_mut(id) {
                session.disconnect_server();
            }
            let result = {
                let session = self
                    .pool
                    .session(id)
                    .and_then(crate::RuntimeSession::hardware_session)
                    .ok_or_else(|| {
                        UiError::MissingSession(
                            "hardware attachment has no live device session".to_string(),
                        )
                    })?;
                session.reconnect().await
            };
            result.map_err(|error| UiError::Link(error.to_string()))?;
        }
        self.attach_runtime(id, updates).await
    }

    /// Attach the server protocol to the session `id`'s runtime (the
    /// device session's channel for hardware, worker io for the sim) and
    /// run the post-attach sequence: readiness probe, no-firmware /
    /// incompatible handling, connect-as-pull, deploy re-derivation.
    ///
    /// Session-targeted throughout (P2): every state write lands on the
    /// session being attached, never "the lens" — the lens may be on the
    /// OTHER session while a device reconnects under an open sim project.
    async fn attach_runtime(&mut self, id: crate::RuntimeId, updates: UxUpdateSink) -> UiResult {
        let (is_sim, attach_result) = match self.pool.session_mut(id) {
            Some(session) => (session.is_sim(), session.attach_server(updates.clone())),
            None => (
                false,
                Err(UiError::MissingSession(
                    "link connection is not open".to_string(),
                )),
            ),
        };
        match attach_result {
            Ok(()) => {
                let mut outcome =
                    UiNotices::new().with_notice(UiNotice::info("Server protocol connected"));
                updates.emit(UxUpdate::View(self.view()));
                // a home-card open skips the running-project probe: opening
                // is a push of the library head regardless of what runs
                // (D19) — always on the sim (the open flows target it and
                // put the lens on it)
                if self.pending_open.is_some() && is_sim {
                    let open_outcome = self.open_pending_package(updates).await?;
                    outcome.notices.extend(open_outcome.notices);
                    return Ok(outcome);
                }
                if is_sim && let Some(session) = self.pool.session_mut(id) {
                    session.clear_reconcile();
                }
                emit_activity(
                    &updates,
                    UxActivityTarget::pane(ProjectController::NODE_ID),
                    "Checking running projects",
                    "Checking",
                    "Checking server response",
                );
                // The sim WITH the lens auto-connects the editor to
                // whatever runs. Everything else — hardware (roster model,
                // M3: attach observes; editor entry is the explicit D29
                // click) and a sim attaching while the lens is elsewhere
                // (P3: attach never steals the editor) — probes readiness
                // only. The probe still issues the first wire request
                // either way, so readiness settles and NoFirmware/
                // Incompatible classify.
                let lens_bound = self.pool.lens() == Some(id);
                let probe = if is_sim && lens_bound {
                    self.connect_running_project_if_available(updates.clone())
                        .await
                } else {
                    self.probe_server_readiness(id).await
                };
                let auto_connect = match probe {
                    Ok(auto_connect) => auto_connect,
                    Err(error) => {
                        // This session's own streams, onto this session's
                        // console tail (D42) — the ring would swallow them.
                        // A failed attach is exactly when they are worth
                        // reading, and the setup wizard's terminal renders
                        // that same tail: on the bench (2026-08-08) the
                        // connect's whole narration was recorded here, into
                        // the global ring, moments before the wizard looked
                        // at the session and found nothing.
                        let pending_logs = self
                            .pool
                            .session_mut(id)
                            .map(|session| session.take_pending_logs())
                            .unwrap_or_default();
                        self.record_session_logs(id, pending_logs);
                        let device_logs = self.device.take_pending_device_logs();
                        self.record_session_logs(id, device_logs);
                        // Quiesce the editor only when it is a lens on the
                        // failing session (P2: a project open on the sim
                        // survives a failed device attach).
                        if self.pool.lens() == Some(id) {
                            self.project.reset();
                        }
                        if matches!(error, UiError::NoFirmwareDetected(_)) {
                            self.push_log(UiLogDraft::new(
                                UiLogLevel::Info,
                                UiLogOrigin::Studio,
                                "No LightPlayer firmware detected during server readiness",
                            ));
                            if let Some(session) = self.pool.session_mut(id) {
                                session.fail_no_firmware();
                            }
                            // now the dialog's Blank state is the truth
                            return Ok(UiNotices::new().with_notice(UiNotice::info(
                                "No LightPlayer firmware detected; flash firmware onto the selected ESP32",
                            )));
                        }
                        // Incompatible firmware (hello gate): surface the
                        // reflash affordance instead of a dead-end error —
                        // reflashing is the ONE way out, and it must stay
                        // reachable (explicit, never automatic).
                        if matches!(
                            self.device_state_for(id),
                            Some(DeviceState::Incompatible { .. })
                        ) {
                            self.push_log(UiLogDraft::new(
                                UiLogLevel::Warn,
                                UiLogOrigin::Studio,
                                format!("device firmware is incompatible: {error}"),
                            ));
                            if let Some(session) = self.pool.session_mut(id) {
                                session.fail(error.to_string());
                            }
                            return Ok(UiNotices::new().with_notice(UiNotice::info(
                                "Device firmware is incompatible with this Studio; update the firmware",
                            )));
                        }
                        self.push_log(UiLogDraft::new(
                            UiLogLevel::Error,
                            UiLogOrigin::Studio,
                            format!("server readiness probe failed: {error}"),
                        ));
                        if let Some(session) = self.pool.session_mut(id) {
                            session.fail(error.to_string());
                        }
                        return Err(error);
                    }
                };
                match auto_connect {
                    AutoProjectConnect::Connected { synced } => {
                        outcome = outcome.with_notice(project_sync_notice(
                            synced,
                            "Connected running project",
                            "Connected running project; project sync needs attention",
                        ));
                    }
                    AutoProjectConnect::SelectionRequired => {
                        outcome = outcome.with_notice(UiNotice::info("Choose running project"));
                    }
                    AutoProjectConnect::NotFound if is_sim && lens_bound => {
                        let demo_outcome = self.load_demo_project(updates).await?;
                        outcome.notices.extend(demo_outcome.notices);
                    }
                    AutoProjectConnect::NotFound => {}
                }
                // connect-is-a-pull (D8): bank + classify the device's
                // copy — AFTER the readiness probe, so the wire is ready
                // and `has_lightplayer_state` is settled. Hardware only —
                // the sim is not a device (D22). Failures are logged,
                // never fatal (flash/erase must stay reachable).
                if !is_sim {
                    // Multi-device M3: pull the session being ATTACHED,
                    // not whatever the id-less seam resolves.
                    self.refresh_device_sync_for(id).await;
                }
                Ok(outcome)
            }
            Err(error) => {
                if let Some(session) = self.pool.session_mut(id) {
                    session.fail(error.to_string());
                }
                Err(error)
            }
        }
    }

    async fn connect_running_project(&mut self, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Connecting project",
            "Connecting",
            "Checking loaded projects",
        );
        let result = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.connect_running_project(server).await
        };
        match result {
            Ok(ProjectConnectResult::Connected { logs }) => {
                self.record_logs(logs);
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(UiNotices::new().with_notice(project_sync_notice(
                    sync.synced,
                    "Connected running project",
                    "Connected running project; project sync needs attention",
                )))
            }
            Ok(ProjectConnectResult::SelectionRequired { logs }) => {
                self.record_logs(logs);
                Ok(UiNotices::new().with_notice(UiNotice::info("Choose running project")))
            }
            Ok(ProjectConnectResult::NotFound { logs }) => {
                self.record_logs(logs);
                Ok(UiNotices::new().with_notice(UiNotice::info("No running project found")))
            }
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Error,
                    UiLogOrigin::Studio,
                    error.to_string(),
                ));
                self.project.fail(error.to_string());
                Err(error)
            }
        }
    }

    /// Observation-only readiness probe: issue the wire's first request on
    /// session `id` so readiness settles (and NoFirmware/Incompatible
    /// surface through the same error path as the auto-connect probe)
    /// WITHOUT connecting the editor to anything the runtime runs —
    /// hardware attach is observation (roster model; editor entry is the
    /// explicit D29 click), and a sim attaching without the lens must not
    /// steal the mirror (P3).
    async fn probe_server_readiness(
        &mut self,
        id: crate::RuntimeId,
    ) -> Result<AutoProjectConnect, UiError> {
        let catalog = {
            let server = self
                .pool
                .session_mut(id)
                .ok_or_else(|| {
                    UiError::MissingSession("runtime session is not attached".to_string())
                })?
                .client_mut()?;
            server.list_loaded_projects().await?
        };
        self.record_logs(catalog.logs);
        Ok(AutoProjectConnect::NotFound)
    }

    async fn connect_running_project_if_available(
        &mut self,
        updates: UxUpdateSink,
    ) -> Result<AutoProjectConnect, UiError> {
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Checking running projects",
            "Checking",
            "Checking loaded projects",
        );
        let result = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project
                .connect_running_project_if_available(server)
                .await
        };
        match result? {
            ProjectConnectResult::Connected { logs } => {
                self.record_logs(logs);
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(AutoProjectConnect::Connected {
                    synced: sync.synced,
                })
            }
            ProjectConnectResult::SelectionRequired { logs } => {
                self.record_logs(logs);
                Ok(AutoProjectConnect::SelectionRequired)
            }
            ProjectConnectResult::NotFound { logs } => {
                self.record_logs(logs);
                Ok(AutoProjectConnect::NotFound)
            }
        }
    }

    async fn connect_loaded_project(&mut self, handle_id: u32, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Connecting project",
            "Connecting",
            "Loading project shape",
        );
        let result = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.connect_loaded_project(server, handle_id).await
        };
        match result {
            Ok(logs) => {
                self.record_logs(logs);
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(UiNotices::new().with_notice(project_sync_notice(
                    sync.synced,
                    "Connected running project",
                    "Connected running project; project sync needs attention",
                )))
            }
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Error,
                    UiLogOrigin::Studio,
                    error.to_string(),
                ));
                self.project.fail(error.to_string());
                Err(error)
            }
        }
    }

    async fn load_demo_project(&mut self, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Loading demo project",
            "Loading",
            "Uploading demo project",
        );
        let result = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.load_demo_project(server).await
        };
        match result {
            Ok(logs) => {
                self.record_logs(logs);
                self.note_sim_loaded_project();
                self.stand_down_sim_setup(updates.clone()).await?;
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(UiNotices::new().with_notice(project_sync_notice(
                    sync.synced,
                    "Demo project loaded",
                    "Demo project loaded; project sync needs attention",
                )))
            }
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Error,
                    UiLogOrigin::Studio,
                    error.to_string(),
                ));
                self.project.fail(error.to_string());
                Err(error)
            }
        }
    }

    /// The docs-sim bootstrap ([`ProjectOp::OpenDocsExample`], interactive
    /// docs D1/D2): ensure THIS controller's browser-worker sim is
    /// connected, put the lens on it, and deploy a compiled-in example
    /// directly — never through the library. A docs page's leased
    /// controller dispatches this as its first action; re-dispatching on
    /// the live sim is the docs "reset" (pristine re-deploy of the same
    /// files). Never dispatched by any app surface.
    async fn open_docs_example(&mut self, example_id: &str, updates: UxUpdateSink) -> UiResult {
        match self.pool.sim_session() {
            // Live sim: the reset path. Lens back on it, re-deploy below.
            Some(sim) if matches!(sim.server_state(), ServerState::Connected { .. }) => {
                let id = sim.id();
                self.pool.set_lens(id);
            }
            // A sim in any half-state is unexpected here: the docs host
            // boots fresh controllers and resets only live ones. Refuse
            // loudly instead of guessing at recovery (removing the session
            // without closing its provider would leak the worker).
            Some(_) => {
                return Err(UiError::MissingSession(
                    "the docs simulator is not connected; shut this host down and boot a fresh one"
                        .to_string(),
                ));
            }
            None => {
                emit_activity(
                    &updates,
                    UxActivityTarget::pane(ProjectController::NODE_ID),
                    "Starting simulator",
                    "Starting",
                    "Starting the docs simulator",
                );
                let outcome = self
                    .device
                    .open_provider(LinkProviderKind::BrowserWorker)
                    .await;
                match outcome {
                    Ok(DeviceOpenOutcome::Connected { payload, logs }) => {
                        self.record_logs(logs);
                        let id = self.install_session(payload).await?;
                        // The fresh install claims a lens-less pool's lens;
                        // set it explicitly anyway so the invariant is local.
                        self.pool.set_lens(id);
                        let attach = self
                            .pool
                            .session_mut(id)
                            .ok_or_else(|| {
                                UiError::MissingSession(
                                    "the docs simulator session vanished after install".to_string(),
                                )
                            })?
                            .attach_server(updates.clone());
                        if let Err(error) = attach {
                            self.pool.remove_kind(crate::RuntimeKind::Sim);
                            return Err(error);
                        }
                    }
                    Ok(_) => {
                        self.pool.remove_kind(crate::RuntimeKind::Sim);
                        return Err(UiError::MissingSession(
                            "the docs simulator did not connect".to_string(),
                        ));
                    }
                    Err(error) => {
                        self.pool.remove_kind(crate::RuntimeKind::Sim);
                        return Err(error);
                    }
                }
            }
        }
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Opening example",
            "Opening",
            "Pushing the example to the docs simulator",
        );
        let result = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.load_example_direct(server, example_id).await
        };
        match result {
            Ok(logs) => {
                self.record_logs(logs);
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(UiNotices::new().with_notice(project_sync_notice(
                    sync.synced,
                    "Example running",
                    "Example running; project sync needs attention",
                )))
            }
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Error,
                    UiLogOrigin::Studio,
                    error.to_string(),
                ));
                self.project.fail(error.to_string());
                Err(error)
            }
        }
    }

    async fn disconnect_project(&mut self) -> UiResult {
        self.project.disconnect();
        Ok(UiNotices::new().with_notice(UiNotice::info("Project disconnected")))
    }

    /// Record what a just-landed load-as-push put on the lens SIM session
    /// — the live sim card's identity evidence (D36) and the project
    /// card's "Running in simulator" pairing key. No-op when the lens is
    /// not on a sim or the open carried no library identity (the storeless
    /// demo path); the record outlives the lens (detach keeps the sim
    /// running) and dies with the session.
    ///
    /// The sim's BOARD identity rides along (vision D4: the sim inherits
    /// its board from the project it runs) — the project's advisory
    /// manifest `target`, which is where that fact persists. It follows the
    /// project exactly: an untargeted project leaves the sim with no board,
    /// which is today's behavior everywhere until the wizard generates
    /// targeted projects.
    fn note_sim_loaded_project(&mut self) {
        let project = self
            .project
            .active_library_uid()
            .zip(self.project.active_library_slug());
        let target = self.project.active_target();
        if let Some((uid, name)) = project
            && let Ok(session) = self.pool.lens_session_mut()
            && session.is_sim()
        {
            session.set_sim_loaded_project(Some(crate::SimLoadedProject { uid, name }));
            session.set_sim_board_id(target);
        }
    }

    /// A project running on the simulator answers the setup wizard's board
    /// question, so a flow still sitting at its picker stands down (G1b
    /// ruling 6, 2026-08-05 — "Open in sim" used to leave the sim reading
    /// "select a board" for a board it had already inherited).
    ///
    /// Called from every shape an open-in-sim lands in: the push
    /// ([`Self::open_pending_package`], where
    /// [`Self::note_sim_loaded_project`] has just set the board), the demo
    /// load, and the D37 re-attach, where the project landed on an earlier
    /// click and nothing loads at all.
    ///
    /// The board is read back through the same D4 accessor the card's
    /// "as \<board\>" line uses, so the wizard and the card can only ever
    /// agree. An untargeted project infers nothing, and the reducer keeps
    /// the picker up for exactly that case.
    ///
    /// Gated on the SIM flow: a hardware wizard is working through its own
    /// board on the end of a cable, and a project opening on the simulator
    /// is not its business. The reducer refuses it a second time on
    /// capabilities (§7.14), so neither side depends on the other's check.
    async fn stand_down_sim_setup(&mut self, updates: UxUpdateSink) -> UiResult {
        if !self.setup.as_ref().is_some_and(|session| session.sim) {
            return Ok(UiNotices::new());
        }
        if !self
            .pool
            .lens_session()
            .is_some_and(crate::RuntimeSession::is_sim)
        {
            return Ok(UiNotices::new());
        }
        let board_id = self.lens_board_id().map(str::to_string);
        let Some(session) = self.setup.as_mut() else {
            return Ok(UiNotices::new());
        };
        let commands = session
            .flow
            .handle(crate::SetupEvent::SetUpElsewhere { board_id });
        // Through the executor loop like every other reduction, which is
        // also what drops a flow that just closed. Boxed because the two
        // are mutually recursive by TYPE — the loop can run a command that
        // opens a package, and a package open is what calls this — even
        // though this edge asks for no commands at all.
        Box::pin(self.run_setup_commands(commands, updates)).await
    }

    /// Detach the editor lens (runtime-pool P3): the mirror drops, every
    /// session STAYS in the pool — worker running, wire client attached,
    /// device reconcile state intact. The gallery-return route policy
    /// dispatches this; explicit disconnect affordances keep their full
    /// teardown meaning ([`Self::disconnect_device`]).
    ///
    /// Quiescing is the actor's serialized dispatch (verified, per the P3
    /// contract): every edit dispatch is fully awaited — its ack landed —
    /// before the next queued command runs, and the op's Foreground class
    /// cancels an in-flight passive pull at a frame boundary before this
    /// executes. By the time we run, no edit ack is in flight; acked
    /// overlay state is server-side and survives for re-attach.
    fn detach_lens(&mut self) -> UiResult {
        self.quiesce_lens();
        Ok(UiNotices::new())
    }

    /// Drop the mirror's session binding: drain the departing lens
    /// session's buffered wire logs into its console tail (D42 — the
    /// card's console carries them once the gallery shows; nothing
    /// strands while detached), reset the mirror (edit state lives with
    /// the lens — Q1/Q4 of the roadmap DQ record), release the lens id.
    /// Sessions untouched.
    fn quiesce_lens(&mut self) {
        let lens = self.pool.lens();
        let pending = self
            .pool
            .lens_session_mut()
            .map(|session| session.take_pending_logs())
            .unwrap_or_default();
        match lens {
            Some(id) => self.record_session_logs(id, pending),
            None => self.record_logs(pending),
        }
        self.project.reset();
        self.pool.detach_lens();
    }

    /// The D29 attach ([`ProjectOp::OpenDeviceProject`]): put the editor
    /// lens on the device session and open its running project.
    ///
    /// `uid: None` (the card click) targets the attached device session.
    /// `uid: Some` (the `#/device/<uid>` route — D37) attaches the
    /// existing session when its identity matches; otherwise it runs the
    /// M1 granted-port connect first, then attaches. Soft connect endings
    /// (chooser opened / cancelled) and non-Ready devices return their
    /// notices without moving the lens — the gallery's connect evidence
    /// narrates the card honestly; no new UI.
    async fn open_device_project(
        &mut self,
        uid: Option<String>,
        updates: UxUpdateSink,
    ) -> UiResult {
        let session_uid = |session: &crate::RuntimeSession| {
            session.device_uid().or_else(|| {
                session
                    .device_sync()
                    .and_then(|sync| sync.identity.as_ref())
                    .map(|identity| identity.uid.clone())
            })
        };
        if let Some(session) = self.pool.oldest_device_session() {
            let matches = match &uid {
                Some(uid) => session_uid(session).as_deref() == Some(uid.as_str()),
                None => true,
            };
            if matches {
                let id = session.id();
                return self.attach_lens(id, updates).await;
            }
            // A DIFFERENT device is attached: refuse rather than tear it
            // down on a possibly-failing reconnect — routes never
            // sacrifice a live session (explicit disconnect affordances
            // keep that meaning).
            return Err(UiError::UnsupportedAction(
                "A different device is connected — disconnect it first".to_string(),
            ));
        }
        let Some(uid) = uid else {
            return Err(UiError::MissingSession(
                "no device is connected".to_string(),
            ));
        };
        // Route reload (D37): connect through the granted port (M1's
        // direct path; the full auto-connect ladder is M6), then attach.
        let outcome = self.device.reconnect_granted_device(Some(uid)).await;
        let mut notices = self
            .settle_connect_outcome(crate::RuntimeKind::Device, outcome, updates.clone())
            .await?;
        let connected = self
            .pool
            .oldest_device_session()
            .is_some_and(crate::RuntimeSession::is_connected);
        if !connected {
            // Chooser opened, cancelled, or a non-Ready device (blank /
            // foreign / incompatible): the card carries the state; the
            // lens stays where it is.
            return Ok(notices);
        }
        let id = self
            .pool
            .oldest_device_session()
            .map(crate::RuntimeSession::id)
            .unwrap_or_else(|| unreachable!("a connected device session exists"));
        let attach = self.attach_lens(id, updates).await?;
        notices.notices.extend(attach.notices);
        Ok(notices)
    }

    /// Attach the editor lens to session `id` and rebuild the mirror
    /// against that session's client via the existing connect sequence
    /// (`connect_running_project` → `sync_loaded_project`), for BOTH kinds
    /// (P3). A mirror open on another session quiesces first; that session
    /// stays in the pool.
    pub(crate) async fn attach_lens(
        &mut self,
        id: crate::RuntimeId,
        updates: UxUpdateSink,
    ) -> UiResult {
        let connected = self
            .pool
            .session(id)
            .ok_or_else(|| UiError::MissingSession("runtime session is not attached".to_string()))?
            .is_connected();
        if !connected {
            return Err(UiError::MissingSession(
                "server client is not connected".to_string(),
            ));
        }
        if self.pool.lens() != Some(id) {
            self.quiesce_lens();
            self.pool.set_lens(id);
        }
        // Library sync must target the dir this runtime ACTUALLY serves:
        // a device's storage dir is discovered at connect (CLI uploads /
        // older pushes use dirs other than the sim's default slot) — the
        // sim (no discovered dir) keeps the demo slot. Save-as-pull from
        // the wrong dir silently skipped the library save (2026-07-26).
        let storage_id = self
            .pool
            .session(id)
            .and_then(|session| session.device_storage_id().map(str::to_string))
            .unwrap_or_else(|| {
                crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID.to_string()
            });
        self.project.set_runtime_storage_id(storage_id);
        self.connect_running_project(updates).await
    }

    /// Stop-sim (runtime-pool P3, Q5): destroy THE simulator session —
    /// quiesce the editor when the lens is on it, remove it from the pool,
    /// close the provider session (`worker.terminate()` on the web). Every
    /// other session stays. A failed provider close still removes the
    /// session (the pool is the truth about attachment); the failure lands
    /// in the ring as a warning.
    async fn stop_simulator(&mut self) -> UiResult {
        let sim_id = self
            .pool
            .sim_session()
            .map(crate::RuntimeSession::id)
            .ok_or_else(|| UiError::MissingSession("the simulator is not running".to_string()))?;
        if self.pool.lens() == Some(sim_id) {
            self.quiesce_lens();
        }
        let Some(mut session) = self.pool.remove_kind(crate::RuntimeKind::Sim) else {
            return Err(UiError::MissingSession(
                "the simulator is not running".to_string(),
            ));
        };
        let pending = session.take_pending_logs();
        self.record_logs(pending);
        match session.into_payload() {
            crate::RuntimePayload::Sim(sim) => {
                if let Err(error) = sim.connector.close(&sim.session.id).await {
                    self.push_log(UiLogDraft::new(
                        UiLogLevel::Warn,
                        UiLogOrigin::Studio,
                        format!("simulator session close reported: {error}"),
                    ));
                }
            }
            crate::RuntimePayload::Device(handle) => {
                // Unreachable by construction (`remove_kind(Sim)` returns a
                // sim payload); close defensively rather than leak.
                let _ = handle.close().await;
            }
        }
        if !self.pool.has_session() {
            // Nothing attached anymore: return the connect flow to the
            // provider catalog, like a full disconnect would.
            self.device.refresh_provider_catalog();
        }
        Ok(UiNotices::new().with_notice(UiNotice::info("Simulator stopped")))
    }

    async fn refresh_project(&mut self, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Refreshing project",
            "Refreshing",
            "Reading project state",
        );
        updates.emit(UxUpdate::View(self.view()));
        let sync = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.refresh_project(server).await?
        };
        self.record_project_sync_run(&sync);
        updates.emit(UxUpdate::View(self.view()));
        Ok(UiNotices::new().with_notice(project_sync_notice(
            sync.synced,
            "Project refreshed",
            "Project refresh needs attention",
        )))
    }

    /// The P6 pull loop's apply step: re-push the active library project's
    /// on-disk content (already fast-forwarded by the platform edge) to the
    /// running runtime, so the open editor shows what the library now
    /// holds. Quiet on success — the edge raises its own "Updated to the
    /// latest version" toast; a second notice here would double-speak.
    async fn reload_active_project(&mut self, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Updating project",
            "Updating",
            "Reloading the project from its library copy",
        );
        let result = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project.reload_active_from_library(server).await
        };
        match result {
            Ok(logs) => {
                self.record_logs(logs);
                self.note_sim_loaded_project();
                updates.emit(UxUpdate::View(self.view()));
                Ok(UiNotices::new())
            }
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Error,
                    UiLogOrigin::Studio,
                    format!("project reload failed: {error}"),
                ));
                Err(error)
            }
        }
    }

    async fn sync_project_after_attach(
        &mut self,
        updates: UxUpdateSink,
    ) -> Result<ProjectSyncRun, UiError> {
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Syncing project",
            "Syncing",
            "Reading project state",
        );
        updates.emit(UxUpdate::View(self.view()));
        let sync = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            // Gated when the actor installed a platform timer: a device that
            // dies mid-stream then FAILS the sync (with the pull loop's
            // progress counts in the console) instead of hanging "Syncing
            // project" forever. The ungated arm survives for timer-less
            // constructions (unit tests drive the controller directly).
            match self.sync_timer.clone() {
                Some(factory) => {
                    let deadline =
                        lpa_client::ProgressDeadline::new(crate::PASSIVE_REFRESH_DEADLINE, {
                            move |budget| (factory.borrow_mut())(budget)
                        });
                    self.project
                        .sync_loaded_project_gated(server, deadline, &lpa_client::NeverCancel)
                        .await?
                }
                None => self.project.sync_loaded_project(server).await?,
            }
        };
        self.record_project_sync_run(&sync);
        updates.emit(UxUpdate::View(self.view()));
        Ok(sync)
    }

    fn record_project_sync_run(&mut self, sync: &ProjectSyncRun) {
        // New log lines are a local change the next gate should surface
        // even if the project revision did not move; recording marks dirty
        // and no-ops on an empty batch. The lines are the LENS session's
        // stream, so they land on its console tail (D42) — the card's
        // console has the history when the gallery next shows.
        match self.pool.lens() {
            Some(id) => self.record_session_logs(id, sync.logs.clone()),
            None => self.record_logs(sync.logs.clone()),
        }
    }

    /// Disconnect ONE device session (the card's Danger-zone affordance):
    /// close its link and remove it from the pool — the other sessions (a
    /// second board, the sim) stay attached. The caller resolved `id`
    /// from the card's target (M4). (The pre-M3 shape took EVERY session
    /// down, sim included — the ≤1-era "explicit disconnect = full
    /// teardown" semantics.)
    async fn disconnect_device(&mut self, id: crate::RuntimeId) -> UiResult {
        // The mirror quiesces only when the editor was a lens on THIS
        // session (a project open on the sim or another board survives).
        if self.pool.lens() == Some(id) {
            self.project.reset();
        }
        let session = self.pool.remove_session(id);
        self.record_device_event(
            Some(&id.to_string()),
            None,
            DeviceEventKind::Pool {
                action: "disconnect".to_string(),
                detail: "user".to_string(),
            },
        );
        if let Some(session) = session {
            self.device.disconnect(Some(session.into_payload())).await?;
        }
        self.mark_dirty();
        Ok(UiNotices::new().with_notice(UiNotice::info("Device disconnected")))
    }

    /// Detach the server protocol while keeping the runtime attached (the
    /// device pane's "Disconnect LightPlayer" affordance — the
    /// keep-worker-drop-client precedent P3's lens detach built on).
    /// Re-homed for the pool: the op is a device-pane affordance, so it
    /// targets the HARDWARE session when one exists, the lens session
    /// otherwise; the mirror only quiesces when the lens sat on the
    /// disconnected session (a project open on the sim survives).
    async fn disconnect_lightplayer(&mut self, target: &crate::DeviceTarget) -> UiResult {
        let id = match self.resolve_device_target(target) {
            Ok(id) => id,
            // The sim's own disconnect: no device to name, the lens is
            // the runtime the pane is showing.
            Err(error) => self.pool.lens().ok_or(error)?,
        };
        if self.pool.lens() == Some(id) {
            self.quiesce_lens();
        }
        if let Some(session) = self.pool.session_mut(id) {
            session.clear_reconcile();
            session.disconnect_server();
        }
        Ok(UiNotices::new().with_notice(UiNotice::info("LightPlayer disconnected")))
    }

    /// Ask the connected server to apply `level` at runtime and record the
    /// confirmation as a Server-origin log entry. The console's selector
    /// shows the LENS session's level, so the request targets the lens
    /// session's server (the runtime whose console the user is looking
    /// at). The requested level is tracked optimistically on that session
    /// (no wire read-back); failure surfaces through the normal action
    /// error path.
    async fn set_device_log_level(&mut self, level: UiLogLevel) -> UiResult {
        let mut logs = self
            .pool
            .lens_session_mut()?
            .client_mut()?
            .set_log_level(level)
            .await?;
        logs.push(UiLogDraft::new(
            UiLogLevel::Info,
            UiLogOrigin::Server,
            format!("device log level set to {}", level.label()),
        ));
        self.record_logs(logs);
        if let Ok(session) = self.pool.lens_session_mut() {
            session.set_requested_log_level(level);
        }
        Ok(UiNotices::new())
    }

    async fn reset_device(
        &mut self,
        device_id: crate::RuntimeId,
        updates: UxUpdateSink,
    ) -> UiResult {
        self.run_device_management(
            device_id,
            ManagementFlowSpec {
                request: LinkManagementRequest::ResetRuntime,
                progress_label: "Resetting device",
                reconnect_detail: "Waiting for device boot",
                failed_exit_label: "Close",
                record_captured_logs_on_success: true,
                done_notice: |_| UiNotice::info("Device reset"),
                degrade_subject: "device reset",
                server_reconnect_failed_notice: "Device reset; reconnect after it finishes booting",
                // a runtime reset reboots the device; the project survives.
                // Reset/erase/boot-control all leave the device able to come
                // back by itself.
                awaits_manual_replug: false,
                severs_lens: false,
                result_sink: None,
            },
            updates,
        )
        .await
    }

    async fn provision_firmware(
        &mut self,
        device_id: crate::RuntimeId,
        updates: UxUpdateSink,
        setup_name: Option<String>,
        board_id: Option<String>,
    ) -> UiResult {
        // A flash performed while the chip sits in ROM download mode is a
        // different flow, and the difference is the ENDING: the board does
        // not boot the image it was just given until it is physically
        // replugged. The shape follows the device's actual state rather
        // than which button was pressed, because that is what determines
        // whether waiting for a reattach is sensible.
        let from_recovery = self.device_is_in_recovery_mode(device_id);
        // No image, no flash — there is no fallback build (Yona,
        // 2026-08-03: "either it matches, or its a fail case"). The
        // deployment default used to aim an unidentified board at the C6
        // image and leave the flash-time chip guard to refuse a build
        // nobody chose ("Refusing to flash: this device is ESP32-D0WD-V3
        // … but the image is built for esp32c6"). Refuse HERE, where the
        // evidence is, and say what would fix it.
        let Some(build_id) = self.provisioning_build_id(device_id, board_id.as_deref()) else {
            let detected = self
                .hardware_session_for(device_id)
                .and_then(|session| session.snapshot().detected_chip);
            let subject = match detected
                .as_deref()
                .and_then(lpa_link::chip_id_from_reported)
            {
                Some(chip) => format!("this {chip} device"),
                None => "this device (its chip could not be identified)".to_string(),
            };
            let message = format!(
                "No firmware image matches {subject}. Pick your board on the set-up form — \
                 and if it is not listed, this Studio build ships no image for that chip."
            );
            // The refusal must land ON THE CARD, not only in the console —
            // the gate-1 sitting hit exactly this: "console did say there
            // was no firmware, but nothing in the UI said that" (Yona,
            // 2026-08-03). The card-owned op flow's Failed phase is the
            // surface the user is already looking at, and it carries the
            // copy-details affordance.
            self.device_card_ops.insert(
                device_id,
                Rc::new(RefCell::new(crate::CardOp::failed(
                    "Flashing firmware",
                    message.clone(),
                    "Back to set up",
                ))),
            );
            self.push_log(UiLogDraft::new(
                UiLogLevel::Warn,
                UiLogOrigin::Studio,
                message.clone(),
            ));
            self.mark_dirty();
            updates.emit(UxUpdate::View(self.view()));
            return Ok(UiNotices::new().with_notice(UiNotice::warning(message)));
        };
        let build_id = Some(build_id);
        let mut outcome = self
            .run_device_management(
                device_id,
                ManagementFlowSpec {
                    request: LinkManagementRequest::FlashFirmware { build_id },
                    progress_label: "Flashing firmware",
                    reconnect_detail: if from_recovery {
                        "Unplug the board and plug it back in to start it"
                    } else {
                        "Waiting for firmware boot"
                    },
                    failed_exit_label: "Back to set up",
                    record_captured_logs_on_success: false,
                    done_notice: provision_notice,
                    degrade_subject: "firmware flashed",
                    server_reconnect_failed_notice: if from_recovery {
                        "Firmware flashed. Unplug the board and plug it back in — \
                         it will not start on its own from recovery mode."
                    } else {
                        "Firmware flashed; reconnect the server after the device finishes booting"
                    },
                    awaits_manual_replug: from_recovery,
                    // a flash reboots into new firmware; the stored project
                    // survives and reloads on reattach.
                    severs_lens: false,
                    result_sink: None,
                },
                updates,
            )
            .await?;
        // The setup form's name lands at first post-flash contact
        // (model §1-A): the happy path never detours through
        // Needs-a-name. Only an UNNAMED board takes the name — an update
        // on a named device keeps its name. Naming failure degrades
        // honestly: the flash stands, the card offers naming. A
        // MAC-identified board arrives with a uid and no name (device
        // identity design §3), so the gate reads the NAME; the name
        // itself is a registry write under that uid, and the board's
        // filesystem is never touched.
        let setup_name = setup_name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty());
        if let Some(name) = setup_name
            && self.device_sync_for(device_id).is_some_and(|sync| {
                sync.identity
                    .as_ref()
                    .is_none_or(|identity| identity.name.is_empty())
            })
        {
            match self.run_device_naming(device_id, name).await {
                Ok(identity) => {
                    outcome = outcome.with_notice(UiNotice::info(format!(
                        "This device is now \"{}\"",
                        identity.name
                    )));
                }
                Err(error) => {
                    outcome = outcome.with_notice(UiNotice::info(format!(
                        "Firmware installed, but naming failed: {error} — name it from its card."
                    )));
                }
            }
        }
        // The setup form's board choice lands on the device as
        // `/hardware.json` (D4) — after the naming write so the registry
        // row exists to cache the board. Generic choice (`None`) writes
        // nothing: the compiled-in default stands. Failure degrades
        // honestly — the flash and name stand; the pin map just stays
        // default until the board is set again.
        if let Some(board_id) = board_id.filter(|id| !id.trim().is_empty()) {
            match self.run_hardware_stamp(device_id, &board_id).await {
                Ok(()) => {
                    outcome = outcome.with_notice(UiNotice::info(format!(
                        "Board set to {board_id} — the pin map loads on the next restart."
                    )));
                }
                Err(error) => {
                    outcome = outcome.with_notice(UiNotice::info(format!(
                        "Firmware installed, but board setup failed: {error} — \
                         the compiled-in default pin map stands."
                    )));
                }
            }
        }
        Ok(outcome)
    }

    /// Wipe the device's PROJECT storage back to blank over the live wire
    /// (the Holds-unreadable-data card's way out — model rev 2026-07-26:
    /// the way out is BLANK, never push-over). Firmware stays; the
    /// re-pull reclassifies the board as Connected-empty (landing I3).
    /// The unreadable content cannot be banked (that's what unreadable
    /// means) — the confirm sheet's copy says so plainly; raw-image
    /// backup is the model-§5 backlog item.
    async fn wipe_project(&mut self, device_id: crate::RuntimeId) -> UiResult {
        let storage_id = self
            .pool
            .device_session(device_id)
            .and_then(|session| session.device_storage_id().map(str::to_string))
            .ok_or_else(|| {
                UiError::MissingSession("no device project storage to wipe".to_string())
            })?;
        let logs = self
            .pool
            .device_session_mut(device_id)?
            .client_mut()?
            .replace_project_files(
                &storage_id,
                Vec::<lpa_client::project_deploy::ProjectDeployFile>::new(),
            )
            .await?;
        self.record_logs(logs);
        self.refresh_device_sync_for(device_id).await;
        self.mark_dirty();
        Ok(UiNotices::new().with_notice(UiNotice::info(
            "Wiped — this board is empty now. Pick a project to put on it.",
        )))
    }

    /// Ask the device whether a bootloader is listening, and fold the answer
    /// into the card's open bootloader-entry sheet.
    ///
    /// This is the step that makes the ritual's confirmation real. Without
    /// it the sheet can show steps and then never resolve, which is worse
    /// than showing nothing: the user has no way to tell a failed attempt
    /// from a dead board, which is the exact confusion the flow exists to
    /// remove.
    ///
    /// The probe REBOOTS the device, so it runs only here — on the user's
    /// explicit "I've done that", which is itself the signal that a replug
    /// just happened.
    async fn probe_bootloader_mode(
        &mut self,
        device_id: crate::RuntimeId,
        card_key: String,
        flow: crate::BootloaderEntryFlow,
    ) -> UiResult {
        use crate::CardUiOp;

        // Show the waiting state first: the probe takes seconds (reset,
        // sync, re-enumerate) and a frozen sheet reads as a hang.
        let waiting = flow.begin_waiting();
        self.apply_card_ui_op(CardUiOp::OpenSheet {
            card: card_key.clone(),
            sheet: crate::CardSheet::BootloaderEntry(waiting.clone()),
        });

        let Some(session) = self.hardware_session_for(device_id) else {
            return Err(UiError::MissingSession(
                "no hardware device session to probe".to_string(),
            ));
        };
        let probed = session
            .probe_link_mode(lpa_link::DeviceEventSink::noop())
            .await;

        let settled = match probed {
            Ok(mode) if mode.is_bootloader() => {
                let chip_name = match &mode {
                    lpa_link::DeviceLinkMode::Bootloader { chip_name, .. } => chip_name.clone(),
                    _ => None,
                };
                waiting.on_probe_answered(chip_name)
            }
            // Reached the device but it is not in bootloader mode, OR the
            // probe itself failed. Both mean "that attempt did not land" —
            // NOT "the device is broken", since an app-mode device ignores
            // the handshake too.
            Ok(_) | Err(_) => waiting.on_probe_unanswered(),
        };
        self.apply_card_ui_op(CardUiOp::OpenSheet {
            card: card_key,
            sheet: crate::CardSheet::BootloaderEntry(settled.clone()),
        });

        Ok(if settled.is_confirmed() {
            UiNotices::new().with_notice(UiNotice::info("Device is in recovery mode"))
        } else {
            UiNotices::new()
        })
    }

    /// Whether the managed device is currently sitting in ROM download
    /// mode — the state whose flash needs a manual replug to take effect.
    /// Which firmware build to flash onto the attached device.
    ///
    /// Chip first, because a different ISA cannot execute the image at all,
    /// and the chip is DISCOVERED — the boot-line classifier's ROM banner or
    /// the bootloader probe, not something the user typed. The picked board
    /// refines that when several served builds run on the chip, and also
    /// wins outright when it resolves, because provisioning stamps that
    /// board's runtime manifest into `/hardware.json`: flashing one board's
    /// image while recording another's pin map is worse than the refusal the
    /// flash-time chip guard produces.
    ///
    /// `None` — nothing picked and nothing detected — leaves the provider on
    /// its deployment default, and the guard catches it if that is wrong.
    fn provisioning_build_id(
        &self,
        device_id: crate::RuntimeId,
        board_id: Option<&str>,
    ) -> Option<String> {
        // `detected_chip` arrives in either of two spellings — the ROM boot
        // banner's `esp32c6`, or the bootloader probe's esptool-js
        // description `ESP32-C6 (QFN32) (revision v0.2)` — so it has to be
        // resolved to an id, not merely lowercased.
        let detected_chip = self
            .hardware_session_for(device_id)
            .and_then(|session| session.snapshot().detected_chip)
            .and_then(|chip| lpa_link::chip_id_from_reported(&chip));
        let board = board_id.and_then(lpa_boards::board_by_id);
        lpa_boards::provisioning_build_id(board, detected_chip).map(|build_id| build_id.to_string())
    }

    fn device_is_in_recovery_mode(&self, device_id: crate::RuntimeId) -> bool {
        self.hardware_session_for(device_id).is_some_and(|session| {
            matches!(session.snapshot().state, lpa_link::DeviceState::Bootloader)
        })
    }

    /// Write the boot-control record so the next restart skips project
    /// auto-load.
    ///
    /// Deliberately gentler than [`Self::reset_to_blank`]: nothing is erased,
    /// so the lens is not severed and the user keeps their project. The
    /// device consumes the record as it boots, so this affects exactly one
    /// restart.
    async fn boot_safe_once(
        &mut self,
        device_id: crate::RuntimeId,
        updates: UxUpdateSink,
    ) -> UiResult {
        // From app mode the record is written and the device reboots into
        // its firmware by itself. From RECOVERY mode it cannot: the
        // manually-entered download mode latches until power-on reset
        // (bench-confirmed 2026-07-31), so the ending is the replug
        // instruction — the same physics as the recovery flash.
        let from_recovery = self.device_is_in_recovery_mode(device_id);
        self.run_device_management(
            device_id,
            ManagementFlowSpec {
                request: LinkManagementRequest::start_safe_mode(),
                progress_label: "Arming safe mode",
                reconnect_detail: if from_recovery {
                    "Unplug the board and plug it back in to start it"
                } else {
                    "Restarting in safe mode — if it doesn't reconnect in a \
                     few seconds, unplug the board and plug it back in"
                },
                failed_exit_label: "Back to device",
                record_captured_logs_on_success: false,
                done_notice: boot_safe_once_notice,
                degrade_subject: "safe mode armed",
                server_reconnect_failed_notice: if from_recovery {
                    "Safe mode armed. Unplug the board and plug it back in — \
                     it will start dim, or with nothing loaded on older \
                     firmware."
                } else {
                    "Safe mode armed. If the board doesn't reconnect on its \
                     own, unplug it and plug it back in."
                },
                // ALWAYS the awaiting ending, both modes (bench 2026-07-31):
                // from app mode the board normally returns by itself and a
                // successful reattach clears the op — but when the reattach
                // misses (USB re-enumeration races the rebuild), a bare
                // "Not seen yet" offline card with no guidance is the worst
                // of the endings. An instruction that self-clears on success
                // costs nothing when the happy path lands.
                awaits_manual_replug: true,
                // Nothing is erased — the project is still on the device and
                // the editor's lens stays valid.
                severs_lens: false,
                result_sink: None,
            },
            updates,
        )
        .await
    }

    /// Read the device's filesystem over the bootloader and publish a ZIP of
    /// it for the shell to download.
    ///
    /// This is the operation that makes the originating failure survivable —
    /// the user's work comes off the board BEFORE anything destructive
    /// happens to it — so it deliberately reads like the gentle ops: nothing
    /// is erased, the lens is not severed, and a device in recovery mode is
    /// told it needs a replug rather than being reported as a failure.
    async fn back_up_filesystem(
        &mut self,
        device_id: crate::RuntimeId,
        updates: UxUpdateSink,
    ) -> UiResult {
        let from_recovery = self.device_is_in_recovery_mode(device_id);
        let device_label = self
            .device_sync_for(device_id)
            .and_then(|sync| sync.identity.as_ref())
            .map(|identity| identity.name.clone())
            // an identified-but-unnamed board (device identity design §3)
            // is as nameless as an anonymous one for a FILE name
            .filter(|name| !name.is_empty());
        let sink: Rc<RefCell<Option<LinkManagementResult>>> = Rc::new(RefCell::new(None));
        let mut outcome = self
            .run_device_management(
                device_id,
                ManagementFlowSpec {
                    request: LinkManagementRequest::ReadRawFilesystem,
                    progress_label: "Backing up",
                    reconnect_detail: if from_recovery {
                        "Unplug the board and plug it back in to start it"
                    } else {
                        "Waiting for device boot"
                    },
                    failed_exit_label: "Back to device",
                    record_captured_logs_on_success: false,
                    done_notice: |_| UiNotice::info("Backup read from the device"),
                    degrade_subject: "device backed up",
                    server_reconnect_failed_notice: if from_recovery {
                        "Backup taken. Unplug the board and plug it back in — \
                         it will not start on its own from recovery mode."
                    } else {
                        "Backup taken; reconnect after the device finishes booting"
                    },
                    awaits_manual_replug: from_recovery,
                    // A read writes nothing: the project is still on the
                    // device and a lens on it stays valid.
                    severs_lens: false,
                    result_sink: Some(Rc::clone(&sink)),
                },
                updates,
            )
            .await?;

        let read = match sink.borrow_mut().take() {
            Some(LinkManagementResult::ReadRawFilesystem(read)) => read,
            // The provider answered something else entirely — a dispatch
            // bug, not a device problem. Say so rather than pretending.
            _ => {
                return Err(UiError::Link(
                    "the filesystem read returned no image".to_string(),
                ));
            }
        };
        let archive = crate::app::device::filesystem_backup::build_backup_archive(
            &read.image,
            &crate::app::device::BackupSource {
                chip: read.chip_name.clone(),
                partition_offset: read.region.offset,
                partition_length: read.region.length,
                device_label,
            },
            (self.now_secs)(),
        )
        .map_err(|error| UiError::Link(error.to_string()))?;

        self.device_backup_seq += 1;
        let file_count = archive.manifest.file_count;
        outcome = outcome.with_notice(UiNotice::info(format!(
            "Backed up {file_count} file(s) to {}",
            archive.file_name
        )));
        self.device_backup = Some(crate::UiDeviceBackup {
            seq: self.device_backup_seq,
            file_name: archive.file_name,
            bytes: Rc::from(archive.bytes),
            file_count,
        });
        self.mark_dirty();
        Ok(outcome)
    }

    async fn reset_to_blank(
        &mut self,
        device_id: crate::RuntimeId,
        updates: UxUpdateSink,
    ) -> UiResult {
        self.run_device_management(
            device_id,
            ManagementFlowSpec {
                request: LinkManagementRequest::EraseDeviceFlash,
                progress_label: "Wiping device",
                reconnect_detail: "Checking for LightPlayer firmware",
                failed_exit_label: "Back to set up",
                record_captured_logs_on_success: false,
                done_notice: reset_notice,
                degrade_subject: "device wiped",
                server_reconnect_failed_notice:
                    "Device wiped; reconnect after the device finishes booting",
                // a wipe erases the flash — the project is gone; a lens on
                // this device is severed and the app returns to the gallery.
                // Reset/erase/boot-control all leave the device able to come
                // back by itself.
                awaits_manual_replug: false,
                severs_lens: true,
                result_sink: None,
            },
            updates,
        )
        .await
    }

    /// The shared management orchestration core behind `reset_device` /
    /// `provision_firmware` / `reset_to_blank`: quiesce project+server, run
    /// `DeviceSession::manage` (release → manage → rebuild → re-ready, all
    /// inside the session) with live activity/log capture, then reattach
    /// the server — degrading to an informational notice when the reattach
    /// half fails.
    async fn run_device_management(
        &mut self,
        device_id: crate::RuntimeId,
        spec: ManagementFlowSpec,
        updates: UxUpdateSink,
    ) -> UiResult {
        // Quiesce the editor's live edit-state only when it is a lens on
        // the device being managed (P2: a project open on the sim survives
        // a flash/erase). A destructive wipe additionally DETACHES the lens
        // once the op settles (below) — returning to the gallery — because
        // the project is gone with the flash; a reset/flash keeps the
        // project, so the editor stays put to reload after the reattach.
        let severed_lens = self.pool.lens() == Some(device_id);
        if severed_lens {
            self.project.reset();
        }
        // The CARD-OWNED op flow starts here (model §2): the flow lives
        // on the controller, keyed by the managed SESSION, and survives
        // whatever happens to that session below (I1). The event sink
        // feeds it; the settle half flips it to Failed or clears it.
        let card_op = Rc::new(RefCell::new(crate::CardOp::new(
            format!("{}…", spec.progress_label),
            None,
        )));
        let session_key = device_id.to_string();
        self.device_card_ops.insert(device_id, Rc::clone(&card_op));
        self.record_device_event(
            Some(&device_id.to_string()),
            None,
            DeviceEventKind::Mgmt {
                phase: "start".to_string(),
                label: spec.progress_label.to_string(),
            },
        );
        if let Some(session) = self.pool.session_mut(device_id) {
            session.disconnect_server();
            // The pool refuses a same-kind replace while this runs (DQ-A
            // swap semantics), and the label narrates the device card's
            // Operation-in-flight lane; cleared when the manage half
            // settles.
            session.set_operation(Some(spec.progress_label.to_string()));
        }
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        // MOUNT THE OVERLAY BEFORE THE WORK STARTS. The op slot went in
        // just above, and only a full view build carries it onto a card
        // (`overlay_card_ui`) — but everything below holds `&mut self`
        // until the operation ends, so this is the last chance to build
        // one. Without it the first view wearing the op is the one
        // `attach_runtime` emits AFTER the flash, which is what made a
        // minute-long install look like a dead button.
        self.mark_dirty();
        updates.emit(UxUpdate::View(self.view()));
        let event_sink = management_event_sink(
            updates.clone(),
            Rc::clone(&captured_logs),
            Rc::clone(&card_op),
            session_key.clone(),
            spec.reconnect_detail,
        );
        let manage_result = {
            let session = match self.hardware_session_for(device_id) {
                Some(session) => session,
                None => {
                    if let Some(session) = self.pool.session_mut(device_id) {
                        session.set_operation(None);
                    }
                    // The op never started — clean abort, no Failed render.
                    self.device_card_ops.remove(&device_id);
                    return Err(UiError::MissingSession(
                        "no hardware device session for management".to_string(),
                    ));
                }
            };
            // Cloned so the spec stays whole: the reattach half below still
            // needs its copy for `reattach_failure_op`.
            session.manage(spec.request.clone(), event_sink).await
        };
        // The manage half settled (either way): session replaces unblock
        // and the card's operation narration clears.
        if let Some(session) = self.pool.session_mut(device_id) {
            session.set_operation(None);
        }
        self.record_device_event(
            Some(&device_id.to_string()),
            None,
            DeviceEventKind::Mgmt {
                phase: "settle".to_string(),
                label: format!(
                    "{} — {}",
                    spec.progress_label,
                    if manage_result.is_ok() {
                        "ok"
                    } else {
                        "failed"
                    }
                ),
            },
        );
        let management = match manage_result {
            Ok(management) => management,
            Err(error) => {
                self.record_logs(core::mem::take(&mut *captured_logs.borrow_mut()));
                // The card renders the failure with its ONE exit (I4);
                // the flow stays until the user takes it (ClearOp).
                *card_op.borrow_mut() = crate::CardOp::failed(
                    format!("{} failed", spec.progress_label),
                    error.to_string(),
                    spec.failed_exit_label,
                );
                self.mark_dirty();
                return Err(UiError::Link(error.to_string()));
            }
        };
        if spec.record_captured_logs_on_success {
            self.record_logs(core::mem::take(&mut *captured_logs.borrow_mut()));
        }
        self.record_logs(management_result_logs(&management.result));

        let mut outcome = UiNotices::new().with_notice((spec.done_notice)(&management.result));
        // Hand the raw result on AFTER the log replay and the notice, so the
        // move costs nothing (a filesystem image is ~1 MB).
        if let Some(sink) = &spec.result_sink {
            *sink.borrow_mut() = Some(management.result);
        }
        // The reattach half is the op's AwaitingDevice phase (I2) — the
        // overlay stays up, narrating the expected gap. It is a LONG
        // phase (the board reboots and re-enumerates), so the delta goes
        // out too: the dispatch has not returned, and only a full
        // snapshot would otherwise carry the phase change.
        *card_op.borrow_mut() = crate::CardOp::awaiting(spec.reconnect_detail);
        updates.emit(UxUpdate::CardOp {
            session_key: session_key.clone(),
            op: card_op.borrow().clone(),
        });
        self.mark_dirty();
        updates.emit(UxUpdate::View(self.view()));
        // The link was already rebuilt inside `manage`; what remains is the
        // server reattach + post-attach sequence on the managed session.
        match self.attach_runtime(device_id, updates).await {
            Ok(mut attach_outcome) => {
                outcome.notices.append(&mut attach_outcome.notices);
                if spec.awaits_manual_replug && self.device_is_in_recovery_mode(device_id) {
                    // Attached — but the board landed back in the BOOTLOADER.
                    // A C6 over USB-Serial-JTAG re-enters download mode on
                    // the post-write RTS reset (bench, 2026-07-31), so the
                    // reattach "succeeds" into a recovery session and the
                    // old clear-on-Ok dropped the user on the recovery card,
                    // which advises installing the firmware they may have
                    // JUST installed. For an op that only finishes when the
                    // board really boots, this ending is the replug
                    // instruction, same as the reattach-miss arm below.
                    self.push_log(UiLogDraft::new(
                        UiLogLevel::Info,
                        UiLogOrigin::Studio,
                        format!(
                            "{} — the board stays in the bootloader until replugged",
                            spec.degrade_subject
                        ),
                    ));
                    *card_op.borrow_mut() = reattach_failure_op(&spec, "");
                    outcome =
                        outcome.with_notice(UiNotice::info(spec.server_reconnect_failed_notice));
                } else {
                    // Landed (I3): the flow ends; the card re-derives and its
                    // Status tab announces what's next.
                    self.device_card_ops.remove(&device_id);
                }
            }
            // The device was never going to come back by itself. Stay in
            // the AwaitingDevice phase with the instruction that ends it,
            // rather than calling a successful flash a failure and marking
            // the session failed for doing exactly what it must do.
            Err(_) if spec.awaits_manual_replug => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Info,
                    UiLogOrigin::Studio,
                    format!(
                        "{} — waiting for the board to be replugged",
                        spec.degrade_subject
                    ),
                ));
                *card_op.borrow_mut() = reattach_failure_op(&spec, "");
                outcome = outcome.with_notice(UiNotice::info(spec.server_reconnect_failed_notice));
            }
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!(
                        "{} but server reconnect failed: {error}",
                        spec.degrade_subject
                    ),
                ));
                if let Some(session) = self.pool.session_mut(device_id) {
                    session.fail(error.to_string());
                }
                // Failed reattach renders on the card too (I4) — with the
                // same single exit, not a silent fall-through.
                *card_op.borrow_mut() = reattach_failure_op(&spec, &error.to_string());
                outcome = outcome.with_notice(UiNotice::info(spec.server_reconnect_failed_notice));
            }
        }
        // A destructive wipe severs the editor AFTER the reattach settles:
        // the project went with the flash, so detach the lens (the app
        // returns to the gallery) and say why. Detaching earlier would
        // change the reattach outcome — the managed session must stay the
        // lens through `attach_runtime` (device-lifecycle P3).
        if severed_lens && spec.severs_lens {
            self.quiesce_lens();
            outcome = outcome.with_notice(UiNotice::info(
                "This project is no longer on this device — back to your devices.",
            ));
        }
        Ok(outcome)
    }

    fn project_is_loaded(&self) -> bool {
        matches!(self.project.snapshot().state, ProjectState::Ready { .. })
    }
}

/// Cross-module test builders. The actor tests live in a sibling module and
/// cannot reach the private `device`/`pool`/`project` fields, so these
/// `pub(crate)` helpers assemble a connected controller for them.
#[cfg(test)]
impl StudioController {
    /// Attach stubbed hardware in the given device state (view/derivation
    /// tests that must not script a whole fake device). Replaces the
    /// session PAYLOAD in place when a session exists — the retired
    /// attachment and server slots were independently settable, so an
    /// injected client survives.
    pub(crate) fn set_stub_device_for_test(&mut self, state: lpa_link::DeviceState) {
        self.set_stub_payload_for_test(crate::RuntimePayload::stub_device_for_test(state));
    }

    /// Attach a stubbed SIMULATOR payload and mark the flow `Connected` —
    /// the "connected but not hardware" fixture.
    pub(crate) fn set_stub_sim_for_test(&mut self) {
        self.set_stub_payload_for_test(crate::RuntimePayload::stub_sim_for_test());
    }

    fn set_stub_payload_for_test(&mut self, payload: crate::RuntimePayload) {
        match self.pool.lens_session_mut() {
            Ok(session) => session.set_payload_for_test(payload),
            Err(_) => {
                self.pool
                    .install(payload)
                    .unwrap_or_else(|refusal| panic!("stub install refused: {}", refusal.message));
            }
        }
        self.device.set_stub_connected_flow_for_test();
    }

    /// Install a stubbed SIM session ALONGSIDE whatever is attached (the
    /// P2 coexistence fixture — `set_stub_sim_for_test` would replace the
    /// lens session's payload instead) and give it an injected wire
    /// client. Install preserves a held lens (P3): the sim only claims it
    /// when nothing does; open flows move it explicitly.
    pub(crate) fn install_stub_sim_with_client_for_test(
        &mut self,
        client: crate::StudioServerClient,
    ) -> crate::RuntimeId {
        let id = self
            .pool
            .install(crate::RuntimePayload::stub_sim_for_test())
            .unwrap_or_else(|refusal| panic!("sim install refused: {}", refusal.message));
        self.pool
            .session_mut(id)
            .expect("just-installed sim session")
            .set_client_for_test(client);
        id
    }

    /// The runtime pool, for e2e assertions about session coexistence.
    #[cfg(test)]
    pub(crate) fn project_for_test(&self) -> &ProjectController {
        &self.project
    }

    pub(crate) fn runtime_pool_for_test(&self) -> &RuntimePool {
        &self.pool
    }

    /// Lift the single-session policy (module doc) for tests whose
    /// subject is the POOL's N-session behavior rather than the web
    /// app's policy on top of it: two boards attached at once, a board
    /// beside an open sim project. The policy's own behavior — the
    /// teardown and the in-flight refusal — is covered against a
    /// controller that keeps it, in `studio_link_e2e_tests`.
    pub(crate) fn allow_multi_session_for_test(&mut self) {
        self.multi_session_for_test = true;
    }

    /// Mark an operation in flight on a session, as a deploy or a flash
    /// does — the single-session policy's refusal evidence, without
    /// scripting a whole long-running op.
    pub(crate) fn set_session_operation_for_test(
        &mut self,
        id: crate::RuntimeId,
        label: Option<&str>,
    ) {
        self.pool
            .session_mut(id)
            .expect("a session to mark busy")
            .set_operation(label.map(str::to_string));
    }

    /// The sim session's advisory board (D4), for control-DTO
    /// projections — the board a project normally stamps at open.
    pub(crate) fn set_sim_board_for_test(&mut self, board_id: &str) {
        self.pool
            .sim_session_mut()
            .expect("a sim session is installed")
            .set_sim_board_id(Some(board_id.to_string()));
    }

    /// The storage dir library sync targets, for the save-as-pull
    /// wiring regression (2026-07-26).
    #[cfg(test)]
    pub(crate) fn project_runtime_storage_id_for_test(&self) -> &str {
        self.project.runtime_storage_id_for_test()
    }

    /// Test-only: the ONE device a single-device harness attached.
    ///
    /// Production code never asks for "the" device — it names the board
    /// the gesture came from (M4). These harnesses attach exactly one, so
    /// the ambient rule resolves it unambiguously.
    pub(crate) fn the_device_for_test(&self) -> crate::RuntimeId {
        self.ambient_device_id()
            .expect("a device session is attached")
    }

    /// Test-only: a [`crate::DeviceTarget`] naming the one attached
    /// device — what a card-owned op carries once a real UI supplies it.
    pub(crate) fn device_target_for_test(&self) -> crate::DeviceTarget {
        crate::DeviceTarget::card(self.the_device_for_test().to_string())
    }

    /// Test-only: [`Self::refresh_device_sync_for`] against
    /// [`Self::the_device_for_test`].
    pub(crate) async fn refresh_device_sync_for_test(&mut self) {
        let id = self.the_device_for_test();
        self.refresh_device_sync_for(id).await;
    }

    /// Test-only: the setup flow's `ProbeBoard` read against the one
    /// attached device, bound as the flow's device first.
    ///
    /// The escalation inside it needs a REAL link session (a stub has no
    /// hardware session to probe), so its rows live in
    /// `studio_link_e2e_tests` — a sibling module that cannot reach
    /// `setup_device` or `read_setup_probe`.
    pub(crate) async fn setup_probe_for_test(&mut self) -> crate::BoardProbe {
        self.setup_device = Some(self.the_device_for_test());
        self.read_setup_probe().await
    }

    /// Test-only: [`Self::device_sync_for`] against the one attached
    /// device, or `None` when nothing is attached.
    pub(crate) fn device_sync_for_test(&self) -> Option<&DeviceSyncState> {
        self.device_sync_for(self.ambient_device_id()?)
    }

    /// Test-only: the ONE attached device session's frame feed.
    pub(crate) fn card_feed_for_test(&self) -> Option<&crate::CardFeedState> {
        Some(
            self.pool
                .device_session(self.ambient_device_id()?)?
                .card_feed(),
        )
    }

    /// The connect-flow state, for ladder assertions (M6).
    pub(crate) fn device_flow_state_for_test(&self) -> &ConnectFlowState {
        self.device.flow_state()
    }

    /// Push a console line into the DEVICE session's buffer, as the live
    /// event sink would (heartbeat-drain tests).
    pub(crate) fn push_device_console_log_for_test(&mut self, draft: UiLogDraft) {
        let id = self
            .ambient_device_id()
            .expect("a device session is attached");
        self.pool
            .device_session_mut(id)
            .expect("a device session is attached")
            .push_device_console_log_for_test(draft);
    }

    /// Set the lens session's server protocol state directly (the retired
    /// `ServerController::set_state` seam). Requires a stub session.
    pub(crate) fn set_server_state_for_test(&mut self, state: crate::ServerState) {
        self.pool
            .lens_session_mut()
            .expect("a stub session is installed")
            .set_server_state_for_test(state);
    }

    /// Install an injected wire client on the lens session (the retired
    /// `ServerController::set_client_for_test` seam).
    pub(crate) fn set_server_client_for_test(&mut self, client: crate::StudioServerClient) {
        self.pool
            .lens_session_mut()
            .expect("a stub session is installed")
            .set_client_for_test(client);
    }

    /// A fresh controller whose device flow uses the given provider
    /// registry (and poll timers) — the entry point for e2e tests that
    /// drive the REAL provider path (`open_provider → discover →
    /// connect_endpoint → attach`) instead of injecting connections.
    pub(crate) fn with_link_registry_for_test(
        now_secs: impl Fn() -> f64 + 'static,
        registry: lpa_link::providers::LinkProviderRegistry,
    ) -> Self {
        let mut studio = Self::new(now_secs);
        let mut device = DeviceController::with_registry(registry);
        device.set_timers(DeviceController::test_poll_timers());
        studio.device = device;
        studio
    }

    pub(crate) fn connected_with_client_for_test(client: crate::StudioServerClient) -> Self {
        use crate::ProjectInventorySummary;

        let clock = std::rc::Rc::new(std::cell::Cell::new(0.0_f64));
        let mut studio = Self::new({
            let clock = std::rc::Rc::clone(&clock);
            move || clock.get()
        });
        studio.test_clock = Some(clock);
        studio.set_stub_sim_for_test();
        studio.set_server_client_for_test(client);
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        studio
    }

    /// Advance the injected test clock (no-op for controllers built with a
    /// real clock). Lets pacing/heartbeat tests move time past a
    /// completion gap.
    pub(crate) fn advance_clock_for_test(&mut self, secs: f64) {
        if let Some(clock) = &self.test_clock {
            clock.set(clock.get() + secs);
        }
    }

    /// Apply a project view into the owned tree (drives probe scoping).
    pub(crate) fn apply_project_view_for_test(&mut self, view: &lpc_view::ProjectView) {
        self.project.apply_project_view(view).unwrap();
    }

    /// The attached hardware's device-session state, for e2e assertions.
    pub(crate) fn device_state_for_test(&self) -> Option<lpa_link::DeviceState> {
        self.ambient_device_id()
            .and_then(|id| self.device_state_for(id))
    }
}

impl ControllerContext for StudioController {
    fn dispatch(
        &mut self,
        action: UiAction,
    ) -> core::pin::Pin<Box<dyn Future<Output = UiResult> + '_>> {
        Box::pin(StudioController::dispatch(self, action))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AutoProjectConnect {
    Connected { synced: bool },
    SelectionRequired,
    NotFound,
}

/// Human-readable mapping kind for the system prompt's fixture line.
fn mapping_kind_label(mapping: &lpc_model::nodes::fixture::MappingConfig) -> &'static str {
    use lpc_model::nodes::fixture::MappingConfig;
    match mapping {
        MappingConfig::Unset => "unset",
        MappingConfig::PathPoints { .. } => "path points",
        MappingConfig::Map2d { .. } => "map2d document",
    }
}

/// Collapse per-fixture summaries into the prompt's single fixture slot:
/// one fixture passes through; several aggregate (joined names, summed LED
/// count, joined kinds) — v1 targets all fixtures at once.
fn fold_fixture_summaries(
    mut summaries: Vec<lpa_agent::FixtureSummary>,
) -> Option<lpa_agent::FixtureSummary> {
    match summaries.len() {
        0 => None,
        1 => summaries.pop(),
        _ => {
            let name = summaries
                .iter()
                .map(|summary| summary.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let led_count = summaries.iter().map(|summary| summary.led_count).sum();
            let mut kinds: Vec<&str> = summaries
                .iter()
                .map(|summary| summary.mapping_kind.as_str())
                .collect();
            kinds.dedup();
            let mapping_kind = kinds.join(" + ");
            Some(lpa_agent::FixtureSummary {
                name,
                led_count,
                mapping_kind,
            })
        }
    }
}

/// The import/paste confirmation, saying so when the archive arrived at an
/// older format and was migrated on the way in.
///
/// Naming the upgrade is the point: the project the user gets back is not
/// byte-identical to the one they handed over, and the only moment that
/// fact is cheap to state is the moment it happens.
fn import_message(verb: &str, outcome: &crate::app::library::CatalogOutcome) -> String {
    let name = outcome
        .summary
        .as_ref()
        .map(|summary| summary.name.clone())
        .unwrap_or_default();
    match outcome.upgraded_from {
        Some(found) => format!(
            "{verb} {name} — upgraded from format {found} to {}",
            lpc_model::PROJECT_FORMAT_VERSION
        ),
        None => format!("{verb} {name}"),
    }
}

fn project_sync_notice(synced: bool, success: &str, needs_attention: &str) -> UiNotice {
    if synced {
        UiNotice::info(success)
    } else {
        UiNotice::warning(needs_attention)
    }
}

/// Which pool slot a connect flow is aimed at: the browser-worker provider
/// is THE simulator; every other provider class is hardware.
fn runtime_kind_for(provider_id: LinkProviderKind) -> crate::RuntimeKind {
    if provider_id == LinkProviderKind::BrowserWorker {
        crate::RuntimeKind::Sim
    } else {
        crate::RuntimeKind::Device
    }
}

/// Constructor-default randomness: clock-derived bytes. Unique enough
/// A pulled device content's classification as the event trace spells it
/// (part of the JSONL contract — extend, do not rename).
fn device_content_label(content: &DeviceContent) -> &'static str {
    match content {
        DeviceContent::Empty => "empty",
        DeviceContent::Known { .. } => "known",
        DeviceContent::Adopted { .. } => "adopted",
        DeviceContent::PendingIdentity { .. } => "pending-identity",
        DeviceContent::OldFormat { .. } => "old-format",
        DeviceContent::Unreadable { .. } => "unreadable",
    }
}

/// The card-facing content for a board whose project format is not this
/// build's — `None` when the sniff says the project IS current, which is
/// the ordinary case and leaves the caller's own classification standing.
///
/// A manifest that reads as JSON but is not a project manifest at all
/// (no `format`, wrong root shape) lands on the unreadable card rather
/// than on a format claim we cannot back up.
fn device_content_for_format(
    class: &lpa_upgrade::FormatClass,
    project_uid: Option<String>,
    slug: Option<String>,
    observed: lpc_history::ContentHash,
) -> Option<DeviceContent> {
    match class {
        lpa_upgrade::FormatClass::Current => None,
        lpa_upgrade::FormatClass::NotAProject | lpa_upgrade::FormatClass::Unreadable { .. } => {
            Some(DeviceContent::Unreadable {
                detail: class.describe(),
            })
        }
        _ => Some(DeviceContent::OldFormat {
            project_uid,
            slug,
            observed,
            class: class.clone(),
        }),
    }
}

/// The published-frame entries carried by a card-feed read's event stream.
///
/// The feed asks for exactly one probe, so this walks the stream for probe
/// index 0 and reassembles it: a small result arrives whole, and a
/// dome-scale frame arrives as a header plus bounded chunks the transport
/// already validated for coverage. Anything else in the stream (the
/// begin/end revision markers) is not this read's business.
///
/// A malformed stream yields no entries rather than an error: the feed's
/// answer to "no frame this time" is to keep the last one, and there is no
/// user-facing failure to raise for a picture that did not arrive.
fn output_frame_entries(events: &[lpc_wire::ProjectReadEvent]) -> Vec<lpc_wire::OutputFrameEntry> {
    use lpc_wire::{
        OutputFrameProbeResult, ProjectProbeResult, ProjectProbeResultHeader, ProjectReadEvent,
        ProjectReadProbeEvent,
    };

    let mut pending: Option<(ProjectProbeResultHeader, Vec<u8>)> = None;
    for event in events {
        let ProjectReadEvent::Probe { event, .. } = event else {
            continue;
        };
        match event {
            ProjectReadProbeEvent::Result(ProjectProbeResult::OutputFrame(
                OutputFrameProbeResult::Frame { outputs },
            )) => return outputs.clone(),
            ProjectReadProbeEvent::ResultBegin { header, .. } => {
                pending = Some((header.clone(), Vec::new()));
            }
            ProjectReadProbeEvent::ResultBytes { bytes, .. } => {
                if let Some((_, buffer)) = pending.as_mut() {
                    buffer.extend_from_slice(bytes);
                }
            }
            ProjectReadProbeEvent::ResultEnd => {
                let Some((header, bytes)) = pending.take() else {
                    continue;
                };
                if let ProjectProbeResult::OutputFrame(OutputFrameProbeResult::Frame { outputs }) =
                    header.into_result(bytes)
                {
                    return outputs;
                }
            }
            _ => {}
        }
    }
    Vec::new()
}

/// The grant's short id for display: the trailing `port-N` of a
/// browser-serial endpoint id ("browser-serial-esp32-port-2" → "port-2");
/// `None` for ids without that shape (fake/host endpoints, whose full id
/// adds nothing a label doesn't).
fn short_endpoint_id(endpoint_id: &str) -> Option<&str> {
    endpoint_id
        .rfind("-port-")
        .map(|index| &endpoint_id[index + 1..])
}

/// for tests; the web shell replaces it with crypto randomness via
/// [`StudioController::set_random`].
fn clock_fallback_random() -> [u8; 16] {
    use core::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0x5eed);
    let n = COUNTER.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed);
    let a = n.wrapping_mul(0xff51_afd7_ed55_8ccd);
    let b = a ^ (a >> 33);
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&a.to_le_bytes());
    bytes[8..].copy_from_slice(&b.to_le_bytes());
    bytes
}

/// `YYYY-MM-DD-HHMM` in UTC from epoch seconds — the fallback slug stamp
/// for a shell that installed no local one (see
/// [`StudioController::set_local_stamp`]). Howard Hinnant's civil-from-days.
fn utc_slug_stamp(now_secs: f64) -> String {
    let secs = now_secs as i64;
    let days = secs.div_euclid(86_400);
    let seconds_of_day = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}-{:02}{:02}",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
    )
}

fn emit_activity(
    updates: &UxUpdateSink,
    target: UxActivityTarget,
    title: impl Into<String>,
    status: impl Into<String>,
    detail: impl Into<String>,
) {
    updates.emit(UxUpdate::Activity {
        target,
        status: UiStatus::working(status),
        activity: UiActivityView::new(title).with_detail(detail),
    });
}

fn view_actions(view: &UiStudioView) -> Vec<UiAction> {
    let mut actions = Vec::new();
    for pane in &view.panes {
        actions.extend(pane.actions.clone());
        actions.extend(body_actions(&pane.body));
    }
    actions
}

fn body_actions(body: &UiViewContent) -> Vec<UiAction> {
    match body {
        UiViewContent::Text(_)
        | UiViewContent::Progress(_)
        | UiViewContent::Activity(_)
        | UiViewContent::Issue(_)
        | UiViewContent::Metrics(_) => Vec::new(),
        UiViewContent::ProjectEditor(editor) => editor
            .tree
            .roots
            .iter()
            .flat_map(project_tree_item_actions)
            .collect(),
    }
}

fn project_tree_item_actions(
    item: &crate::ProjectNodeTreeItem,
) -> Box<dyn Iterator<Item = UiAction> + '_> {
    Box::new(
        core::iter::once(item.action.clone())
            .chain(item.children.iter().flat_map(project_tree_item_actions)),
    )
}

/// The per-operation shape of one device management flow (reset / flash /
/// wipe): the link request plus the notice/log wording that differs between
/// them. Everything else — quiesce, capture, manage, reopen, reattach,
/// degrade — is shared in `StudioController::run_device_management`.
/// What the card shows when the post-operation reattach does not land.
///
/// Two different situations wear the same error: a device that FAILED to come
/// back, and a device that was never going to. A board flashed from ROM
/// download mode does not boot the new image until it is physically
/// replugged, so treating its non-return as a failure reports a successful
/// flash as a failure — on the one path a user in recovery actually takes.
fn reattach_failure_op(spec: &ManagementFlowSpec, error: &str) -> crate::CardOp {
    if spec.awaits_manual_replug {
        // Not failed: awaiting the one action that finishes the job, with
        // the instruction itself as the narration.
        crate::CardOp::awaiting(spec.reconnect_detail)
    } else {
        crate::CardOp::failed(
            format!("{} — reconnect failed", spec.degrade_subject),
            error.to_string(),
            spec.failed_exit_label,
        )
    }
}

struct ManagementFlowSpec {
    request: LinkManagementRequest,
    /// Activity label while the management operation runs.
    progress_label: &'static str,
    /// Activity detail while waiting for the post-operation reconnect.
    reconnect_detail: &'static str,
    /// The Failed card-op's single exit label (model §2 I4) — the door to
    /// the nearest stable state, e.g. "Back to set up".
    failed_exit_label: &'static str,
    /// Reset records the live-captured logs on success (its result replay
    /// is empty); flash/erase rely on the result replay alone, so recording
    /// the capture too would double every line.
    record_captured_logs_on_success: bool,
    /// Success notice derived from the management result.
    done_notice: fn(&LinkManagementResult) -> UiNotice,
    /// Log-line subject when the reconnect half degrades, e.g. "device
    /// reset" → "device reset but serial reopen failed: …".
    degrade_subject: &'static str,
    server_reconnect_failed_notice: &'static str,
    /// The device will NOT come back on its own — the user has to unplug
    /// and replug it — so a failed reattach is the EXPECTED ending, not a
    /// failure.
    ///
    /// True for a flash performed while the chip sits in ROM download mode:
    /// it does not boot the freshly written image without a power cycle.
    /// Without this the flow flashes successfully, waits for a device that
    /// cannot return, and then reports the success as a failure — which is
    /// exactly the path a user in recovery takes.
    awaits_manual_replug: bool,
    /// The op takes the project WITH it (a destructive wipe): when the
    /// editor lens sits on the managed device, fully quiesce it — detach
    /// the lens so the app returns to the gallery — and say so, rather than
    /// leaving a severed-but-still-open editor. A reset/flash keeps the
    /// project on the device, so its lens only resets its live edit-state
    /// and stays put to reload after the reattach (device-lifecycle P3).
    severs_lens: bool,
    /// Where the raw management result lands for flows whose outcome is more
    /// than a notice — the filesystem backup needs the image bytes. `None`
    /// everywhere else, and the result is MOVED in (it can be a megabyte).
    result_sink: Option<Rc<RefCell<Option<LinkManagementResult>>>>,
}

fn provision_notice(result: &LinkManagementResult) -> UiNotice {
    match result {
        LinkManagementResult::FlashFirmware(result) => {
            UiNotice::info(format!("Flashed {}", result.manifest.display_name))
        }
        _ => UiNotice::info("Firmware flashed"),
    }
}

fn boot_safe_once_notice(result: &LinkManagementResult) -> UiNotice {
    let label = match result {
        LinkManagementResult::SetBootControl(result) => {
            result.chip_name.as_deref().unwrap_or("This device")
        }
        _ => "This device",
    };
    UiNotice::info(format!(
        "{label} will start once in safe mode — dim, or with nothing loaded \
         on older firmware. The restart after that is normal."
    ))
}

fn reset_notice(result: &LinkManagementResult) -> UiNotice {
    match result {
        LinkManagementResult::EraseDeviceFlash(result) => {
            let label = result.chip_name.as_deref().unwrap_or("selected ESP32");
            UiNotice::info(format!("{label} wiped"))
        }
        _ => UiNotice::info("Device wiped"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    use std::cell::RefCell;
    use std::rc::Rc;

    use lpa_client::ClientIo;
    use lpa_link::providers::LinkProviderRegistry;
    use lpa_link::providers::fake::FakeProvider;
    use lpa_link::{LinkEndpoint, LinkEndpointId, LinkProviderKind};
    use lpc_model::{
        LpType, LpValue, NodeId, ProductKind, ProductRef, Revision, SlotData, SlotFieldShape,
        SlotMeta, SlotRecord, SlotShape, SlotShapeId, TreePath, VisualProduct, WithRevision,
    };
    use lpc_view::{ProjectView, TreeEntryView};
    use lpc_wire::{
        ClientMessage, ClientRequest, MemoryStats, NodeRuntimeStatus, ProjectReadEvent,
        ProjectReadQueryEvent, ProjectRuntimeStatus, RuntimeReadResult, ServerRuntimeStatus,
        TransportError, WireEntryState, WireServerMessage, WireServerMsgBody,
    };

    use super::*;
    use crate::{
        ConnectFlowState, ControllerId, ProjectController, ProjectEditorOp, ProjectEditorTarget,
        ProjectInventorySummary, ProjectNodeAddress, ProjectNodeTarget, ProjectState,
        ProjectSyncPhase, ServerState, StudioServerClient,
    };

    /// A card-owned op flow rides across a same-ENDPOINT session replace
    /// (M4 P2).
    ///
    /// The flow is keyed by session, and the replug that ENDS a recovery
    /// write brings the board back as a NEW session on the same endpoint.
    /// Without the migration the "unplug the board and plug it back in"
    /// instruction would vanish at the exact moment the user obeyed it —
    /// the shape of the 2026-07-31 bench regression, which the pre-M4
    /// uid-less rule survived only by accident.
    #[test]
    fn a_card_op_flow_follows_its_board_across_a_replug() {
        let mut studio = StudioController::new(|| 1.0);
        let first = studio
            .pool
            .install(RuntimePayload::stub_device_for_test(
                lpa_link::DeviceState::Bootloader,
            ))
            .unwrap_or_else(|_| panic!("first attach"));
        studio.device_card_ops.insert(
            first,
            Rc::new(RefCell::new(crate::CardOp::awaiting("Replug the board"))),
        );

        // The replug: the board comes back on the same endpoint, so the
        // pool replaces `first` with a session of its own.
        let second = studio
            .pool
            .install(RuntimePayload::stub_device_for_test(
                lpa_link::DeviceState::Booting,
            ))
            .unwrap_or_else(|_| panic!("re-attach"));
        assert_ne!(second, first, "a replug mints a new session");
        studio.migrate_card_op(Some(first), second);

        assert!(
            !studio.device_card_ops.contains_key(&first),
            "the dead session keeps nothing"
        );
        assert_eq!(
            *studio.device_card_ops[&second].borrow(),
            crate::CardOp::awaiting("Replug the board"),
            "the instruction survives onto the board that came back"
        );

        // A replace that is NOT a same-endpoint one (no outgoing session)
        // leaves the newcomer's card clean — a fresh board must not
        // inherit somebody else's instruction.
        studio.device_card_ops.clear();
        studio.migrate_card_op(None, second);
        assert!(studio.device_card_ops.is_empty());
    }

    #[test]
    fn streamed_logs_publish_on_a_throttle_not_per_batch() {
        use std::cell::Cell;

        fn draft(message: &str) -> UiLogDraft {
            UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Studio, message)
        }

        let clock = Rc::new(Cell::new(0.0_f64));
        let mut studio = {
            let clock = Rc::clone(&clock);
            StudioController::new(move || clock.get())
        };

        // Prime: the first gate always emits (starts dirty).
        assert!(studio.view_if_changed().is_some());
        assert!(studio.view_if_changed().is_none());

        // The first streamed batch publishes at once.
        studio.record_logs(vec![draft("one")]);
        assert!(studio.view_if_changed().is_some());

        // A stream arriving faster than the throttle gates out — the lines
        // wait in the ring instead of forcing a rebuild per batch.
        studio.record_logs(vec![draft("two")]);
        assert!(studio.view_if_changed().is_none());

        // Once the gap elapses the pending lines publish.
        clock.set(LOG_ONLY_PUBLISH_MIN_GAP_SECS + 0.01);
        assert!(studio.view_if_changed().is_some());

        // A local mutation (action-outcome log) publishes immediately and
        // carries any throttled stream lines with it.
        studio.record_logs(vec![draft("three")]);
        studio.push_log(draft("outcome"));
        assert!(studio.view_if_changed().is_some());
    }

    #[test]
    fn sim_crash_reboot_guard_allows_first_and_expired_never_within_window() {
        // Never rebooted: always allowed.
        assert!(sim_crash_reboot_allowed(None, 100.0, 30.0));
        // Inside the window (including the same instant): suppressed.
        assert!(!sim_crash_reboot_allowed(Some(100.0), 100.0, 30.0));
        assert!(!sim_crash_reboot_allowed(Some(100.0), 129.9, 30.0));
        // Window elapsed: allowed again.
        assert!(sim_crash_reboot_allowed(Some(100.0), 130.0, 30.0));
    }

    #[test]
    fn an_upgraded_import_says_so_and_an_ordinary_one_stays_quiet() {
        use crate::app::library::{CatalogOutcome, PackageHealth, PackageSummary};

        let summary = PackageSummary {
            uid: "prj0123456789abcdef".parse().unwrap(),
            name: "Plasma".to_string(),
            kind: "Project".to_string(),
            slug: "2026-08-04-1800-plasma".to_string(),
            health: PackageHealth::Ready,
        };

        // The whole point of the import gate reaching the user: the bytes
        // they handed over are not the bytes that landed.
        let upgraded = CatalogOutcome {
            summary: Some(summary.clone()),
            upgraded_from: Some(4),
        };
        let message = import_message("Imported", &upgraded);
        assert!(message.contains("Plasma"), "{message}");
        assert!(message.contains("upgraded from format 4"), "{message}");
        assert!(
            message.contains(&lpc_model::PROJECT_FORMAT_VERSION.to_string()),
            "{message}"
        );

        // ...and the ordinary case is not made noisy by it.
        let plain = CatalogOutcome {
            summary: Some(summary),
            upgraded_from: None,
        };
        assert_eq!(import_message("Pasted", &plain), "Pasted Plasma");
    }

    #[test]
    fn initial_snapshot_selects_provider() {
        let studio = StudioController::new(|| 0.0);

        assert!(matches!(
            studio.snapshot().flow,
            ConnectFlowState::SelectingProvider { .. }
        ));
    }

    #[test]
    fn settings_commands_layer_persist_and_reach_the_view() {
        use crate::{SettingsCommand, SettingsLayer, StudioSettings};

        let mut studio = StudioController::new(|| 0.0);
        let persisted: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        studio.set_on_user_settings({
            let persisted = Rc::clone(&persisted);
            move |json| persisted.borrow_mut().push(json.to_string())
        });

        // Boot: the stored user layer loads without firing the hook.
        studio.load_user_settings_json(r#"{"agent":{"model":"user-model"}}"#);
        assert!(persisted.borrow().is_empty());

        // The host layer arrives; user override still wins for the model,
        // the host supplies the key.
        studio.apply_settings_command(SettingsCommand::HostLayerLoaded(
            StudioSettings::from_json_str(
                r#"{"agent":{"anthropic_api_key":"sk-ant-host-key","model":"host-model"}}"#,
            )
            .unwrap(),
        ));
        assert_eq!(studio.settings().agent_model(), Some("user-model"));
        assert_eq!(
            studio.settings().agent_anthropic_api_key(),
            Some("sk-ant-host-key")
        );
        assert!(persisted.borrow().is_empty());

        // A user mutation persists the user layer (only user fields).
        studio.apply_settings_command(SettingsCommand::SetAgentAnthropicApiKey(Some(
            "sk-ant-user-key".to_string(),
        )));
        assert_eq!(persisted.borrow().len(), 1);
        assert!(persisted.borrow()[0].contains("sk-ant-user-key"));
        assert!(!persisted.borrow()[0].contains("host"));

        // The view carries the settings slice: provenance + masked key.
        let view = studio.view();
        assert_eq!(view.settings.agent.api_key_layer, SettingsLayer::User);
        assert_eq!(view.settings.agent.model_layer, SettingsLayer::User);
        let masked = view.settings.agent.api_key_masked.clone().unwrap();
        assert!(
            !masked.contains("sk-ant-user"),
            "unmasked key in view: {masked}"
        );

        // Clearing the override falls back to the host layer and persists.
        studio.apply_settings_command(SettingsCommand::SetAgentModel(None));
        assert_eq!(studio.settings().agent_model(), Some("host-model"));
        assert_eq!(persisted.borrow().len(), 2);
    }

    #[test]
    fn credential_changes_spawn_one_model_fetch_and_results_reach_the_view() {
        use crate::app::studio::studio_actor::poll_now;
        use crate::app::studio::studio_view_channel::command_channel;
        use crate::{AgentTaskFuture, SettingsCommand, StudioCommand};
        use lpa_agent::ModelInfo;

        let mut studio = StudioController::new(|| 7.0);
        let tasks: Rc<RefCell<Vec<AgentTaskFuture>>> = Rc::new(RefCell::new(Vec::new()));
        studio.set_agent_spawner({
            let tasks = Rc::clone(&tasks);
            move |future| tasks.borrow_mut().push(future)
        });
        studio.set_agent_models_fetcher(|_config| {
            Box::pin(async {
                Ok(vec![ModelInfo {
                    display_name: Some("Claude Sonnet 5".to_string()),
                    ..ModelInfo::new("claude-sonnet-5".to_string())
                }])
            })
        });
        let (tx, rx) = command_channel();
        studio.set_agent_command_sender(tx);

        // A key change triggers exactly one spawned fetch; the view flags
        // the load.
        studio.apply_settings_command(SettingsCommand::SetAgentAnthropicApiKey(Some(
            "sk-a".to_string(),
        )));
        assert_eq!(tasks.borrow().len(), 1);
        assert!(studio.view().settings.agent.models_loading);
        // A settings-surface open debounces against the in-flight fetch.
        studio.apply_settings_command(SettingsCommand::RequestModels { force: false });
        assert_eq!(tasks.borrow().len(), 1);

        // Drive the fetch: it reports ModelsLoaded through the command
        // queue; applying it lands the options with the clock's stamp.
        let task = tasks.borrow_mut().remove(0);
        poll_now(task).expect("scripted fetch resolves in one poll");
        let mut loaded = 0;
        for command in rx.try_recv_all_for_test() {
            let StudioCommand::Settings(command) = command else {
                panic!("unexpected command class");
            };
            assert!(matches!(command, SettingsCommand::ModelsLoaded { .. }));
            studio.apply_settings_command(command);
            loaded += 1;
        }
        assert_eq!(loaded, 1);
        let agent = studio.view().settings.agent;
        assert!(!agent.models_loading);
        assert_eq!(agent.model_options.len(), 1);
        assert_eq!(agent.model_options[0].id, "claude-sonnet-5");

        // Repeat opens stay debounced; a credential change refetches.
        studio.apply_settings_command(SettingsCommand::RequestModels { force: false });
        assert!(tasks.borrow().is_empty());
        studio.apply_settings_command(SettingsCommand::SetAgentAnthropicApiKey(Some(
            "sk-b".to_string(),
        )));
        assert_eq!(tasks.borrow().len(), 1);
    }

    #[test]
    fn model_requests_without_platform_seams_leave_no_loading_marker() {
        use crate::{AgentProvider, SettingsCommand};

        // No fetcher/spawner/sender installed (host tests, story builds):
        // the request must not wedge the view in a perpetual load.
        let mut studio = StudioController::new(|| 0.0);
        studio.apply_settings_command(SettingsCommand::SetAgentAnthropicApiKey(Some(
            "sk".to_string(),
        )));
        studio.apply_settings_command(SettingsCommand::RequestModels { force: true });
        let agent = studio.view().settings.agent;
        assert!(!agent.models_loading);
        assert!(agent.model_options.is_empty());
        assert!(
            studio
                .settings()
                .agent_models(AgentProvider::Anthropic)
                .is_none()
        );
    }

    #[test]
    fn agent_view_context_carries_the_model_slice() {
        use crate::app::studio::studio_actor::poll_now;
        use crate::app::studio::studio_view_channel::command_channel;
        use crate::{AgentTaskFuture, SettingsCommand, StudioCommand};
        use lpa_agent::ModelInfo;

        let mut studio = StudioController::new(|| 0.0);
        let tasks: Rc<RefCell<Vec<AgentTaskFuture>>> = Rc::new(RefCell::new(Vec::new()));
        studio.set_agent_spawner({
            let tasks = Rc::clone(&tasks);
            move |future| tasks.borrow_mut().push(future)
        });
        studio.set_agent_models_fetcher(|_config| {
            Box::pin(async {
                Ok(vec![ModelInfo {
                    display_name: Some("Claude Haiku 4".to_string()),
                    ..ModelInfo::new("claude-haiku-4".to_string())
                }])
            })
        });
        let (tx, rx) = command_channel();
        studio.set_agent_command_sender(tx);

        studio.apply_settings_command(SettingsCommand::SetAgentModel(Some(
            "claude-sonnet-5".to_string(),
        )));
        studio.apply_settings_command(SettingsCommand::SetAgentAnthropicApiKey(Some(
            "sk-a".to_string(),
        )));
        assert!(studio.agent_view_context().model.loading);
        let task = tasks.borrow_mut().remove(0);
        poll_now(task).expect("scripted fetch resolves in one poll");
        for command in rx.try_recv_all_for_test() {
            let StudioCommand::Settings(command) = command else {
                panic!("unexpected command class");
            };
            studio.apply_settings_command(command);
        }

        let model = studio.agent_view_context().model;
        assert_eq!(model.effective.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(model.options.len(), 1);
        assert_eq!(model.options[0].id, "claude-haiku-4");
        assert!(!model.loading);
    }

    #[test]
    fn push_log_stamps_drafts_with_the_injected_clock() {
        use std::cell::Cell;

        // A stepping fake clock: each read advances one second.
        let ticks = Rc::new(Cell::new(0_u32));
        let mut studio = StudioController::new({
            let ticks = Rc::clone(&ticks);
            move || {
                ticks.set(ticks.get() + 1);
                100.0 + f64::from(ticks.get())
            }
        });

        studio.push_log(UiLogDraft::new(
            UiLogLevel::Info,
            UiLogOrigin::Studio,
            "first",
        ));
        studio.push_log(UiLogDraft::new(
            UiLogLevel::Warn,
            crate::UiLogSource::with_detail(UiLogOrigin::Link, "browser-serial"),
            "second",
        ));

        let logs = studio.logs();
        assert_eq!(logs[0].timestamp, 101.0);
        assert_eq!(logs[1].timestamp, 102.0);
        assert_eq!(logs[1].source.detail.as_deref(), Some("browser-serial"));
    }

    #[test]
    fn console_commands_reshape_the_emitted_console_view() {
        let mut studio = StudioController::new(|| 7.5);
        studio.push_log(UiLogDraft::new(
            UiLogLevel::Debug,
            UiLogOrigin::Server,
            "heartbeat frame=1",
        ));
        studio.push_log(UiLogDraft::new(
            UiLogLevel::Info,
            UiLogOrigin::Studio,
            "connected",
        ));

        // Default filter: Info+ shows only the studio line; the debug
        // heartbeat is counted, not dropped.
        let console = studio.view().console;
        assert_eq!(console.entries.len(), 1);
        assert_eq!(console.hidden_count, 1);

        // Lowering the threshold reveals the retained history.
        studio.apply_console_command(ConsoleCommand::SetMinLevel(UiLogLevel::Trace));
        let console = studio.view().console;
        assert_eq!(console.entries.len(), 2);
        assert_eq!(console.hidden_count, 0);
        assert_eq!(console.min_level, UiLogLevel::Trace);

        // Disabling an origin hides its entries.
        studio.apply_console_command(ConsoleCommand::SetOriginEnabled(UiLogOrigin::Server, false));
        let console = studio.view().console;
        assert_eq!(console.entries.len(), 1);
        assert_eq!(console.hidden_count, 1);

        // Clear empties the ring itself.
        studio.apply_console_command(ConsoleCommand::Clear);
        assert!(studio.logs().is_empty());
        let console = studio.view().console;
        assert!(console.entries.is_empty());
        assert_eq!(console.hidden_count, 0);
    }

    #[test]
    fn on_entry_hook_sees_every_ring_entry_once_regardless_of_filter() {
        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let mut studio = StudioController::new(|| 42.0);
        studio.set_on_entry({
            let seen = Rc::clone(&seen);
            move |entry| seen.borrow_mut().push(entry.message.clone())
        });
        // Hide everything from the *display*: the hook must still fire.
        studio.apply_console_command(ConsoleCommand::SetMinLevel(UiLogLevel::Error));
        studio.apply_console_command(ConsoleCommand::SetOriginEnabled(UiLogOrigin::Link, false));

        studio.push_log(UiLogDraft::new(UiLogLevel::Debug, UiLogOrigin::Link, "one"));
        studio.record_logs(vec![
            UiLogDraft::new(UiLogLevel::Info, UiLogOrigin::Studio, "two"),
            UiLogDraft::new(UiLogLevel::Warn, UiLogOrigin::Server, "three"),
        ]);

        assert!(
            studio.view().console.entries.is_empty(),
            "the display filter hides all three entries"
        );
        assert_eq!(
            *seen.borrow(),
            vec!["one".to_string(), "two".to_string(), "three".to_string()],
            "the hook fires exactly once per entry, in ring order"
        );
    }

    /// The reflash escalation, pinned where it renders now: the LENS
    /// CARD. The retired step-stack pane carried a status label and an
    /// Issue-bodied Device section for this; the card's state derivation
    /// and its Danger tab carry it since D43.
    #[test]
    fn incompatible_device_surfaces_the_reflash_state_on_the_card() {
        use lpa_link::{DeviceState, IncompatibleReason};

        let mut studio = connected_studio();
        studio.set_stub_device_for_test(DeviceState::Incompatible {
            reason: IncompatibleReason::NoHello,
        });

        let card = studio
            .view()
            .lens_card
            .expect("the editor's device surface is the lens card");
        assert_eq!(card.state, crate::RosterCardState::NeedsFirmwareUpdate);
        // The ONE affordance: reflash (explicit, never automatic).
        assert_eq!(
            card.state.affordance(),
            Some(crate::RosterAffordance::UpdateFirmware),
        );
    }

    #[test]
    fn initial_actions_target_device_node() {
        let studio = StudioController::new(|| 0.0);

        let actions = studio.actions();

        assert!(
            actions
                .iter()
                .all(|action| action.node_id().as_str() == DeviceController::NODE_ID)
        );
    }

    #[test]
    fn detached_sim_sessions_join_the_slow_heartbeat_lane() {
        use crate::app::studio::refresh_cadence::{
            DEVICE_HEARTBEAT_INTERVAL, SIMULATOR_REFRESH_INTERVAL,
        };

        let mut studio = StudioController::new(|| 100.0);
        studio.set_stub_sim_for_test();

        // Lens on the sim: a never-pulled lens is immediately due
        // (completion-based pacing); once a pull completes, the fast sim
        // gap counts down from the completion stamp.
        assert_eq!(studio.next_refresh_interval(), Duration::ZERO);
        studio.note_passive_refresh_completed();
        assert_eq!(studio.next_refresh_interval(), SIMULATOR_REFRESH_INTERVAL);

        // Detached (P3): the sim leaves the lens lane and joins the slow
        // heartbeat lane, so its buffered wire logs keep draining while no
        // project pull touches its client. A never-heartbeated session is
        // immediately due; a fresh heartbeat re-arms the full interval.
        studio.detach_lens().expect("detach succeeds");
        assert_eq!(studio.next_refresh_interval(), Duration::ZERO);
        studio.run_due_heartbeats();
        assert_eq!(studio.next_refresh_interval(), DEVICE_HEARTBEAT_INTERVAL);
    }

    #[test]
    fn a_sim_lens_with_a_board_feeds_lens_board_id_and_its_card() {
        // Gallery-rework P04 / vision D4. The sim is still not a device
        // (D22) — no registry row backs it — but a board identity makes
        // the output face's pin diagram light up for it exactly as it does
        // for hardware, and the card says what it is pretending to be.
        let mut studio = StudioController::new(|| 100.0);
        studio.set_stub_sim_for_test();

        assert_eq!(
            studio.lens_board_id(),
            None,
            "default: no board, today's behavior"
        );
        assert_eq!(
            studio
                .lens_device_card()
                .expect("a lens card for the sim session")
                .board_id,
            None
        );

        studio
            .pool
            .lens_session_mut()
            .expect("the sim holds the lens")
            .set_sim_board_id(Some("seeed/xiao-esp32-c6".to_string()));

        assert_eq!(studio.lens_board_id(), Some("seeed/xiao-esp32-c6"));
        assert_eq!(
            studio
                .lens_device_card()
                .expect("a lens card for the sim session")
                .board_id
                .as_deref(),
            Some("seeed/xiao-esp32-c6"),
            "the card carries it too — that is the \"as <board>\" line"
        );
    }

    #[test]
    fn a_device_session_never_takes_the_sims_board_field() {
        // The two sources stay separate: a device's board is its registry
        // row, and `set_sim_board_id` is a no-op on a device session.
        let mut studio = StudioController::new(|| 100.0);
        studio.set_stub_device_for_test(lpa_link::DeviceState::Booting);
        let session = studio
            .pool
            .lens_session_mut()
            .expect("the device holds the lens");
        session.set_sim_board_id(Some("seeed/xiao-esp32-c6".to_string()));
        assert_eq!(session.sim_board_id(), None);
    }

    #[test]
    fn initial_view_shows_the_home_gallery() {
        let studio = StudioController::new(|| 0.0);

        let view = studio.view();

        let home = view.home.expect("an idle studio shows home");
        assert!(view.panes.is_empty(), "home replaces the pane layout");
        assert!(!home.library_available, "no store attached on host");
        assert!(!home.examples.is_empty(), "examples always show");
    }

    #[test]
    fn home_ops_rename_duplicate_import_and_delete_library_packages() {
        use crate::app::library::{
            LibraryStore, MemoryLibraryHost, PackageProvenance, export_package,
        };
        use crate::{HOME_NODE_ID, HomeOp, ZipBytes};
        use lpfs::LpFsMemory;

        let mut studio = StudioController::new(|| 42.0);
        let counter = Rc::new(RefCell::new(0u8));
        let store = LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || {
                *counter.borrow_mut() += 1;
                [*counter.borrow(); 16]
            }),
            Rc::new(|| "2026-07-09-1421".to_string()),
        );
        studio.attach_library(Rc::new(MemoryLibraryHost::new(
            store.clone(),
            Rc::new(|| 42.0),
        )));
        let home_action = |op: HomeOp| UiAction::from_op(ControllerId::new(HOME_NODE_ID), op);

        // seed one package directly (the gallery's own create op has its
        // own test below)
        let seeded = store
            .install_package("Seeded", &[], PackageProvenance::Created, 42.0)
            .unwrap();
        // the gallery is cache+invalidate now: hydrate the pending refresh
        // (the actor's settle point, driven by hand in controller tests)
        studio.request_library_refresh();
        block_on_ready(studio.settle_library());
        let home = studio.view().home.expect("home with library");
        assert!(home.library_available);
        assert_eq!(home.projects.len(), 1);
        let uid = seeded.uid.to_string();

        // rename (slug move), then duplicate the renamed package
        block_on_ready(studio.dispatch(home_action(HomeOp::RenamePackage {
            uid: uid.clone(),
            name: "Porch".to_string(),
        })))
        .unwrap();
        block_on_ready(studio.dispatch(home_action(HomeOp::DuplicatePackage { uid: uid.clone() })))
            .unwrap();
        let home = studio.view().home.unwrap();
        let copy = home
            .projects
            .iter()
            .find(|card| card.slug == "2026-07-09-1421-porch")
            .expect("duplicate landed (re-stamped from the renamed slug)");
        assert_eq!(copy.provenance.as_deref(), Some("Forked from porch"));

        // export the copy's bytes, delete it, and import it back
        let zip = {
            let handle = store.open(copy.uid.parse().unwrap()).unwrap();
            export_package(&handle).unwrap()
        };
        let copy_uid = copy.uid.clone();
        block_on_ready(studio.dispatch(home_action(HomeOp::DeletePackage {
            uid: copy.uid.clone(),
        })))
        .unwrap();
        assert!(
            !studio
                .view()
                .home
                .unwrap()
                .projects
                .iter()
                .any(|card| card.uid == copy_uid)
        );
        block_on_ready(studio.dispatch(home_action(HomeOp::ImportZip {
            file_name: "porch-copy.zip".to_string(),
            bytes: ZipBytes(zip),
        })))
        .unwrap();
        let home = studio.view().home.unwrap();
        let imported = home
            .projects
            .iter()
            .find(|card| card.provenance.as_deref() == Some("Imported from zip"))
            .expect("import landed");
        // re-stamped from the imported manifest's label; the deleted copy
        // freed the plain stamp+label slot
        assert_eq!(imported.slug, "2026-07-09-1421-porch");
    }

    #[test]
    fn home_create_project_mints_blank_packages_with_deduped_slugs() {
        use crate::app::library::{LibraryStore, MemoryLibraryHost};
        use crate::{HOME_NODE_ID, HomeOp};
        use lpc_history::EventKind;
        use lpfs::LpFsMemory;

        let mut studio = StudioController::new(|| 42.0);
        let counter = Rc::new(RefCell::new(0u8));
        let store = LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || {
                *counter.borrow_mut() += 1;
                [*counter.borrow(); 16]
            }),
            Rc::new(|| "2026-07-27-0900".to_string()),
        );
        studio.attach_library(Rc::new(MemoryLibraryHost::new(
            store.clone(),
            Rc::new(|| 42.0),
        )));
        let home_action = |op: HomeOp| UiAction::from_op(ControllerId::new(HOME_NODE_ID), op);

        // Two creates in a row. Creation lands FIRST; the follow-on open
        // then refuses on host (this build has no browser-worker sim), so
        // the dispatch errs — the successful create-and-open round-trip is
        // the edit-e2e test's. The packages stick either way.
        for _ in 0..2 {
            block_on_ready(studio.dispatch(home_action(HomeOp::CreateProject {
                template: crate::ProjectTemplate::Blank,
            })))
            .expect_err("host test builds have no sim runtime to open into");
        }
        studio.request_library_refresh();
        block_on_ready(studio.settle_library());

        let home = studio.view().home.expect("home with library");
        assert_eq!(home.projects.len(), 2, "both creates landed");
        let slug_of = |slug: &str| {
            home.projects
                .iter()
                .find(|card| card.slug == slug)
                .unwrap_or_else(|| panic!("{slug} listed, got {:?}", home.projects))
        };
        // dated + slugified from the default "Project" label; the second
        // create dedups with the `-2` suffix and mints its own uid
        let first = slug_of("2026-07-27-0900-project");
        let second = slug_of("2026-07-27-0900-project-2");
        assert_ne!(first.uid, second.uid, "each create mints a fresh uid");
        assert!(
            first.provenance.is_none() && second.provenance.is_none(),
            "Created packages carry no provenance line"
        );

        // history: the Created origin plus the initial-save snapshot
        let handle = store
            .open(first.uid.parse().expect("card carries the minted uid"))
            .expect("created package opens");
        assert_eq!(handle.history.events()[0].kind, EventKind::Created);
        assert!(
            handle
                .history
                .events()
                .iter()
                .any(|event| matches!(event.kind, EventKind::Saved { .. })),
            "the initial save snapshot is recorded"
        );
    }

    #[test]
    fn open_elsewhere_projects_refuse_kindly_and_badge_their_cards() {
        use crate::app::library::{LibraryStore, MemoryLibraryHost, PackageProvenance};
        use crate::{HOME_NODE_ID, HomeOp};
        use lpfs::LpFsMemory;

        let mut studio = StudioController::new(|| 42.0);
        let store = LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(|| [9u8; 16]),
            Rc::new(|| "2026-07-09-1421".to_string()),
        );
        let held = store
            .install_package("Held", &[], PackageProvenance::Created, 42.0)
            .unwrap();
        let host = Rc::new(MemoryLibraryHost::new(store, Rc::new(|| 42.0)));
        host.set_open_elsewhere(vec![held.uid.to_string()]);
        studio.attach_library(host.clone());
        let home_action = |op: HomeOp| UiAction::from_op(ControllerId::new(HOME_NODE_ID), op);

        // structural ops refuse with the friendly multi-tab message
        let error = block_on_ready(studio.dispatch(home_action(HomeOp::DeletePackage {
            uid: held.uid.to_string(),
        })))
        .expect_err("delete of an open-elsewhere project refuses");
        assert!(
            error.to_string().contains("open in another tab"),
            "friendly refusal, got: {error}"
        );
        // by the second dispatch the gallery inputs are hydrated, so the
        // refusal names the project (P4 copy)
        let error = block_on_ready(studio.dispatch(home_action(HomeOp::RenamePackage {
            uid: held.uid.to_string(),
            name: "stolen".to_string(),
        })))
        .expect_err("rename of an open-elsewhere project refuses");
        assert!(
            error
                .to_string()
                .contains("2026-07-09-1421-held is open in another tab"),
            "named refusal, got: {error}"
        );

        // the gallery data carries the badge (the failed dispatches still
        // settled the pending hydration from attach)
        let home = studio.view().home.expect("home with library");
        assert_eq!(home.projects.len(), 1, "the held project still lists");
        assert!(home.projects[0].open_elsewhere, "card carries the badge");
    }

    #[test]
    fn connected_without_project_shows_gallery_not_panes() {
        let mut studio = connected_studio();
        studio.project.reset();

        let view = studio.view();
        assert!(view.home.is_some(), "no project open means gallery (D24)");
        assert!(view.panes.is_empty());
        // the gallery's actions are home ops; the wizard's project steps
        // are gone for good
        let actions = view_actions(&view);
        assert!(!actions.iter().any(|action| {
            matches!(
                action.op_as::<ProjectOp>(),
                Some(ProjectOp::ConnectRunningProject | ProjectOp::LoadDemoProject)
            )
        }));
    }

    #[test]
    fn connected_link_without_project_shows_the_gallery() {
        // gallery-always (D24): an engaged link with no open project is a
        // gallery state, never a pane takeover
        let studio = link_connected_studio();

        let view = studio.view();
        assert!(
            view.home.is_some(),
            "home renders whenever no project is open"
        );
        assert!(view.panes.is_empty());
    }

    #[test]
    fn no_firmware_marks_the_lens_card_ready_to_set_up() {
        // an open project whose device link answers without firmware:
        // the CARD escalates to the flash affordance
        let mut studio = connected_studio();
        studio.set_stub_device_for_test(lpa_link::DeviceState::BlankFlash);

        let card = studio.view().lens_card.expect("lens card");
        assert_eq!(card.state, crate::RosterCardState::ReadyToSetUp);
        assert_eq!(
            card.state.affordance(),
            Some(crate::RosterAffordance::SetUp),
        );
    }

    #[test]
    fn loaded_project_gets_the_project_pane_plus_the_lens_card() {
        let studio = connected_studio();

        let view = studio.view();

        // The editor is the Project pane ALONE, with the device surface
        // docked as the lens CARD. Two panes have been retired from this
        // column: the step-stack device pane, and — with P3 — the bus
        // pane, whose content now hangs off the module card that owns the
        // scope (controls on the panel, writers/readers in the wiring
        // drawer).
        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.panes[0].node_id.as_str(), ProjectController::NODE_ID);
        assert!(
            !view.panes.iter().any(|pane| pane.node_id.as_str() == "bus"),
            "no bus pane"
        );
        assert!(view.lens_card.is_some());
        assert!(
            !view
                .panes
                .iter()
                .any(|pane| pane.node_id.as_str() == DeviceController::NODE_ID),
            "no device pane"
        );

        // The wizard's project steps stayed gone through the deletion.
        let actions = view_actions(&view);
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::ConnectRunningProject | ProjectOp::LoadDemoProject)
        )));
        // …and so did its connect plumbing. Firmware/erase/disconnect are
        // no longer PANE actions: the card's Danger tab carries flash and
        // erase (`DeployOp::EraseDevice` runs the same `reset_to_blank`).
        assert!(
            actions
                .iter()
                .all(|action| action.op_as::<DeviceOp>().is_none()),
            "the editor's panes carry no device ops at all now"
        );
    }

    #[test]
    fn open_provider_for_recovery_skips_server_attach() {
        let mut studio =
            StudioController::with_link_registry_for_test(|| 0.0, registry_with_fake_endpoint());

        let outcome = block_on_ready(
            studio.open_provider_link_only(LinkProviderKind::Fake, UxUpdateSink::noop()),
        )
        .unwrap();

        assert!(
            outcome
                .notices
                .iter()
                .any(|notice| notice.message == "Choose the device endpoint to open for flashing")
        );
        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.snapshot().server.state,
            ServerState::Disconnected
        ));
        assert!(matches!(
            studio.snapshot().flow,
            ConnectFlowState::SelectingEndpoint { .. }
        ));
    }

    #[test]
    fn project_disconnect_leaves_server_and_link_connected() {
        let mut studio = connected_studio();

        block_on_ready(studio.disconnect_project()).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.snapshot().server.state,
            ServerState::Connected { .. }
        ));
        assert!(matches!(
            studio.snapshot().flow,
            ConnectFlowState::Connected { .. }
        ));
    }

    #[test]
    fn lightplayer_disconnect_leaves_device_link_connected() {
        let mut studio = connected_studio();

        let target = studio.device_target_for_test();
        block_on_ready(studio.disconnect_lightplayer(&target)).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.snapshot().server.state,
            ServerState::Disconnected
        ));
        assert!(matches!(
            studio.snapshot().flow,
            ConnectFlowState::Connected { .. }
        ));
        // no project → gallery, with the link still up underneath
        assert!(studio.view().home.is_some());
    }

    #[test]
    fn device_disconnect_clears_project_server_and_link() {
        let mut studio = connected_studio();

        let id = studio.the_device_for_test();
        block_on_ready(studio.disconnect_device(id)).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.snapshot().server.state,
            ServerState::Disconnected
        ));
        assert!(matches!(
            studio.snapshot().flow,
            ConnectFlowState::SelectingProvider { .. }
        ));
    }

    #[test]
    fn device_action_dispatch_routes_exact_device_target() {
        let mut studio = connected_studio();
        let action = UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::DisconnectDevice {
                target: studio.device_target_for_test(),
            },
        );

        block_on_ready(studio.dispatch(action)).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.snapshot().server.state,
            ServerState::Disconnected
        ));
        assert!(matches!(
            studio.snapshot().flow,
            ConnectFlowState::SelectingProvider { .. }
        ));
    }

    #[test]
    fn project_action_dispatch_routes_exact_project_target() {
        let mut studio = connected_studio();
        let action = UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            ProjectOp::DisconnectProject,
        );

        block_on_ready(studio.dispatch(action)).unwrap();

        assert!(matches!(
            studio.project.snapshot().state,
            ProjectState::NotLoaded
        ));
        assert!(matches!(
            studio.snapshot().server.state,
            ServerState::Connected { .. }
        ));
        assert!(matches!(
            studio.snapshot().flow,
            ConnectFlowState::Connected { .. }
        ));
    }

    #[test]
    fn set_device_log_level_sends_request_and_records_confirmation() {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let io = ScriptedClientIo::new(
            Rc::clone(&sent),
            vec![WireServerMessage::new(1, WireServerMsgBody::SetLogLevel)],
        );
        let mut studio = connected_studio_with_client(io);
        let action = UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::SetLogLevel {
                level: UiLogLevel::Debug,
            },
        );

        block_on_ready(studio.dispatch(action)).unwrap();

        {
            let sent = sent.borrow();
            assert_eq!(sent.len(), 1);
            let ClientRequest::SetLogLevel { level } = &sent[0].msg else {
                panic!("expected a SetLogLevel request, got {:?}", sent[0].msg);
            };
            assert_eq!(*level, lpc_wire::server::api::LogLevel::Debug);
        }

        assert!(
            studio.logs().iter().any(|entry| {
                entry.source.origin == UiLogOrigin::Server
                    && entry.message == "device log level set to debug"
            }),
            "success should record a Server-origin confirmation entry"
        );
        assert_eq!(
            studio.view().console.device_log_level,
            Some(UiLogLevel::Debug),
            "the console's device selector shows the requested level"
        );
    }

    #[test]
    fn device_log_level_is_absent_while_disconnected() {
        let studio = StudioController::new(|| 0.0);
        assert_eq!(studio.view().console.device_log_level, None);
    }

    #[test]
    fn refresh_project_dispatch_reads_project_and_updates_sync_summary() {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let io = ScriptedClientIo::new(
            Rc::clone(&sent),
            vec![project_read_response_with_runtime(1, Revision::new(13))],
        );
        let mut studio = connected_studio_with_client(io);
        let action = UiAction::from_op(
            ControllerId::new(ProjectController::NODE_ID),
            ProjectOp::RefreshProject,
        );

        let outcome = block_on_ready(studio.dispatch(action)).unwrap();

        assert!(
            outcome
                .notices
                .iter()
                .any(|notice| notice.message == "Project refreshed")
        );
        let sent = sent.borrow();
        assert_eq!(sent.len(), 1);
        let ClientRequest::ProjectRead { handle, request } = &sent[0].msg else {
            panic!("refresh should send a project read request");
        };
        assert_eq!(sent[0].id, 1);
        assert_eq!(handle.id(), 7);
        assert_eq!(request.since, None);
        assert_eq!(request.queries.len(), 4);

        let sync = studio
            .project
            .snapshot()
            .sync
            .expect("refresh should leave a sync summary");
        assert_eq!(sync.phase, ProjectSyncPhase::Ready);
        assert_eq!(sync.revision, 13);
        assert_eq!(
            sync.runtime.as_ref().map(|runtime| runtime.frame_num),
            Some(77)
        );
        assert_eq!(
            sync.runtime.as_ref().and_then(|runtime| runtime.free_bytes),
            Some(4096)
        );
    }

    #[test]
    fn project_descendant_action_dispatch_routes_to_project_ux() {
        let mut studio = StudioController::new(|| 0.0);
        let target = ProjectEditorTarget::node_tree();
        let action = UiAction::from_op(target.node_id(), ProjectEditorOp::Focus);

        block_on_ready(studio.dispatch(action)).unwrap();

        assert_eq!(studio.project.active_editor_target(), Some(&target));
    }

    #[test]
    fn project_node_focus_dispatch_requests_visual_product_preview() {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let io = ScriptedClientIo::new(
            Rc::clone(&sent),
            vec![project_read_response_with_runtime(1, Revision::new(13))],
        );
        let mut studio = connected_studio_with_client(io);
        studio
            .project
            .apply_project_view(&single_product_project_view(3))
            .unwrap();
        let product = VisualProduct::new(NodeId::new(3), 0);
        let target = ProjectEditorTarget::addressed_node(ProjectNodeTarget::new(
            ProjectNodeAddress::new(TreePath::parse("/demo.module/orbit.shader").unwrap()),
            NodeId::new(3),
        ));
        let action = UiAction::from_op(target.node_id(), ProjectEditorOp::Focus);

        block_on_ready(studio.dispatch(action)).unwrap();

        // Focus is local-only (P3): it updates the active editor target and the
        // focus-scoped probe set but does NOT send a project read. The changed
        // probe set is picked up by the next passive refresh tick.
        assert_eq!(sent.borrow().len(), 0, "Focus must not send a project read");
        assert_eq!(studio.project.active_editor_target(), Some(&target));
        // The now-focused node subscribes to its visual product, so the next
        // refresh request will carry the render probe.
        let _ = product;
    }

    #[test]
    fn unknown_top_level_dispatch_fails_clearly() {
        let mut studio = StudioController::new(|| 0.0);
        let action = UiAction::from_op(ControllerId::new("studio|unknown"), ProjectEditorOp::Focus);

        let result = block_on_ready(studio.dispatch(action));

        assert!(matches!(
            result,
            Err(UiError::UnsupportedAction(message))
                if message.contains("unknown UX node studio|unknown")
        ));
    }

    #[test]
    fn unknown_project_descendant_dispatch_fails_as_project_target() {
        let mut studio = StudioController::new(|| 0.0);
        let action = UiAction::from_op(
            ControllerId::new("studio|project|unknown"),
            ProjectEditorOp::Focus,
        );

        let result = block_on_ready(studio.dispatch(action));

        assert!(matches!(
            result,
            Err(UiError::UnsupportedAction(message))
                if message.contains("unknown project editor target studio|project|unknown")
        ));
    }

    #[test]
    fn project_descendant_dispatch_rejects_wrong_op_type() {
        let mut studio = StudioController::new(|| 0.0);
        let action = UiAction::from_op(
            ProjectEditorTarget::node_tree().node_id(),
            ProjectOp::LoadDemoProject,
        );

        let result = block_on_ready(studio.dispatch(action));

        assert!(matches!(
            result,
            Err(UiError::UnsupportedAction(message))
                if message.contains("ProjectEditorOp")
        ));
    }

    /// The M6 ladder ending, pinned at the surface that carries it now:
    /// the CARD. This used to assert an "Opening device session" activity
    /// landing in the retired step-stack pane's Device section — pane
    /// shape, not behaviour. The ladder contract is unchanged: soft
    /// ending, no toast, no issue chip, honest Not-responding card.
    #[test]
    fn failed_link_dispatch_ends_soft_on_the_card() {
        let mut studio = StudioController::with_link_registry_for_test(
            || 0.0,
            registry_with_fake_connect_error("Failed to open serial port."),
        );
        let action = UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::ConnectEndpoint {
                provider_id: LinkProviderKind::Fake,
                endpoint_id: LinkEndpointId::new("fake-runtime"),
            },
        );

        let result = drive(studio.dispatch_with_updates(action, UxUpdateSink::noop()));

        // M6: the ladder ends SOFT — no error, no toast, no issue chip.
        // The honest ending is the card's Not-responding state.
        let notices = result.expect("ladder endings are soft");
        assert!(notices.notices.is_empty(), "no toast from the ladder");
        assert!(matches!(
            studio.device_flow_state_for_test(),
            ConnectFlowState::Unresponsive { .. }
        ));
        let home = studio.view().home.expect("the gallery still shows");
        assert!(home.issue.is_none(), "no gallery issue chip either");
        assert!(
            home.devices
                .iter()
                .any(|card| card.state == crate::RosterCardState::NotResponding),
            "the ladder's honest ending is the Not-responding card: {:?}",
            home.devices
                .iter()
                .map(|card| card.state.clone())
                .collect::<Vec<_>>()
        );
    }

    // -----------------------------------------------------------------
    // The lens-card invariant (D43)
    //
    // The editor layout has exactly ONE device surface. Nothing pinned
    // that before, which is how a device-gone hole stayed invisible long
    // enough for the shell to keep falling back to the retired
    // step-stack pane — defect
    // docs/defects/2026-07-28-retired-device-pane-still-reachable.md.
    // -----------------------------------------------------------------

    /// Panes render ⇒ the editor is open ⇒ a lens card exists. Held
    /// across every state the lens can reach while the pane layout is up.
    #[test]
    fn panes_never_render_without_a_lens_card() {
        use lpa_link::DeviceState;

        fn assert_invariant(studio: &StudioController, what: &str) {
            let view = studio.view();
            if view.panes.is_empty() {
                return;
            }
            assert!(
                view.lens_card.is_some(),
                "{what}: panes render with no lens card — the editor's right column has no \
                 device surface"
            );
        }

        let mut studio = connected_studio();
        assert_invariant(&studio, "live device lens");

        for state in [
            DeviceState::Booting,
            DeviceState::BlankFlash,
            DeviceState::Bootloader,
            DeviceState::ForeignFirmware,
            DeviceState::Gone,
        ] {
            studio.set_stub_device_for_test(state.clone());
            assert_invariant(&studio, &format!("device lens, link {state:?}"));
        }

        let mut studio = connected_studio();
        studio.set_stub_sim_for_test();
        assert_invariant(&studio, "sim lens");
    }

    /// Unplugging mid-project FADES the card; it never removes it, and it
    /// never touches the editor. Yona 2026-07-28: a flaky cable must not
    /// yank anyone out of their work, so the lens and the project stay.
    #[test]
    fn an_unplugged_lens_fades_to_an_offline_card_and_keeps_the_editor() {
        use lpa_link::DeviceState;

        let mut studio = connected_studio();
        studio.set_stub_device_for_test(DeviceState::Gone);

        let view = studio.view();
        assert!(view.home.is_none(), "the editor stays open");
        assert!(!view.panes.is_empty(), "the pane layout stays up");
        let card = view.lens_card.expect("the unplugged lens keeps its card");
        assert!(
            matches!(card.state, crate::RosterCardState::Offline { .. }),
            "the card reads offline, not gone: {:?}",
            card.state
        );
        assert_eq!(
            card.state.affordance(),
            Some(crate::RosterAffordance::Reconnect),
            "the way back is on the card"
        );
        assert!(
            studio.pool.lens_session().is_some(),
            "the lens is untouched — the session is still the editor's"
        );
    }

    /// A DEVICE connect that ends WITHOUT a session leaves live sessions
    /// alone (multi-device M3): a failed attempt at an ADDITIONAL board
    /// must not tear down the board you are working on, so the lens, the
    /// editor, and the existing session all stay. (Pre-M3 this scenario
    /// cleared the kind's slot and quiesced back to the gallery — the
    /// "empty-slot ending" semantics that only make sense at capacity 1;
    /// the retired-pane hazard that teardown guarded against cannot arise
    /// here because nothing is removed. A reconnect of the SAME endpoint
    /// still replaces its own session at install time.)
    #[test]
    fn a_soft_connect_ending_leaves_live_sessions_and_the_editor_alone() {
        let mut studio = StudioController::with_link_registry_for_test(
            || 0.0,
            registry_with_fake_connect_error("Failed to open serial port."),
        );
        studio.set_stub_device_for_test(
            crate::app::runtime_pool::runtime_session::ready_state_for_test(),
        );
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        assert!(!studio.view().panes.is_empty(), "the editor is open");

        let action = UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::ConnectEndpoint {
                provider_id: LinkProviderKind::Fake,
                endpoint_id: LinkEndpointId::new("fake-runtime"),
            },
        );
        drive(studio.dispatch_with_updates(action, UxUpdateSink::noop())).expect("soft ending");

        let view = studio.view();
        assert!(
            studio.pool.lens_session().is_some(),
            "the live session and its lens survive the failed extra connect"
        );
        assert!(
            !view.panes.is_empty() && view.home.is_none(),
            "the editor stays open — a failed extra connect never yanks it"
        );
    }

    fn connected_studio() -> StudioController {
        let mut studio = link_connected_studio();
        studio.set_server_state_for_test(ServerState::Connected {
            protocol: "fake-protocol".to_string(),
        });
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        studio
    }

    fn connected_studio_with_client(io: ScriptedClientIo) -> StudioController {
        let mut studio = link_connected_studio();
        studio.set_server_client_for_test(StudioServerClient::from_io_for_test(
            "fake-protocol",
            Box::new(io),
        ));
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        studio
    }

    fn link_connected_studio() -> StudioController {
        let mut studio = StudioController::new(|| 0.0);
        // hardware, as far as the pane is concerned: a stubbed device
        // session in the Ready state
        studio.set_stub_device_for_test(
            crate::app::runtime_pool::runtime_session::ready_state_for_test(),
        );
        studio
    }

    fn single_product_project_view(node_id: u32) -> ProjectView {
        let revision = Revision::new(1);
        let path = TreePath::parse("/demo.module/orbit.shader").unwrap();
        let state_shape = SlotShapeId::new(700);
        let mut view = ProjectView::new();
        view.tree.insert(TreeEntryView::new(
            NodeId::new(node_id),
            path,
            None,
            None,
            NodeRuntimeStatus::Ok,
            WireEntryState::Alive,
            revision,
            revision,
            revision,
        ));
        view.slots
            .registry
            .register_dynamic_shape(
                state_shape,
                SlotShape::Record {
                    meta: SlotMeta::empty(),
                    fields: vec![
                        SlotFieldShape::new(
                            "output",
                            SlotShape::value(LpType::Product(ProductKind::Visual)),
                        )
                        .unwrap(),
                    ],
                },
            )
            .unwrap();
        view.slots
            .root_shapes
            .insert(format!("node.{node_id}.state"), state_shape);
        view.slots.roots.insert(
            format!("node.{node_id}.state"),
            SlotData::Record(SlotRecord::with_revision(
                revision,
                vec![SlotData::Value(WithRevision::new(
                    revision,
                    LpValue::Product(ProductRef::visual(VisualProduct::new(
                        NodeId::new(node_id),
                        0,
                    ))),
                ))],
            )),
        );
        view
    }

    fn registry_with_fake_connect_error(message: impl Into<String>) -> LinkProviderRegistry {
        let mut registry = LinkProviderRegistry::new();
        registry.insert(
            FakeProvider::new()
                .with_endpoint(LinkEndpoint::new(
                    "fake-runtime",
                    LinkProviderKind::Fake,
                    "Fake runtime",
                ))
                .with_connect_error(message),
        );
        registry
    }

    fn registry_with_fake_endpoint() -> LinkProviderRegistry {
        let mut registry = LinkProviderRegistry::new();
        registry.insert(FakeProvider::new().with_endpoint(LinkEndpoint::new(
            "fake-runtime",
            LinkProviderKind::Fake,
            "Fake runtime",
        )));
        registry
    }

    fn project_read_response_with_runtime(id: u64, revision: Revision) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::ProjectRead {
                events: vec![
                    ProjectReadEvent::Begin { revision },
                    ProjectReadEvent::Query {
                        index: 0,
                        event: ProjectReadQueryEvent::Runtime(RuntimeReadResult {
                            project: ProjectRuntimeStatus {
                                revision,
                                overlay_changed_at: Revision::default(),
                                frame_num: 77,
                                frame_delta_ms: 16,
                                frame_total_ms: 17,
                                demand_root_count: 2,
                                runtime_buffer_count: 3,
                            },
                            server: Some(ServerRuntimeStatus {
                                theoretical_fps: Some(60.0),
                                last_frame_time_us: Some(16_000),
                                memory: Some(MemoryStats {
                                    free_bytes: 4096,
                                    used_bytes: 2048,
                                    total_bytes: 6144,
                                    largest_free_block: None,
                                    oom_retry_saves: None,
                                }),
                                panel_auto_save: Some(true),
                            }),
                        }),
                    },
                    ProjectReadEvent::End { revision },
                ],
            },
        )
    }

    struct ScriptedClientIo {
        sent: Rc<RefCell<Vec<ClientMessage>>>,
        responses: Rc<RefCell<VecDeque<WireServerMessage>>>,
    }

    impl ScriptedClientIo {
        fn new(sent: Rc<RefCell<Vec<ClientMessage>>>, responses: Vec<WireServerMessage>) -> Self {
            Self {
                sent,
                responses: Rc::new(RefCell::new(responses.into())),
            }
        }
    }

    impl ClientIo for ScriptedClientIo {
        fn send<'life0, 'async_trait>(
            &'life0 mut self,
            msg: ClientMessage,
        ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            self.sent.borrow_mut().push(msg);
            Box::pin(async { Ok(()) })
        }

        fn receive<'life0, 'async_trait>(
            &'life0 mut self,
        ) -> Pin<Box<dyn Future<Output = Result<WireServerMessage, TransportError>> + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            let response =
                self.responses.borrow_mut().pop_front().ok_or_else(|| {
                    TransportError::Other("scripted client io exhausted".to_string())
                });
            Box::pin(async move { response })
        }

        fn close<'life0, 'async_trait>(
            &'life0 mut self,
        ) -> Pin<Box<dyn Future<Output = Result<(), TransportError>> + 'async_trait>>
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async { Ok(()) })
        }
    }

    fn block_on_ready<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("test future unexpectedly yielded"),
        }
    }

    /// Poll to completion for futures that legitimately yield (the M6
    /// ladder's poll-timer backoff between connect attempts).
    fn drive<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        for _ in 0..10_000 {
            if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
                return output;
            }
            // the poll timers run on the real clock (the ladder backoff
            // is 750 ms) — breathe instead of spinning
            std::thread::sleep(core::time::Duration::from_millis(1));
        }
        panic!("test future did not settle in 10k polls");
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    // ---- the setup wizard ------------------------------------------------

    fn setup_action(op: HomeOp) -> UiAction {
        UiAction::from_op(ControllerId::new(crate::HOME_NODE_ID), op)
    }

    fn wizard_of(studio: &StudioController) -> Option<crate::UiSetupWizard> {
        studio.view().home.and_then(|home| home.setup)
    }

    #[test]
    fn the_wizard_is_a_card_on_the_home_view_from_the_moment_it_opens() {
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        assert!(
            wizard_of(&studio).is_none(),
            "no wizard until one is asked for"
        );

        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();
        let wizard = wizard_of(&studio).expect("the wizard renders as a card");
        assert!(!wizard.sim);
        assert_eq!(wizard.title, "Connect a device");
        assert_eq!(wizard.state.kind(), crate::SetupStateKind::ConnectIntro);
        assert_eq!(
            wizard
                .steps
                .iter()
                .map(|step| step.label.as_str())
                .collect::<Vec<_>>(),
            ["Connect", "Flash", "Project", "Done"],
        );
    }

    #[test]
    fn the_simulator_entry_opens_on_the_board_pick_with_no_probe() {
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: true }))).unwrap();
        let wizard = wizard_of(&studio).expect("the sim wizard is a card too");
        assert!(wizard.sim);
        // Design §7.1: the sim's entry IS `BOARD_PICK`, entered with no
        // probe evidence — not a second state wearing a disguise.
        assert_eq!(wizard.state.kind(), crate::SetupStateKind::BoardPick);
        assert_eq!(wizard.state.probe(), None);
        assert_eq!(wizard.detected_chip(), None);
    }

    #[test]
    fn closing_the_wizard_takes_its_card_off_the_grid() {
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();
        block_on_ready(studio.dispatch(setup_action(HomeOp::Setup(
            crate::SetupGesture::CloseRequested,
        ))))
        .unwrap();
        assert!(
            wizard_of(&studio).is_none(),
            "a closed flow leaves no card behind"
        );
    }

    const KNOWN_UID: &str = "dev000000daqf6dvvqz";

    /// Install a stub device session — a board on the wire, no flow.
    fn install_stub_device(studio: &mut StudioController) -> crate::RuntimeId {
        studio
            .pool
            .install(
                crate::app::runtime_pool::RuntimePayload::stub_device_for_test(
                    crate::app::runtime_pool::runtime_session::ready_state_for_test(),
                ),
            )
            .unwrap_or_else(|refusal| panic!("stub device installs: {}", refusal.message))
    }

    /// The port-grant moment without a wire: a stub session, bound to the
    /// flow, with the machine walked to PROBING (where a real grant lands).
    fn grant_a_port(studio: &mut StudioController) -> crate::RuntimeId {
        let device_id = install_stub_device(studio);
        studio.bind_setup_device(device_id);
        let flow = &mut studio.setup.as_mut().expect("wizard").flow;
        flow.handle(crate::SetupEvent::ItsConnected);
        flow.handle(crate::SetupEvent::PortGranted);
        device_id
    }

    /// A probe verdict, as the executor would report it.
    fn blank_probe(hardware_uid: Option<&str>) -> crate::BoardProbe {
        crate::BoardProbe {
            verdict: crate::BoardVerdict::Blank { known: None },
            detected_chip: Some("esp32c6".to_string()),
            hardware_uid: hardware_uid.map(str::to_string),
            hardware_origin: hardware_uid.map(|_| "efuse:aa:bb:cc:dd:ee:ff".to_string()),
        }
    }

    /// Land a verdict on the open flow — the recognition moment the
    /// takeover binds at.
    fn land_a_verdict(studio: &mut StudioController, probe: crate::BoardProbe) {
        studio
            .setup
            .as_mut()
            .expect("wizard")
            .flow
            .handle(crate::SetupEvent::ProbeCompleted { probe });
    }

    /// Walk the pure machine from a landed BLANK verdict to DEVICE_HOME.
    /// The machine does not care where the events came from (R1).
    fn walk_the_flow_to_device_home(studio: &mut StudioController) {
        let flow = &mut studio.setup.as_mut().expect("wizard").flow;
        flow.handle(crate::SetupEvent::BoardChosen {
            board_id: "espressif/esp32-c6-devkitc-1".to_string(),
        });
        flow.handle(crate::SetupEvent::Confirm);
        flow.handle(crate::SetupEvent::FlashSucceeded);
        flow.handle(crate::SetupEvent::Confirm);
        flow.handle(crate::SetupEvent::ProjectGenerated {
            project_uid: "prjtest".to_string(),
        });
        flow.handle(crate::SetupEvent::PushCompleted);
    }

    #[test]
    fn a_bound_flow_rides_the_devices_own_card_from_the_verdict_on() {
        // G2 ruling 2026-08-05 + its follow-up: one physical board, ONE
        // card, at every moment. Pre-device and PRE-VERDICT the wizard is
        // standalone and the bound session's row stands down (an anonymous
        // row cannot merge with a remembered card); from the verdict on the
        // wizard is the BODY of that board's own roster card.
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();

        // Pre-device: nothing to attach to, so the wizard is standalone —
        // and no phantom "Connecting…" card rides along beside it.
        let home = studio.view().home.expect("home view");
        let wizard = home.setup.expect("the wizard is on the grid");
        assert_eq!(wizard.takeover_card, None, "nothing to be the body of yet");
        assert!(
            home.devices.is_empty(),
            "the connect narration is the wizard's own; got {:?}",
            home.devices
                .iter()
                .map(|card| &card.name)
                .collect::<Vec<_>>()
        );

        let device_id = grant_a_port(&mut studio);

        // Pre-verdict: still standalone, and the bound row stands down —
        // the wizard's PROBING body is the whole of the narration.
        let home = studio.view().home.expect("home view");
        assert_eq!(
            studio.setup.as_ref().expect("wizard").state().kind(),
            crate::SetupStateKind::Probing,
        );
        assert_eq!(
            home.setup.expect("the wizard renders").takeover_card,
            None,
            "no verdict, no card to ride"
        );
        assert!(
            home.devices.is_empty(),
            "the pre-verdict row stands down; got {:?}",
            home.devices
                .iter()
                .map(|card| &card.name)
                .collect::<Vec<_>>()
        );

        land_a_verdict(&mut studio, blank_probe(None));

        let home = studio.view().home.expect("home view");
        assert_eq!(
            home.devices.len(),
            1,
            "exactly one card carries the device; got {:?}",
            home.devices
                .iter()
                .map(|card| &card.name)
                .collect::<Vec<_>>()
        );
        let card = &home.devices[0];
        assert_eq!(
            card.session_key.as_deref(),
            Some(device_id.to_string().as_str()),
            "and it is the bound session's own card"
        );
        let wizard = home.setup.as_ref().expect("the wizard still renders");
        assert_eq!(
            wizard.takeover_card.as_deref(),
            Some(card.identity_key()),
            "the wizard rides that card's body"
        );
    }

    #[test]
    fn a_recognised_board_never_renders_twice() {
        // The G2 re-walk finding: a board the registry already knows was
        // showing its remembered card AND the connection's anonymous card.
        // The verdict is what fixes it — the probe's uid rides the live row
        // as `pending_uid`, the live card adopts the remembered identity,
        // and the roster's twin filter drops the registry row.
        use crate::app::library::{LibraryStore, MemoryLibraryHost};
        use crate::app::places::{DeviceRegistry, RegisteredDevice};
        use lpfs::LpFsMemory;

        let mut studio = StudioController::new(|| 1_800_000_000.0);
        let store = LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(|| [7u8; 16]),
            Rc::new(|| "2026-08-05-0900".to_string()),
        );
        DeviceRegistry::new(store.fs_handle())
            .upsert(RegisteredDevice {
                uid: KNOWN_UID.to_string(),
                name: "Porch sign".to_string(),
                transport: "USB".to_string(),
                last_seen_at: 1_799_000_000.0,
                association: None,
                board_id: Some("espressif/esp32-c6-devkitc-1".to_string()),
                hardware_id: Some("efuse:aa:bb:cc:dd:ee:ff".to_string()),
                previous_uids: Vec::new(),
            })
            .expect("the registry remembers this board");
        studio.attach_library(Rc::new(MemoryLibraryHost::new(store, Rc::new(|| 1.0))));
        block_on_ready(studio.settle_library());

        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();
        let home = studio.view().home.expect("home view");
        assert_eq!(
            home.devices.len(),
            1,
            "the remembered board's card is on the grid to begin with"
        );

        grant_a_port(&mut studio);
        let home = studio.view().home.expect("home view");
        assert_eq!(
            home.devices.len(),
            1,
            "pre-verdict the connection adds NO second card; got {:?}",
            home.devices
                .iter()
                .map(|card| &card.name)
                .collect::<Vec<_>>()
        );

        land_a_verdict(&mut studio, blank_probe(Some(KNOWN_UID)));

        let home = studio.view().home.expect("home view");
        assert_eq!(
            home.devices.len(),
            1,
            "and from the verdict on there is still exactly one; got {:?}",
            home.devices
                .iter()
                .map(|card| &card.name)
                .collect::<Vec<_>>()
        );
        let card = &home.devices[0];
        assert_eq!(card.uid.as_deref(), Some(KNOWN_UID), "it is THE board");
        assert_eq!(card.name, "Porch sign", "wearing the name we remember");
        assert!(card.session_key.is_some(), "and it is the LIVE card");
        assert_eq!(
            home.setup.expect("wizard").takeover_card.as_deref(),
            Some(card.identity_key()),
            "the wizard rides the merged card"
        );
    }

    /// A studio with a memory library, plus the store behind it so a test
    /// can read what the flow wrote.
    fn studio_with_library() -> (StudioController, crate::app::library::LibraryStore) {
        use crate::app::library::{LibraryStore, MemoryLibraryHost};
        use lpfs::LpFsMemory;

        let mut studio = StudioController::new(|| 1_800_000_000.0);
        let store = LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(|| [9u8; 16]),
            Rc::new(|| "2026-08-05-0900".to_string()),
        );
        studio.attach_library(Rc::new(MemoryLibraryHost::new(
            store.clone(),
            Rc::new(|| 1_800_000_000.0),
        )));
        block_on_ready(studio.settle_library());
        (studio, store)
    }

    /// Give the bound session the identity a post-flash hello resolves:
    /// a uid from silicon, and NO name (the name is what provisioning is
    /// about to write).
    fn absorb_identity(studio: &mut StudioController, device_id: crate::RuntimeId, uid: &str) {
        let session = studio
            .pool
            .device_session_mut(device_id)
            .expect("the bound session");
        session.set_device_sync(Some(crate::app::places::DeviceSyncState {
            identity: Some(crate::app::places::DeviceIdentity {
                uid: uid.to_string(),
                name: String::new(),
            }),
            content: crate::app::places::DeviceContent::Empty,
        }));
    }

    #[test]
    fn provisioning_names_the_board_under_the_identity_the_flash_gave_it() {
        // G2 blank-C6 walk, 2026-08-05: a blank board probed in its boot
        // loop anchors NO uid, so the reducer's old "write the registry
        // only when the probe carried one" meant the name the user typed
        // was written nowhere — and the push then refused the board with
        // "no named device is connected". The identity the FLASH gave the
        // board (its post-flash hello) is what the row is addressed with.
        let (mut studio, store) = studio_with_library();
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();
        let device_id = grant_a_port(&mut studio);
        // A probe that anchored nothing — the case that used to lose the name.
        land_a_verdict(&mut studio, blank_probe(None));
        {
            let flow = &mut studio.setup.as_mut().expect("wizard").flow;
            flow.handle(crate::SetupEvent::BoardChosen {
                board_id: "espressif/esp32-c6-devkitc-1".to_string(),
            });
            flow.handle(crate::SetupEvent::Confirm);
            flow.handle(crate::SetupEvent::FlashSucceeded);
            flow.handle(crate::SetupEvent::NameEdited {
                name: "Porch sign".to_string(),
            });
        }
        // …and the flash's reattach absorbed the board's identity.
        absorb_identity(&mut studio, device_id, "devfromthehello");

        block_on_ready(studio.dispatch(setup_action(HomeOp::Setup(crate::SetupGesture::Confirm))))
            .expect("provisioning runs");

        let rows = crate::app::places::DeviceRegistry::new(store.fs_handle())
            .list()
            .expect("the registry is readable");
        let row = rows
            .iter()
            .find(|row| row.uid == "devfromthehello")
            .unwrap_or_else(|| panic!("the board is remembered; got {rows:?}"));
        assert_eq!(row.name, "Porch sign", "under the name the user typed");
        assert_eq!(
            row.board_id.as_deref(),
            Some("espressif/esp32-c6-devkitc-1")
        );
        // …and the SESSION carries that name, which is what the push gate
        // reads. Without this the push refuses a board it can see.
        assert_eq!(
            studio
                .device_sync_for(device_id)
                .and_then(|sync| sync.identity.as_ref())
                .map(|identity| identity.name.as_str()),
            Some("Porch sign"),
        );
    }

    #[test]
    fn a_board_no_identity_anchors_provisions_without_a_registry_row() {
        // The other side of the same seam: neither the probe nor the
        // session can name the board, so no row is invented — the flow
        // carries on rather than writing garbage under an empty uid.
        let (mut studio, store) = studio_with_library();
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();
        grant_a_port(&mut studio);
        land_a_verdict(&mut studio, blank_probe(None));
        {
            let flow = &mut studio.setup.as_mut().expect("wizard").flow;
            flow.handle(crate::SetupEvent::BoardChosen {
                board_id: "espressif/esp32-c6-devkitc-1".to_string(),
            });
            flow.handle(crate::SetupEvent::Confirm);
            flow.handle(crate::SetupEvent::FlashSucceeded);
        }

        block_on_ready(studio.dispatch(setup_action(HomeOp::Setup(crate::SetupGesture::Confirm))))
            .expect("provisioning runs");

        assert!(
            crate::app::places::DeviceRegistry::new(store.fs_handle())
                .list()
                .expect("the registry is readable")
                .is_empty(),
            "a board anchored to nothing is remembered by nothing"
        );
    }

    #[test]
    fn closing_at_provision_leaves_the_flashed_board_connected() {
        // G2 walk: ✕ after a successful flash released the port, and the
        // board that had just been set up read "not connected" on the very
        // next frame. The session — and its card — stay.
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();
        let device_id = grant_a_port(&mut studio);
        land_a_verdict(&mut studio, blank_probe(None));
        {
            let flow = &mut studio.setup.as_mut().expect("wizard").flow;
            flow.handle(crate::SetupEvent::BoardChosen {
                board_id: "espressif/esp32-c6-devkitc-1".to_string(),
            });
            flow.handle(crate::SetupEvent::Confirm);
            flow.handle(crate::SetupEvent::FlashSucceeded);
        }
        assert_eq!(
            studio.setup.as_ref().expect("wizard").state().kind(),
            crate::SetupStateKind::Provision,
        );

        block_on_ready(studio.dispatch(setup_action(HomeOp::Setup(
            crate::SetupGesture::CloseRequested,
        ))))
        .expect("closing is not an error");

        assert!(studio.setup.is_none(), "the flow is over");
        assert!(
            studio.pool.device_session(device_id).is_some(),
            "the board keeps its session"
        );
        let home = studio.view().home.expect("home view");
        assert_eq!(
            home.devices.len(),
            1,
            "and its card is still on the grid; got {:?}",
            home.devices
                .iter()
                .map(|card| &card.name)
                .collect::<Vec<_>>()
        );
        assert!(home.setup.is_none(), "with its own body back");
    }

    #[test]
    fn the_takeover_ends_at_device_home_without_the_card_changing() {
        // "Becomes the device card" is a BODY SWAP: at DEVICE_HOME the
        // same card is still there, in the same place, wearing its own
        // body again. Nothing appears, nothing disappears.
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();
        grant_a_port(&mut studio);
        land_a_verdict(&mut studio, blank_probe(None));
        let before = studio.view().home.expect("home view");
        let key = before.devices[0].identity_key().to_string();

        walk_the_flow_to_device_home(&mut studio);
        assert_eq!(
            studio.setup.as_ref().expect("wizard").state().kind(),
            crate::SetupStateKind::DeviceHome,
        );

        let after = studio.view().home.expect("home view");
        assert_eq!(
            after
                .devices
                .iter()
                .map(|card| card.identity_key().to_string())
                .collect::<Vec<_>>(),
            vec![key],
            "the same one card, still on the grid"
        );
        assert!(
            after.setup.is_none(),
            "the takeover is gone — the card's own body is the landing"
        );
    }

    #[test]
    fn the_mid_setup_card_leads_the_roster() {
        // A card mid-setup holds a stable, leading grid position: it is
        // pinned first, ahead even of the sim's own pin, so it does not
        // hop columns as other cards land.
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();
        let idle = install_stub_device(&mut studio);
        let bound = grant_a_port(&mut studio);
        land_a_verdict(&mut studio, blank_probe(None));
        assert_ne!(idle, bound, "two boards attached");

        let home = studio.view().home.expect("home view");
        assert_eq!(home.devices.len(), 2, "both boards keep their cards");
        assert_eq!(
            home.devices[0].session_key.as_deref(),
            Some(bound.to_string().as_str()),
            "the board the flow is on leads"
        );
        assert_eq!(
            home.setup.expect("wizard").takeover_card.as_deref(),
            Some(home.devices[0].identity_key()),
        );
    }

    #[test]
    fn a_session_born_during_the_port_request_never_renders_before_the_bind() {
        // The connect-window card flash (G2, 2026-08-05): `open_provider`
        // is a long await that emits renders while the new session
        // installs, and `bind_setup_device` only runs after it returns.
        // The request's snapshot claims the newborn for the flow.
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();
        studio
            .setup
            .as_mut()
            .expect("wizard")
            .flow
            .handle(crate::SetupEvent::ItsConnected);
        // What run_setup_port_request does before the await:
        studio.setup_port_snapshot = Some(vec![]);
        // …and what the connect flow does DURING it:
        let newborn = install_stub_device(&mut studio);
        assert_eq!(studio.setup_device, None, "the bind has not landed yet");
        let home = studio.view().home.expect("home view");
        assert!(
            home.devices.is_empty(),
            "the flow's own newborn session must not flash a card; got {:?}",
            home.devices
                .iter()
                .map(|card| &card.name)
                .collect::<Vec<_>>()
        );
        // A session that predates the request keeps its card.
        studio.setup_port_snapshot = Some(vec![newborn]);
        assert!(!studio.view().home.expect("home view").devices.is_empty());
    }

    #[test]
    fn a_blank_flash_device_state_is_blank_evidence_not_unresponsive() {
        // The G2 blank-C6 walk: the board was hard-reset out of the
        // bootloader before the wizard's read, so `link_mode` was normal —
        // but the link's boot-line classifier had already concluded
        // `BlankFlash` (`invalid header: 0xffffffff`). That state IS the
        // no-firmware signature.
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        let device_id = studio
            .pool
            .install(
                crate::app::runtime_pool::RuntimePayload::stub_device_for_test(
                    DeviceState::BlankFlash,
                ),
            )
            .unwrap_or_else(|refusal| panic!("stub device installs: {}", refusal.message));
        let evidence = studio.setup_probe_evidence(device_id);
        assert!(evidence.no_firmware_signature);
        assert!(!evidence.hello_seen);
        let probe = crate::classify_board(&evidence, &[]);
        assert!(
            matches!(probe.verdict, crate::BoardVerdict::Blank { known: None }),
            "a BlankFlash link state must classify Blank, got {:?}",
            probe.verdict
        );
    }

    #[test]
    fn a_gesture_with_no_wizard_open_is_inert() {
        // The stale-click case (§2 cross-cutting): the card went away
        // between render and click. Nothing happens, and nothing errors.
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        let notices = block_on_ready(
            studio.dispatch(setup_action(HomeOp::Setup(crate::SetupGesture::Confirm))),
        )
        .expect("a stale gesture is not an error");
        assert!(notices.notices.is_empty());
        assert!(wizard_of(&studio).is_none());
    }

    #[test]
    fn the_sim_path_reaches_provision_with_a_project_line_and_no_name() {
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: true }))).unwrap();
        block_on_ready(studio.dispatch(setup_action(HomeOp::Setup(
            crate::SetupGesture::BoardChosen {
                board_id: "espressif/esp32-c6-devkitc-1".to_string(),
            },
        ))))
        .unwrap();
        block_on_ready(studio.dispatch(setup_action(HomeOp::Setup(crate::SetupGesture::Confirm))))
            .unwrap();
        let wizard = wizard_of(&studio).expect("still a card");
        assert_eq!(wizard.state.kind(), crate::SetupStateKind::Provision);
        let project = wizard.project.expect("the compact project line");
        assert!(project.summary.starts_with("meteor → 256-px strip → "));
        // §3: the sim names nothing — `can_rename` is false, so the state
        // carries no name for a field to show.
        let crate::SetupState::Provision(provision) = &wizard.state else {
            panic!("provision state")
        };
        assert_eq!(provision.name, "");
    }

    #[test]
    fn the_hardware_name_derives_from_the_injected_stamp() {
        let mut studio = StudioController::new(|| 1_800_000_000.0);
        studio.set_local_stamp(|| "2026-08-05-0930".to_string());
        block_on_ready(studio.dispatch(setup_action(HomeOp::StartSetup { sim: false }))).unwrap();
        // Walk the hardware path's tail without a wire: the machine does
        // not care where the events came from, which is the point of R1.
        let flow = &mut studio.setup.as_mut().expect("wizard").flow;
        flow.handle(crate::SetupEvent::ItsConnected);
        flow.handle(crate::SetupEvent::PortGranted);
        flow.handle(crate::SetupEvent::ProbeCompleted {
            probe: crate::BoardProbe {
                verdict: crate::BoardVerdict::Blank { known: None },
                detected_chip: Some("esp32c6".to_string()),
                hardware_uid: None,
                hardware_origin: None,
            },
        });
        flow.handle(crate::SetupEvent::BoardChosen {
            board_id: "espressif/esp32-c6-devkitc-1".to_string(),
        });
        flow.handle(crate::SetupEvent::Confirm);
        flow.handle(crate::SetupEvent::FlashSucceeded);
        let crate::SetupState::Provision(provision) = flow.state() else {
            panic!("the flash lands on provision")
        };
        assert!(
            provision.name.ends_with(" · Aug 5"),
            "the derived name reads the LIBRARY's stamp, not a clock: {}",
            provision.name
        );
    }

    #[test]
    fn the_utc_fallback_stamp_is_a_well_formed_slug_stamp() {
        // The default when a shell installs no local stamp. `Aug 5 2026,
        // 12:00 UTC` — the naming helper must be able to read it.
        let stamp = utc_slug_stamp(1_786_000_000.0);
        assert_eq!(stamp.len(), 15, "{stamp}");
        assert!(
            crate::month_day_label(&stamp).is_some(),
            "the naming helper must be able to read the fallback: {stamp}"
        );
    }
}

#[cfg(test)]
mod reattach_failure_tests {
    use super::*;
    use crate::CardOp;

    fn spec(awaits_manual_replug: bool) -> ManagementFlowSpec {
        ManagementFlowSpec {
            request: LinkManagementRequest::FlashFirmware { build_id: None },
            progress_label: "Flashing firmware",
            reconnect_detail: "Unplug the board and plug it back in to start it",
            failed_exit_label: "Back to set up",
            record_captured_logs_on_success: false,
            done_notice: provision_notice,
            degrade_subject: "firmware flashed",
            server_reconnect_failed_notice: "Firmware flashed.",
            awaits_manual_replug,
            severs_lens: false,
            result_sink: None,
        }
    }

    #[test]
    fn a_device_that_cannot_return_is_awaited_not_failed() {
        // The board was flashed from ROM download mode: it does not boot the
        // new image until a human power-cycles it. Reporting that as a
        // failure calls a successful flash a failure, on the one path a user
        // in recovery actually takes.
        let op = reattach_failure_op(&spec(true), "device did not come back");
        assert_eq!(
            op,
            CardOp::awaiting("Unplug the board and plug it back in to start it"),
            "the ending must be the instruction that finishes the job"
        );
    }

    #[test]
    fn a_device_that_should_have_returned_still_fails_loudly() {
        // The ordinary case must keep its Failed render and its single exit
        // (model §2 I4) — tolerating THIS would hide real breakage.
        let op = reattach_failure_op(&spec(false), "serial reopen failed");
        assert_eq!(
            op,
            CardOp::failed(
                "firmware flashed — reconnect failed".to_string(),
                "serial reopen failed".to_string(),
                "Back to set up",
            )
        );
    }
}
