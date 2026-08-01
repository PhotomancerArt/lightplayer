use core::future::Future;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::{CancelSignal, ProgressDeadline};
use lpa_link::{
    DeviceState, LinkManagementRequest, LinkManagementResult, LinkProvider, LinkProviderKind,
};

use crate::app::device::device_controller::DeviceRuntimeEvidence;
use crate::app::device::device_event_adapter::management_event_sink;
use crate::app::device::link_ux::management_result_logs;
use crate::app::device::{DEPLOY_NODE_ID, DeployOp, DeployTarget, DeviceOpenOutcome};
use crate::app::home::home_view_builder::HomeInputs;
use crate::app::home::{HOME_NODE_ID, HomeOp, UiHomeView, home_view_builder};
use crate::app::library::{CatalogOp, LibraryHost};
use crate::app::places::device_session::{self, DeviceContent, DeviceSyncState};
use crate::app::studio::console_command::ConsoleCommand;
use crate::app::studio::refresh_cadence::RefreshCadence;
use crate::app::studio::ui_console_view::UiConsoleView;
use crate::core::log::{LogClock, LogFilter, LogRing};
use crate::core::notice::UiNotices;
use crate::{
    AssetContentFetchOp, AssetEditOp, ConnectFlowState, Controller, ControllerContext,
    DeviceController, DeviceOp, NodeCopyOp, NodeCreateOp, NodePasteOp, NodeRemoveOp, NodeRevertOp,
    PlaylistActivateOp, ProjectConnectResult, ProjectController, ProjectEditRun, ProjectOp,
    ProjectRefreshOutcome, ProjectState, ProjectSyncRun, RuntimePayload, RuntimePool,
    ServerFailureKind, ServerSnapshot, ServerState, SlotEditOp, StudioSnapshot, UiAction,
    UiActions, UiActivityView, UiError, UiLogDraft, UiLogEntry, UiLogLevel, UiLogOrigin, UiNotice,
    UiPaneView, UiProgress, UiResult, UiStatus, UiStudioView, UiViewContent, UxActivityTarget,
    UxUpdate, UxUpdateSink,
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

pub struct StudioController {
    device: DeviceController,
    /// The runtime sessions the studio is attached to, plus the editor
    /// lens. P2 of the runtime-pool milestone: one sim AND one device
    /// session coexist under the capacity policy; every network op
    /// resolves its wire client through one of the pool's two named seams
    /// (lens-bound editor ops vs device-targeted deploy/reconcile ops).
    pool: RuntimePool,
    project: ProjectController,
    /// Bounded, chronological log buffer. Capped in core (P3/Q5) rather than in
    /// the web crate's retired 80-entry mirror.
    logs: LogRing,
    /// The console's display filter (min level + origin toggles), mutated by
    /// [`ConsoleCommand`]s. Display-side only: the ring keeps everything, the
    /// filter shapes the emitted [`UiConsoleView`].
    log_filter: LogFilter,
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
    /// The hardware card's CARD-OWNED op flow (state-flow model §2): set
    /// at management dispatch, fed by the manage event sink, and — the
    /// point (I1) — NOT cleared when the session dies, because heavy ops
    /// sever the very session that used to narrate them. `Failed` stays
    /// until the user takes its one exit (`CardUiOp::ClearOp`). Keyed by
    /// the managed device's stamped uid when it has one (`None` = an
    /// identity-less blank board — the op then rides the live non-sim
    /// card). One slot because the pool holds one hardware session; this
    /// grows into a per-identity map with the pool.
    device_card_op: Option<(Option<String>, Rc<RefCell<crate::CardOp>>)>,
    /// When the last sim crash auto-reboot ran (`None` = never). Epoch
    /// seconds on the injected clock; the flap guard: a second crash
    /// within [`SIM_CRASH_REBOOT_GUARD_SECS`] stays Failed for manual
    /// restart instead of reboot-looping a crashing project.
    sim_crash_reboot_at: Option<f64>,
    /// Injected randomness for identity minting (`dev_` uids). The web
    /// shell installs crypto randomness at startup; the default is a
    /// clock-derived fallback good enough for tests.
    random: Rc<dyn Fn() -> [u8; 16]>,
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

/// What a home card asked to open.
#[derive(Clone, Debug)]
enum PendingOpen {
    /// A library package, by key (`prj_…` uid or slug).
    Package(String),
    /// An embedded example, by id (seeded into the library on first open).
    Example(String),
}

impl PendingOpen {
    /// The card key the gallery marks busy.
    fn card_key(&self) -> &str {
        match self {
            PendingOpen::Package(key) => key,
            PendingOpen::Example(id) => id,
        }
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
        Self {
            device: DeviceController::new(),
            pool: RuntimePool::new(),
            project: ProjectController::new(),
            logs: LogRing::new(),
            log_filter: LogFilter::default(),
            now_secs: Rc::new(now_secs),
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
            device_card_op: None,
            sim_crash_reboot_at: None,
            random: Rc::new(clock_fallback_random),
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

    /// The attached hardware's device state, when hardware is attached
    /// (kind-filtered — the sim is not a device, D22).
    fn device_state(&self) -> Option<lpa_link::DeviceState> {
        self.pool
            .device_session()
            .and_then(crate::RuntimeSession::device_state)
    }

    /// Whether the attached runtime is real hardware.
    fn is_hardware_attached(&self) -> bool {
        self.pool.device_session().is_some()
    }

    /// The live hardware [`lpa_link::DeviceSession`], when one is attached
    /// (test stubs have none).
    fn hardware_session(&self) -> Option<&lpa_link::DeviceSession> {
        self.pool
            .device_session()
            .and_then(crate::RuntimeSession::hardware_session)
    }

    /// Transport label for the attached HARDWARE device ("USB" for serial
    /// classes), derived from the device SESSION's link record — never
    /// from the shared connect flow, which the sim's open may have moved
    /// on (P2 coexistence). `None` when no hardware is attached (the sim
    /// is never a device — D22).
    fn transport_label(&self) -> Option<&'static str> {
        self.pool
            .device_session()?
            .payload()
            .link_session()?
            .provider_kind
            .transport_label()
    }

    /// The pool-derived runtime evidence the device pane renders from.
    /// The server-protocol slice is the DEVICE session's when hardware is
    /// attached (the pane is about hardware), the lens session's otherwise
    /// (the "Running in the simulator" ambient line).
    fn device_runtime_evidence(&self) -> DeviceRuntimeEvidence {
        let server_state = match self.pool.device_session() {
            Some(device) => device.server_state().clone(),
            None => self.server_snapshot().state,
        };
        DeviceRuntimeEvidence {
            is_hardware: self.is_hardware_attached(),
            is_sim: self.pool.sim_session().is_some(),
            device_state: self.device_state(),
            server_state,
        }
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

    pub fn actions(&self) -> UiActions {
        UiActions::new(view_actions(&self.view()))
    }

    pub fn view(&self) -> UiStudioView {
        if let Some(home) = self.home_view() {
            return UiStudioView::new(Vec::new(), self.console_view())
                .with_home(Some(home))
                .with_lens(self.lens_runtime())
                .with_device_sync(self.device_sync().cloned())
                .with_settings(self.settings.ui_view());
        }
        let device_view = self.device.view(
            &self.device_runtime_evidence(),
            self.device_sync(),
            self.usual_device_line(),
        );
        // gallery-always (D24): home covers every no-project state, so the
        // pane layout exists only for an open project
        let mut project_pane = self.project.view(self.has_lightplayer_state());
        // Decorate every GLSL inline editor with its agent chat DTO (the
        // project walk stays agent-free; chat state lives on this
        // controller's agent sub-state).
        if let UiViewContent::ProjectEditor(editor) = &mut project_pane.body {
            self.agent
                .decorate_editor_view(editor, self.pool.lens(), &self.agent_view_context());
        }
        // Hoist the project's edit state to the shell: the web edge arms the
        // unload gate from here rather than walking the pane tree.
        let dirty = match &project_pane.body {
            UiViewContent::ProjectEditor(editor) => editor.dirty,
            _ => crate::DirtySummary::clean(),
        };
        let panes = vec![project_pane, self.bus_pane(), device_view];
        UiStudioView::new(panes, self.console_view())
            .with_lens(self.lens_runtime())
            .with_open_project(
                self.project.active_library_uid(),
                self.project.active_library_slug(),
            )
            .with_device_sync(self.device_sync().cloned())
            .with_lens_card(self.lens_device_card())
            .with_settings(self.settings.ui_view())
            .with_dirty(dirty)
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
    fn lens_device_card(&self) -> Option<crate::UiDeviceCard> {
        let session = self.pool.lens_session()?;
        let evidence = self.home_pool_evidence();
        let card = if session.is_sim() {
            evidence
                .sim
                .as_ref()
                .map(crate::app::home::home_view_builder::sim_card)
        } else {
            evidence
                .devices
                .first()
                .and_then(crate::app::home::home_view_builder::live_device_card)
        };
        card.map(|card| self.overlay_card_ui(card))
    }

    /// The lens's runtime binding for the view (SDI: the URL is the
    /// focused document — the web shell's D37 route reconciliation binds
    /// to this). A device session's `dev_` uid prefers the wire hello and
    /// falls back to the connect-as-pull identity.
    fn lens_runtime(&self) -> Option<crate::UiLensRuntime> {
        self.pool.lens_session().map(|session| {
            if session.is_sim() {
                // the session's loaded-project record (not the library
                // binding) is the key: it survives detach, so re-attach
                // flows address the same document
                crate::UiLensRuntime::Sim {
                    project_key: session
                        .sim_loaded_project()
                        .map(|project| project.name.clone()),
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
        Some(view)
    }

    /// The runtime pool's roster evidence (P4): one evidence bundle per
    /// DEVICE session — reconcile state reads the device session, never
    /// the lens, which may be on the sim (P2 coexistence) — plus the SIM
    /// session's evidence while it lives (D36: the sim card exists exactly
    /// as long as the session does). The connect flow's transient evidence
    /// (a connect in flight before any session exists) rides the device
    /// entry, exactly as the single-session shape carried it.
    fn home_pool_evidence(&self) -> crate::app::home::HomePoolEvidence {
        let (observed_version, head_version) = self
            .pool
            .device_session()
            .map(crate::RuntimeSession::device_versions)
            .unwrap_or((None, None));
        let (local_saved_at, pushed_at) = self
            .pool
            .device_session()
            .map(crate::RuntimeSession::device_drift_times)
            .unwrap_or((None, None));
        // A long-running operation on the device session (flash / erase /
        // push — the same flag that blocks pool replaces) owns the card's
        // narration; the connect flow narrates otherwise.
        let connect = match self
            .pool
            .device_session()
            .and_then(|session| session.operation_label().map(str::to_string))
        {
            Some(label) => crate::ConnectEvidence::OperationInFlight {
                label,
                percent: None,
            },
            None => self.gallery_connect_evidence(),
        };
        let device = crate::app::home::HomeDeviceEvidence {
            sync: self.device_sync().cloned(),
            link: self.device_state(),
            connect,
            transport: self.transport_label().map(str::to_string),
            observed_version,
            head_version,
            local_saved_at,
            pushed_at,
            pending_uid: self.device.pending_reconnect_uid().map(str::to_string),
            console_tail: self
                .pool
                .device_session()
                .map(|session| session.console_tail().iter().cloned().collect())
                .unwrap_or_default(),
        };
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
                console_tail: session.console_tail().iter().cloned().collect(),
            });
        crate::app::home::HomePoolEvidence {
            devices: vec![device],
            sim,
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

    /// The bus pane: a derived view over the binding-graph snapshot.
    ///
    /// Temporary placement (roadmap M3): rides the main column under the
    /// Project pane; the pane's final home is an open UX question.
    fn bus_pane(&self) -> UiPaneView {
        let (status, view) = match self.project.ui_bus_view() {
            Some(view) if !view.channels.is_empty() => (
                UiStatus::good(format!("{} channels", view.channels.len())),
                view,
            ),
            Some(view) => (UiStatus::neutral("No channels"), view),
            None => (UiStatus::working("Reading"), crate::UiBusView::empty()),
        };
        UiPaneView::new(
            "bus",
            "Bus",
            status,
            UiViewContent::Bus(Box::new(view)),
            Vec::new(),
        )
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
        self.library_host = Some(Rc::clone(&host));
        self.project.set_library(host, clock);
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
                self.home_inputs =
                    Some(home_view_builder::hydrate_home_inputs(fs, &open_elsewhere));
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

    /// What the attached device holds (connect-as-pull result), for the
    /// pane and cards. `None` while no hardware is
    /// attached (the sim carries no reconcile bundle — D22). A pool read
    /// since the runtime-pool extraction: the bundle lives on the DEVICE
    /// session, wherever the lens is (P2 coexistence).
    pub fn device_sync(&self) -> Option<&DeviceSyncState> {
        self.pool
            .device_session()
            .and_then(crate::RuntimeSession::device_sync)
    }

    /// Connect-is-a-pull (D8): pull the attached device's copy, classify
    /// it against the library, persist per the M4b locking model, refresh
    /// the registry, and cache the result. Never fails the connect —
    /// errors are logged and leave the state `None` (flash/erase must
    /// stay reachable on a device we can't read).
    pub(crate) async fn refresh_device_sync(&mut self) {
        if let Ok(session) = self.pool.device_session_mut() {
            session.clear_reconcile();
        }
        let pulled = {
            let Ok(session) = self.pool.device_session_mut() else {
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
                    if let Ok(session) = self.pool.device_session_mut() {
                        session.set_device_sync(Some(DeviceSyncState {
                            identity: None,
                            content: DeviceContent::Unreadable {
                                detail: format!("could not read the device: {error}"),
                            },
                        }));
                    }
                    self.mark_dirty();
                    return;
                }
            }
        };
        if let Ok(session) = self.pool.device_session_mut() {
            session.set_device_storage_id(Some(pulled.storage_id.clone()));
        }
        match self.absorb_device_pull(pulled).await {
            Ok(state) => {
                if let Ok(session) = self.pool.device_session_mut() {
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
        mut pulled: device_session::PulledDeviceCopy,
    ) -> Result<DeviceSyncState, UiError> {
        self.record_logs(core::mem::take(&mut pulled.logs));
        let now = (self.now_secs)();
        let identity = self.reconcile_identity_name(pulled.identity.clone()).await;

        if let Some(identity) = &identity {
            self.upsert_device_entry(identity, now).await;
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
            if let Ok(session) = self.pool.device_session_mut() {
                session.set_device_versions(versions);
                session.set_device_drift_times((head_saved_at, pushed_at));
            }
            let Some(identity_value) = identity.clone() else {
                // anonymous hardware: classification only (the wizard
                // stamps an identity, then this re-runs)
                let content = DeviceContent::Known {
                    project_uid: summary.uid.to_string(),
                    slug: summary.slug.clone(),
                    observed: pulled.observed,
                    relation,
                };
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
                    device: device_session::registry_entry_for(
                        &identity_value,
                        self.transport_label().unwrap_or_default(),
                        now,
                    ),
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
            let mut relation = relation;
            if relation == lpc_history::SyncRelation::Diverged
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
                        if let Ok(session) = self.pool.device_session_mut() {
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

            let content = DeviceContent::Known {
                project_uid: summary.uid.to_string(),
                slug: summary.slug.clone(),
                observed: pulled.observed,
                relation,
            };
            self.request_library_refresh();
            return Ok(DeviceSyncState { identity, content });
        }

        // unknown project: adopt when the device has an identity to
        // attribute it to; otherwise wait for the wizard to stamp one
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
                device: device_session::registry_entry_for(
                    identity_value,
                    self.transport_label().unwrap_or_default(),
                    now,
                ),
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
        Ok(DeviceSyncState {
            identity,
            content: DeviceContent::Adopted {
                project_uid: summary.uid.to_string(),
                slug: summary.slug,
                observed: pulled.observed,
            },
        })
    }

    /// D34 name reconcile at connect: the registry name is the user-facing
    /// truth, so a device reporting a stale name (renamed while offline)
    /// gets the registry name written back to `/.lp/device.json` — and the
    /// UI uses the registry name either way. A failed write-back only
    /// logs: the next connect retries, and the registry keeps winning in
    /// the meantime (`upsert_device_merged`).
    async fn reconcile_identity_name(
        &mut self,
        identity: Option<crate::app::places::DeviceIdentity>,
    ) -> Option<crate::app::places::DeviceIdentity> {
        let mut identity = identity?;
        let registry_name = match self.library_host() {
            Ok(host) => match host.catalog_snapshot().await {
                Ok(fs) => crate::app::places::DeviceRegistry::new(fs)
                    .list()
                    .unwrap_or_default()
                    .into_iter()
                    .find(|entry| entry.uid == identity.uid)
                    .map(|entry| entry.name),
                Err(_) => None,
            },
            Err(_) => None,
        };
        if let Some(registry_name) = registry_name
            && !registry_name.is_empty()
            && registry_name != identity.name
        {
            use lpc_model::AsLpPath;
            identity.name = registry_name;
            match self
                .pool
                .device_session_mut()
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
        Some(identity)
    }

    /// Record the device sighting in the registry (merge semantics: an
    /// association survives sight-only upserts).
    async fn upsert_device_entry(
        &mut self,
        identity: &crate::app::places::DeviceIdentity,
        now: f64,
    ) {
        let Ok(host) = self.library_host() else {
            return;
        };
        let entry = device_session::registry_entry_for(
            identity,
            self.transport_label().unwrap_or_default(),
            now,
        );
        if let Err(error) = host.catalog(CatalogOp::UpsertRegisteredDevice(entry)).await {
            log::warn!("device registry upsert failed: {error}");
        }
        self.request_library_refresh();
    }

    /// Where the open project usually lives: the registered device whose
    /// association points at it, for the pane's disconnected state (D23).
    fn usual_device_line(&self) -> Option<String> {
        let slug = self.project.active_library_slug()?;
        let inputs = self.home_inputs.as_ref()?;
        inputs.devices.iter().find_map(|device| {
            let offline = matches!(device.state, crate::RosterCardState::Offline { .. });
            let holds_it = device
                .project
                .as_ref()
                .is_some_and(|chip| chip.name == slug);
            (offline && holds_it).then(|| format!("Usually on {}.", device.name))
        })
    }

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

    async fn execute_deploy_op(&mut self, op: DeployOp, updates: UxUpdateSink) -> UiResult {
        match op {
            DeployOp::PushProject { key } => {
                // The direct push (M5; since M8′ the ONLY push): the
                // dispatching gesture is the D11 consent. The device must
                // carry a stamped identity (the Running-family states and
                // the picker's Connected-empty guarantee it; unstamped
                // boards go through the name sheet first).
                let device = self
                    .device_sync()
                    .and_then(|sync| sync.identity.clone())
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
                self.pool.device_session_mut()?.set_operation(Some(label));
                self.mark_dirty();
                updates.emit(UxUpdate::View(self.view()));
                let result = self.run_device_push(&device, &target).await;
                if let Ok(session) = self.pool.device_session_mut() {
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
            DeployOp::AdoptDeviceCopy => {
                let (project_uid, observed) = self.diverged_device_copy()?;
                let host = self.library_host()?;
                host.catalog(CatalogOp::AdoptObservedVersion {
                    project_uid,
                    observed,
                })
                .await
                .map_err(|error| self.library_error_with_name(error))?;
                self.request_library_refresh();
                self.refresh_device_sync().await;
                Ok(UiNotices::new()
                    .with_notice(UiNotice::info("The device's version is now the newest")))
            }
            DeployOp::KeepBothFork => {
                let (project_uid, observed) = self.diverged_device_copy()?;
                // the fork's name: the live session's stamped identity
                // (the D30 sheet is the one entry since M8′)
                let device_name = self
                    .device_sync()
                    .and_then(|sync| sync.identity.as_ref())
                    .map(|identity| identity.name.clone())
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
            DeployOp::EraseDevice => {
                // from the card's Danger tab, behind the D41 confirm sheet
                self.reset_to_blank(updates).await
            }
        }
    }

    /// The diverged copy an adopt/keep-both verb targets: the live
    /// device session's sync evidence (the D30 card sheet is the one
    /// entry since M8′ — there is no dialog).
    fn diverged_device_copy(&mut self) -> Result<(String, lpc_history::ContentHash), UiError> {
        match self.device_sync().map(|sync| &sync.content) {
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

    /// Stamp a `dev_` identity onto the connected device: mint the uid,
    /// write `/.lp/device.json` at the device's fs ROOT over the wire
    /// (identity is device-scoped, outside every project storage dir),
    /// register the device, and re-pull (adoption may now run for
    /// previously-anonymous content).
    async fn run_identity_stamp(
        &mut self,
        name: String,
    ) -> Result<crate::app::places::DeviceIdentity, UiError> {
        use lpc_model::AsLpPath;
        let identity = crate::app::places::DeviceIdentity {
            uid: lpc_history::PrefixedUid::mint(lpc_history::UidPrefix::Device, &(self.random)())
                .to_string(),
            name,
        };
        {
            let server = self.pool.device_session_mut()?.client_mut()?;
            let logs = server
                .fs_write(
                    crate::app::places::DEVICE_IDENTITY_PATH.as_path(),
                    &identity.to_json_bytes(),
                )
                .await?;
            self.record_logs(logs);
        }
        let now = (self.now_secs)();
        self.upsert_device_entry(&identity, now).await;
        self.refresh_device_sync().await;
        Ok(identity)
    }

    /// Push a library head to the device: hash-verified replace-and-load,
    /// then the push event + association. Identity lives at the device's
    /// fs root, so the storage-dir replace never touches it. The library
    /// side prefers the active handle (this tab owns it); otherwise a
    /// snapshot read + catalog transaction.
    async fn run_device_push(
        &mut self,
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
                .device_session()
                .and_then(|session| session.device_storage_id().map(str::to_string))
                .unwrap_or_else(|| {
                    crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID.to_string()
                });
            let server = self.pool.device_session_mut()?.client_mut()?;
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
            let mut entry = device_session::registry_entry_for(
                device,
                self.transport_label().unwrap_or_default(),
                now,
            );
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
                device: device_session::registry_entry_for(
                    device,
                    self.transport_label().unwrap_or_default(),
                    now,
                ),
                version: local_hash,
            })
            .await
            .map_err(|error| self.library_error_with_name(error))?;
        }

        // 4. the device now runs the pushed head
        self.refresh_device_sync().await;
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
            if action.op_as::<NodeRevertOp>().is_some() {
                let op = action.into_op::<NodeRevertOp>()?;
                return self.execute_node_revert_op(op).await;
            }
            if action.op_as::<PlaylistActivateOp>().is_some() {
                let op = action.into_op::<PlaylistActivateOp>()?;
                return self.execute_playlist_activate_op(op).await;
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
            if action.op_as::<NodePasteOp>().is_some() {
                let op = action.into_op::<NodePasteOp>()?;
                return self.execute_node_paste_op(op).await;
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
            DeviceOp::DisconnectDevice => self.disconnect_device().await,
            DeviceOp::StopSimulator => self.stop_simulator().await,
            DeviceOp::DisconnectLightPlayer => self.disconnect_lightplayer().await,
            DeviceOp::SetLogLevel { level } => self.set_device_log_level(level).await,
            DeviceOp::ResetDevice => self.reset_device(updates).await,
            DeviceOp::ConnectLightPlayer => {
                // A device-pane op: target the hardware session when one
                // exists; the lens session (the sim's reconnect) otherwise.
                let id = self
                    .pool
                    .device_session()
                    .map(crate::RuntimeSession::id)
                    .or_else(|| self.pool.lens())
                    .ok_or_else(|| {
                        UiError::MissingSession("link connection is not open".to_string())
                    })?;
                self.connect_server_from_link(id, updates).await
            }
            DeviceOp::ProvisionFirmware { setup_name } => {
                self.provision_firmware(updates, setup_name).await
            }
            DeviceOp::WipeProject => self.wipe_project().await,
            DeviceOp::ResetToBlank => self.reset_to_blank(updates).await,
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
                if provider_id != LinkProviderKind::BrowserSerialEsp32 {
                    emit_activity(
                        &updates,
                        device_section_target(DeviceController::SECTION_DEVICE),
                        "Opening device",
                        "Opening",
                        format!("Opening {}", provider_id.label()),
                    );
                }
                let outcome = self.device.open_provider(provider_id).await;
                self.settle_connect_outcome(runtime_kind_for(provider_id), outcome, updates)
                    .await
            }
            DeviceOp::ConnectEndpoint {
                provider_id,
                endpoint_id,
            } => {
                emit_activity(
                    &updates,
                    device_section_target(DeviceController::SECTION_DEVICE),
                    "Opening device session",
                    "Connecting",
                    "Opening device endpoint",
                );
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
        if self.pool.device_session().is_some() {
            return Ok(UiNotices::new());
        }
        if matches!(
            self.device.flow_state(),
            ConnectFlowState::DiscoveringEndpoints { .. }
                | ConnectFlowState::Connecting { .. }
                | ConnectFlowState::Retrying { .. }
        ) {
            return Ok(UiNotices::new());
        }
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
        match outcome {
            Ok(DeviceOpenOutcome::Opened) => {
                self.pool.remove_kind(kind);
                Ok(UiNotices::new())
            }
            Ok(DeviceOpenOutcome::Cancelled { message }) => {
                self.pool.remove_kind(kind);
                Ok(UiNotices::new().with_notice(UiNotice::info(message)))
            }
            Ok(DeviceOpenOutcome::Connected { payload, logs }) => {
                self.record_logs(logs);
                let id = self.install_session(payload).await?;
                self.attach_runtime(id, updates).await
            }
            Ok(DeviceOpenOutcome::SoftFailed) => {
                // M6: the ladder's honest ending lives on the CARD
                // (PortHeld/Unresponsive flow states → card evidence);
                // nothing toasts, nothing errors.
                self.pool.remove_kind(kind);
                self.mark_dirty();
                Ok(UiNotices::new())
            }
            Err(error) => {
                self.pool.remove_kind(kind);
                Err(error)
            }
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
        let lens_replaced = self
            .pool
            .lens_session()
            .is_some_and(|session| session.kind() == payload.kind());
        match self.pool.install(payload) {
            Ok(id) => {
                if lens_replaced {
                    self.project.reset();
                }
                Ok(id)
            }
            Err(refusal) => {
                let message = refusal.message;
                match refusal.payload {
                    crate::RuntimePayload::Sim(sim) => {
                        let _ = sim.connector.close(&sim.session.id).await;
                    }
                    crate::RuntimePayload::Device(handle) => {
                        let _ = handle.close().await;
                    }
                }
                Err(UiError::UnsupportedAction(message))
            }
        }
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
            HomeOp::CreateProject => {
                // Create-and-open (the D17 deviation, 2026-07-27): a pure
                // blank one-file package lands in the library — "Project",
                // slugged/dated/deduped by the store; rename lives on the
                // card kebab — then opens like any card, so the user lands
                // in the empty editor with the add-node picker.
                let outcome = self
                    .run_catalog_op(CatalogOp::Create {
                        name: "Project".to_string(),
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
                let imported = outcome
                    .summary
                    .map(|summary| summary.name)
                    .unwrap_or_default();
                Ok(UiNotices::new().with_notice(UiNotice::info(format!("Imported {imported}"))))
            }
            HomeOp::ImportJson { text } => {
                let outcome = self.run_catalog_op(CatalogOp::ImportJson { text }).await?;
                let imported = outcome
                    .summary
                    .map(|summary| summary.name)
                    .unwrap_or_default();
                Ok(UiNotices::new().with_notice(UiNotice::info(format!("Pasted {imported}"))))
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
            HomeOp::ForgetDevice { uid } => {
                self.run_catalog_op(CatalogOp::ForgetRegisteredDevice { uid })
                    .await?;
                Ok(UiNotices::new().with_notice(UiNotice::info("Device forgotten")))
            }
            HomeOp::NameDevice { name } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Err(UiError::UnsupportedAction(
                        "a device name cannot be empty".to_string(),
                    ));
                }
                // the Needs-a-name card's inline form (D14 gently insists
                // upstream): stamp mints the uid, writes the identity over
                // the wire, registers the device, and re-pulls (adoption
                // may now run for previously-anonymous content)
                let identity = self.run_identity_stamp(name).await?;
                Ok(UiNotices::new().with_notice(UiNotice::info(format!(
                    "This device is now \"{}\"",
                    identity.name
                ))))
            }
            HomeOp::CardUi(op) => {
                self.apply_card_ui_op(op);
                Ok(UiNotices::new())
            }
        }
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
            // The failed op's ONE exit (model §2 I4): drop the flow; the
            // card re-derives its honest state on the next view build.
            CardUiOp::ClearOp { .. } => {
                self.device_card_op = None;
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
        if let Some(saved) = self.card_ui.get(card.identity_key()) {
            card.ui = saved.clone();
        }
        // The CARD-OWNED op flow first (model §2, I1): it survives the
        // session the op severed, so it outranks session-derived
        // narration. Targeting: the stamped uid when the managed device
        // had one; an identity-less blank board's op rides the live
        // (non-offline) hardware card.
        if let Some((target_uid, slot)) = self.device_card_op.as_ref()
            && card.takes_card_op(target_uid.as_deref())
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
            // Only the session's OWN card narrates the session op: the
            // stamped identity must match the card's uid. A remembered
            // (offline) card also carries a uid, so a bare is_some()
            // check smeared one device's push across every device card.
            // An identity-less live board has no uid on either side —
            // its card is the one mid-operation.
            let live_uid = self
                .device_sync()
                .and_then(|sync| sync.identity.as_ref())
                .map(|identity| identity.uid.as_str());
            self.pool
                .device_session()
                .filter(|_| match (card.uid.as_deref(), live_uid) {
                    (Some(card_uid), Some(live_uid)) => card_uid == live_uid,
                    _ => matches!(card.state, crate::RosterCardState::OperationInFlight { .. }),
                })
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
    /// the attached one, write `/.lp/device.json` back over the wire and
    /// update the cached sync state so every surface shows the new name
    /// immediately. Offline devices skip this — the write-back happens on
    /// the next connect (`reconcile_identity_name`).
    async fn write_back_live_identity_name(
        &mut self,
        uid: &str,
        name: &str,
    ) -> Result<(), UiError> {
        use lpc_model::AsLpPath;
        let is_live = self
            .device_sync()
            .and_then(|sync| sync.identity.as_ref())
            .is_some_and(|identity| identity.uid == uid);
        if !is_live {
            return Ok(());
        }
        let identity = crate::app::places::DeviceIdentity {
            uid: uid.to_string(),
            name: name.to_string(),
        };
        let logs = self
            .pool
            .device_session_mut()?
            .client_mut()?
            .fs_write(
                crate::app::places::DEVICE_IDENTITY_PATH.as_path(),
                &identity.to_json_bytes(),
            )
            .await?;
        self.record_logs(logs);
        if let Some(sync) = self
            .pool
            .device_session_mut()
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
        self.library_host()?;
        self.pending_open = Some(pending);
        let result = self.open_from_home_inner(updates).await;
        self.pending_open = None;
        result
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
        // The open targets THE sim session: reuse it when it exists — the
        // lens moves onto it (the editor mirror opens on the sim) — and
        // replace-and-load directly when its server protocol is live, or
        // reconnect its server first when not. A fresh install claims the
        // lens by the pool's lens-less rule (the quiesce above cleared it).
        if let Some(sim) = self.pool.sim_session() {
            let sim_id = sim.id();
            let server_live = matches!(sim.server_state(), ServerState::Connected { .. });
            // D37/M5 (`#/sim/<key>` — and the project-card click that now
            // rides it): when the sim ALREADY runs the requested project,
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
                return self.attach_lens(sim_id, updates).await;
            }
            self.pool.set_lens(sim_id);
            if server_live {
                return self.open_pending_package(updates).await;
            }
            return self.connect_server_from_link(sim_id, updates).await;
        }
        // No sim yet: start the simulator runtime. A device session stays
        // attached throughout — only the SIM slot is touched on failure.
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_DEVICE),
            "Starting simulator",
            "Opening",
            "Starting the simulator runtime",
        );
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
        let pending = self
            .pending_open
            .clone()
            .ok_or_else(|| UiError::MissingSession("no pending package to open".to_string()))?;
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_DEVICE),
            "Opening project",
            "Opening",
            "Pushing the project to the simulator",
        );
        let result = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            match &pending {
                PendingOpen::Package(key) => self.project.open_library_package(server, key).await,
                PendingOpen::Example(id) => self.project.open_example_package(server, id).await,
            }
        };
        match result {
            Ok(logs) => {
                self.record_logs(logs);
                self.note_sim_loaded_project();
                let sync = self.sync_project_after_attach(updates).await?;
                Ok(UiNotices::new().with_notice(project_sync_notice(
                    sync.synced,
                    "Project opened",
                    "Project opened; project sync needs attention",
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

    async fn execute_project_op(&mut self, op: ProjectOp, updates: UxUpdateSink) -> UiResult {
        match op {
            ProjectOp::ConnectRunningProject => self.connect_running_project(updates).await,
            ProjectOp::ConnectLoadedProject { handle_id } => {
                self.connect_loaded_project(handle_id, updates).await
            }
            ProjectOp::LoadDemoProject => self.load_demo_project(updates).await,
            ProjectOp::RefreshProject => self.refresh_project(updates).await,
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
                self.record_project_edit_run(run)
            }
            ProjectOp::RevertAllEdits => {
                let run = {
                    let server = self.pool.lens_session_mut()?.client_mut()?;
                    self.project.revert_all_edits(server).await
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
                self.agent.record_upsert_ack(&artifact, seq, rejection);
                Ok(run.notices)
            }
            Err(error) => {
                self.agent
                    .record_upsert_ack(&artifact, seq, Some(error.to_string()));
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
        // session is untouched (P2 coexistence).
        if let Ok(session) = self.pool.device_session_mut() {
            session.disconnect_server();
        }
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_DEVICE),
            "Opening device for flashing",
            "Opening",
            "Opening device without attaching LightPlayer",
        );
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
            emit_activity(
                &updates,
                device_section_target(DeviceController::SECTION_DEVICE),
                "Reopening device",
                "Connecting",
                "Resetting device before server connect",
            );
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
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_DEVICE),
            "Connecting LightPlayer",
            "Connecting",
            "Opening server protocol",
        );
        let server_updates = retarget_activity_updates(
            updates.clone(),
            device_section_target(DeviceController::SECTION_DEVICE),
        );
        let (is_sim, attach_result) = match self.pool.session_mut(id) {
            Some(session) => (session.is_sim(), session.attach_server(server_updates)),
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
                    device_section_target(DeviceController::SECTION_DEVICE),
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
                    self.probe_server_readiness(id, updates.clone()).await
                };
                let auto_connect = match probe {
                    Ok(auto_connect) => auto_connect,
                    Err(error) => {
                        let pending_logs = self
                            .pool
                            .session_mut(id)
                            .map(|session| session.take_pending_logs())
                            .unwrap_or_default();
                        self.record_logs(pending_logs);
                        let device_logs = self.device.take_pending_device_logs();
                        self.record_logs(device_logs);
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
                        if matches!(self.device_state(), Some(DeviceState::Incompatible { .. })) {
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
                    self.refresh_device_sync().await;
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
            device_section_target(DeviceController::SECTION_DEVICE),
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
        updates: UxUpdateSink,
    ) -> Result<AutoProjectConnect, UiError> {
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_DEVICE),
            "Checking device",
            "Checking",
            "Checking server response",
        );
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
            device_section_target(DeviceController::SECTION_DEVICE),
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
            device_section_target(DeviceController::SECTION_DEVICE),
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
            device_section_target(DeviceController::SECTION_DEVICE),
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
    fn note_sim_loaded_project(&mut self) {
        let project = self
            .project
            .active_library_uid()
            .zip(self.project.active_library_slug());
        if let Some((uid, name)) = project
            && let Ok(session) = self.pool.lens_session_mut()
            && session.is_sim()
        {
            session.set_sim_loaded_project(Some(crate::SimLoadedProject { uid, name }));
        }
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
        if let Some(session) = self.pool.device_session() {
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
            .device_session()
            .is_some_and(crate::RuntimeSession::is_connected);
        if !connected {
            // Chooser opened, cancelled, or a non-Ready device (blank /
            // foreign / incompatible): the card carries the state; the
            // lens stays where it is.
            return Ok(notices);
        }
        let id = self
            .pool
            .device_session()
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
            self.project.sync_loaded_project(server).await?
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

    async fn disconnect_device(&mut self) -> UiResult {
        self.project.reset();
        // Taking a session out of the pool drops its wire client, server
        // state, and reconcile bundle with it; the controller closes each
        // payload and resets the connect flow. This is the EXPLICIT
        // disconnect affordance and keeps its full teardown meaning (P3);
        // the gallery-return route policy now dispatches the lens detach
        // (`ProjectOp::DetachLens`) instead, which keeps every session.
        let sessions = self.pool.take_all_sessions();
        if sessions.is_empty() {
            self.device.disconnect(None).await?;
        } else {
            for session in sessions {
                self.device.disconnect(Some(session.into_payload())).await?;
            }
        }
        Ok(UiNotices::new().with_notice(UiNotice::info("Device disconnected")))
    }

    /// Detach the server protocol while keeping the runtime attached (the
    /// device pane's "Disconnect LightPlayer" affordance — the
    /// keep-worker-drop-client precedent P3's lens detach built on).
    /// Re-homed for the pool: the op is a device-pane affordance, so it
    /// targets the HARDWARE session when one exists, the lens session
    /// otherwise; the mirror only quiesces when the lens sat on the
    /// disconnected session (a project open on the sim survives).
    async fn disconnect_lightplayer(&mut self) -> UiResult {
        let id = self
            .pool
            .device_session()
            .map(crate::RuntimeSession::id)
            .or_else(|| self.pool.lens())
            .ok_or_else(|| UiError::MissingSession("server client is not connected".to_string()))?;
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

    async fn reset_device(&mut self, updates: UxUpdateSink) -> UiResult {
        self.run_device_management(
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
                severs_lens: false,
            },
            updates,
        )
        .await
    }

    async fn provision_firmware(
        &mut self,
        updates: UxUpdateSink,
        setup_name: Option<String>,
    ) -> UiResult {
        let mut outcome = self
            .run_device_management(
                ManagementFlowSpec {
                    request: LinkManagementRequest::FlashFirmware,
                    progress_label: "Flashing firmware",
                    reconnect_detail: "Waiting for firmware boot",
                    failed_exit_label: "Back to set up",
                    record_captured_logs_on_success: false,
                    done_notice: provision_notice,
                    degrade_subject: "firmware flashed",
                    server_reconnect_failed_notice:
                        "Firmware flashed; reconnect the server after the device finishes booting",
                    // a flash reboots into new firmware; the stored project
                    // survives and reloads on reattach.
                    severs_lens: false,
                },
                updates,
            )
            .await?;
        // The setup form's name stamps at first post-flash contact
        // (model §1-A): the happy path never detours through
        // Needs-a-name. Only an identity-less board takes the stamp —
        // an update on a stamped device keeps its name. Naming failure
        // degrades honestly: the flash stands, the card offers naming.
        let setup_name = setup_name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty());
        if let Some(name) = setup_name
            && self
                .device_sync()
                .is_some_and(|sync| sync.identity.is_none())
        {
            match self.run_identity_stamp(name).await {
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
        Ok(outcome)
    }

    /// Wipe the device's PROJECT storage back to blank over the live wire
    /// (the Holds-unreadable-data card's way out — model rev 2026-07-26:
    /// the way out is BLANK, never push-over). Firmware stays; the
    /// re-pull reclassifies the board as Connected-empty (landing I3).
    /// The unreadable content cannot be banked (that's what unreadable
    /// means) — the confirm sheet's copy says so plainly; raw-image
    /// backup is the model-§5 backlog item.
    async fn wipe_project(&mut self) -> UiResult {
        let storage_id = self
            .pool
            .device_session()
            .and_then(|session| session.device_storage_id().map(str::to_string))
            .ok_or_else(|| {
                UiError::MissingSession("no device project storage to wipe".to_string())
            })?;
        let logs = self
            .pool
            .device_session_mut()?
            .client_mut()?
            .replace_project_files(
                &storage_id,
                Vec::<lpa_client::project_deploy::ProjectDeployFile>::new(),
            )
            .await?;
        self.record_logs(logs);
        self.refresh_device_sync().await;
        self.mark_dirty();
        Ok(UiNotices::new().with_notice(UiNotice::info(
            "Wiped — this board is empty now. Pick a project to put on it.",
        )))
    }

    async fn reset_to_blank(&mut self, updates: UxUpdateSink) -> UiResult {
        self.run_device_management(
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
                severs_lens: true,
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
        spec: ManagementFlowSpec,
        updates: UxUpdateSink,
    ) -> UiResult {
        let device_id = self
            .pool
            .device_session()
            .map(crate::RuntimeSession::id)
            .ok_or_else(|| {
                UiError::MissingSession("no hardware device session for management".to_string())
            })?;
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
        // The CARD-OWNED op flow starts here (model §2): the slot lives on
        // the controller, keyed by the managed device's stamped uid, and
        // survives whatever happens to the session below (I1). The event
        // sink feeds it; the settle half flips it to Failed or clears it.
        let managed_uid = self
            .device_sync()
            .and_then(|sync| sync.identity.as_ref())
            .map(|identity| identity.uid.clone());
        let card_op = Rc::new(RefCell::new(crate::CardOp::new(
            format!("{}…", spec.progress_label),
            None,
        )));
        let managed_uid_for_events = managed_uid.clone();
        self.device_card_op = Some((managed_uid, Rc::clone(&card_op)));
        if let Some(session) = self.pool.session_mut(device_id) {
            session.disconnect_server();
            // The pool refuses a same-kind replace while this runs (DQ-A
            // swap semantics), and the label narrates the device card's
            // Operation-in-flight lane; cleared when the manage half
            // settles.
            session.set_operation(Some(spec.progress_label.to_string()));
        }
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        let target = device_section_target(DeviceController::SECTION_DEVICE);
        let activity = Rc::new(RefCell::new(
            UiActivityView::new(spec.progress_label)
                .with_progress(UiProgress::indeterminate(spec.progress_label)),
        ));
        // MOUNT THE OVERLAY BEFORE THE WORK STARTS. The op slot went in
        // just above, and only a full view build carries it onto a card
        // (`overlay_card_ui`) — but everything below holds `&mut self`
        // until the operation ends, so this is the last chance to build
        // one. Without it the first view wearing the op is the one
        // `attach_runtime` emits AFTER the flash, which is what made a
        // minute-long install look like a dead button.
        self.mark_dirty();
        updates.emit(UxUpdate::View(self.view()));
        updates.emit(UxUpdate::Activity {
            target: target.clone(),
            status: UiStatus::working("Managing"),
            activity: activity.borrow().clone(),
        });
        let event_sink = management_event_sink(
            updates.clone(),
            target,
            Rc::clone(&activity),
            Rc::clone(&captured_logs),
            Rc::clone(&card_op),
            managed_uid_for_events.clone(),
            spec.reconnect_detail,
        );
        let manage_result = {
            let session = match self.hardware_session() {
                Some(session) => session,
                None => {
                    if let Some(session) = self.pool.session_mut(device_id) {
                        session.set_operation(None);
                    }
                    // The op never started — clean abort, no Failed render.
                    self.device_card_op = None;
                    return Err(UiError::MissingSession(
                        "no hardware device session for management".to_string(),
                    ));
                }
            };
            session.manage(spec.request, event_sink).await
        };
        // The manage half settled (either way): session replaces unblock
        // and the card's operation narration clears.
        if let Some(session) = self.pool.session_mut(device_id) {
            session.set_operation(None);
        }
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
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_DEVICE),
            "Reconnecting device",
            "Connecting",
            spec.reconnect_detail,
        );
        // The reattach half is the op's AwaitingDevice phase (I2) — the
        // overlay stays up, narrating the expected gap. It is a LONG
        // phase (the board reboots and re-enumerates), so the delta goes
        // out too: the dispatch has not returned, and only a full
        // snapshot would otherwise carry the phase change.
        *card_op.borrow_mut() = crate::CardOp::awaiting(spec.reconnect_detail);
        updates.emit(UxUpdate::CardOp {
            uid: managed_uid_for_events,
            op: card_op.borrow().clone(),
        });
        self.mark_dirty();
        // The link was already rebuilt inside `manage`; what remains is the
        // server reattach + post-attach sequence on the managed session.
        match self.attach_runtime(device_id, updates).await {
            Ok(mut attach_outcome) => {
                outcome.notices.append(&mut attach_outcome.notices);
                // Landed (I3): the flow ends; the card re-derives and its
                // Status tab announces what's next.
                self.device_card_op = None;
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
                *card_op.borrow_mut() = crate::CardOp::failed(
                    format!("{} — reconnect failed", spec.degrade_subject),
                    error.to_string(),
                    spec.failed_exit_label,
                );
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
    pub(crate) fn runtime_pool_for_test(&self) -> &RuntimePool {
        &self.pool
    }

    /// The storage dir library sync targets, for the save-as-pull
    /// wiring regression (2026-07-26).
    #[cfg(test)]
    pub(crate) fn project_runtime_storage_id_for_test(&self) -> &str {
        self.project.runtime_storage_id_for_test()
    }

    /// The connect-flow state, for ladder assertions (M6).
    pub(crate) fn device_flow_state_for_test(&self) -> &ConnectFlowState {
        self.device.flow_state()
    }

    /// Push a console line into the DEVICE session's buffer, as the live
    /// event sink would (heartbeat-drain tests).
    pub(crate) fn push_device_console_log_for_test(&mut self, draft: UiLogDraft) {
        self.pool
            .device_session_mut()
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
        self.device_state()
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

fn device_section_target(section_id: &'static str) -> UxActivityTarget {
    UxActivityTarget::stack_section(DeviceController::NODE_ID, section_id)
}

fn retarget_activity_updates(updates: UxUpdateSink, target: UxActivityTarget) -> UxUpdateSink {
    UxUpdateSink::new(move |update| match update {
        UxUpdate::Activity {
            status, activity, ..
        } => updates.emit(UxUpdate::Activity {
            target: target.clone(),
            status,
            activity,
        }),
        update => updates.emit(update),
    })
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
        UiViewContent::Stack(stack) => stack
            .sections
            .iter()
            .flat_map(|section| {
                let mut actions = section.actions.clone();
                actions.extend(body_actions(&section.body));
                actions
            })
            .collect(),
        UiViewContent::Empty
        | UiViewContent::Text(_)
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
        UiViewContent::Bus(bus) => bus
            .channels
            .iter()
            .flat_map(|channel| channel.writers.iter().chain(&channel.readers))
            .filter_map(|site| site.focus.clone())
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
    /// The op takes the project WITH it (a destructive wipe): when the
    /// editor lens sits on the managed device, fully quiesce it — detach
    /// the lens so the app returns to the gallery — and say so, rather than
    /// leaving a severed-but-still-open editor. A reset/flash keeps the
    /// project on the device, so its lens only resets its live edit-state
    /// and stays put to reload after the reattach (device-lifecycle P3).
    severs_lens: bool,
}

fn provision_notice(result: &LinkManagementResult) -> UiNotice {
    match result {
        LinkManagementResult::FlashFirmware(result) => {
            UiNotice::info(format!("Flashed {}", result.manifest.display_name))
        }
        _ => UiNotice::info("Firmware flashed"),
    }
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
    use crate::core::status::UiStatusKind;
    use crate::{
        ConnectFlowState, ControllerId, ProjectController, ProjectEditorOp, ProjectEditorTarget,
        ProjectInventorySummary, ProjectNodeAddress, ProjectNodeTarget, ProjectState,
        ProjectSyncPhase, ServerFailureKind, ServerState, StudioServerClient, UiIssue,
    };

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
                    id: "claude-sonnet-5".to_string(),
                    display_name: Some("Claude Sonnet 5".to_string()),
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
                    id: "claude-haiku-4".to_string(),
                    display_name: Some("Claude Haiku 4".to_string()),
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

    #[test]
    fn incompatible_device_surfaces_reflash_affordance_in_the_pane() {
        use lpa_link::{DeviceState, IncompatibleReason};

        let mut studio = connected_studio();
        studio.set_stub_device_for_test(DeviceState::Incompatible {
            reason: IncompatibleReason::NoHello,
        });

        let view = studio.view();
        let device_pane = view
            .panes
            .iter()
            .find(|pane| pane.node_id.as_str() == DeviceController::NODE_ID)
            .expect("device pane");
        assert_eq!(device_pane.status.kind, UiStatusKind::Attention);
        assert_eq!(device_pane.status.label, "Reflash needed");
        // The ONE affordance: reflash (explicit, never automatic).
        let actions = view_actions(&view);
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware { setup_name: None })
        )));
        // The device section explains the incompatibility as an issue.
        let UiViewContent::Stack(stack) = &device_pane.body else {
            panic!("device pane renders a stack");
        };
        let device_section = stack
            .sections
            .iter()
            .find(|section| section.id == DeviceController::SECTION_DEVICE)
            .expect("device section");
        assert!(matches!(
            &device_section.body,
            UiViewContent::Issue(issue) if issue.message.contains("reflash the firmware")
        ));
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
            block_on_ready(studio.dispatch(home_action(HomeOp::CreateProject)))
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
    fn no_firmware_marks_the_device_pane_ready_to_flash() {
        // an open project whose device link answers without firmware:
        // the pane escalates to the flash affordance (the dialog's Blank
        // state is the full wizard; the pane mirrors the status)
        let mut studio = connected_studio();
        studio.set_server_state_for_test(ServerState::Failed {
            issue: UiIssue::new("No LightPlayer firmware detected."),
            kind: ServerFailureKind::NoFirmware,
        });

        let view = studio.view();
        let device_pane = view
            .panes
            .iter()
            .find(|pane| pane.node_id.as_str() == DeviceController::NODE_ID)
            .expect("device pane");
        assert_eq!(device_pane.status.kind, UiStatusKind::Attention);
        assert_eq!(device_pane.status.label, "Ready to flash");
        let actions = view_actions(&view);
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware { setup_name: None })
        )));
    }

    #[test]
    fn loaded_project_gets_project_pane() {
        let studio = connected_studio();

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(view.panes.len(), 3);
        assert_eq!(view.panes[0].node_id.as_str(), ProjectController::NODE_ID);
        assert_eq!(view.panes[1].node_id.as_str(), "bus");
        assert_eq!(view.panes[2].node_id.as_str(), DeviceController::NODE_ID);
        // D23: the pane is about hardware — one device section plus the
        // visually separate firmware section; the wizard steps are gone
        assert_eq!(device_section_ids(&view), vec!["device", "firmware"]);
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::ConnectRunningProject | ProjectOp::LoadDemoProject)
        )));
        // the pane keeps only the session verb (M8′: pushing lives on
        // the cards)
        assert!(
            actions.iter().any(|action| matches!(
                action.op_as::<DeviceOp>(),
                Some(DeviceOp::DisconnectDevice)
            ))
        );
    }

    #[test]
    fn device_pane_offers_firmware_ops_separately() {
        let studio = connected_studio();

        let actions = view_actions(&studio.view());

        // firmware ops live in their own section (D15), away from deploy
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware { setup_name: None })
        )));
        assert!(
            actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetToBlank)))
        );
        // the wizard's connect plumbing is gone
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ConnectLightPlayer | DeviceOp::OpenProvider { .. })
        )));
    }

    #[test]
    fn loaded_project_keeps_management_recovery_actions_visible() {
        let studio = connected_studio();

        let actions = view_actions(&studio.view());
        // recovery stays reachable from the editor's firmware section
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware { setup_name: None })
        )));
        assert!(
            actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetToBlank)))
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

        block_on_ready(studio.disconnect_lightplayer()).unwrap();

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

        block_on_ready(studio.disconnect_device()).unwrap();

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
            DeviceOp::DisconnectDevice,
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
            ProjectNodeAddress::new(TreePath::parse("/demo.project/orbit.shader").unwrap()),
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

    #[test]
    fn failed_link_dispatch_emits_final_failed_view_after_activity() {
        let mut studio = StudioController::with_link_registry_for_test(
            || 0.0,
            registry_with_fake_connect_error("Failed to open serial port."),
        );
        let updates = Rc::new(RefCell::new(Vec::new()));
        let sink = UxUpdateSink::new({
            let updates = Rc::clone(&updates);
            move |update| {
                updates.borrow_mut().push(update);
            }
        });
        let action = UiAction::from_op(
            ControllerId::new(DeviceController::NODE_ID),
            DeviceOp::ConnectEndpoint {
                provider_id: LinkProviderKind::Fake,
                endpoint_id: LinkEndpointId::new("fake-runtime"),
            },
        );

        let result = drive(studio.dispatch_with_updates(action, sink));

        // M6: the ladder ends SOFT — no error, no toast, no issue chip.
        // The honest ending is the card's Not-responding state.
        let notices = result.expect("ladder endings are soft");
        assert!(notices.notices.is_empty(), "no toast from the ladder");
        assert!(updates.borrow().iter().any(|update| {
            matches!(
                update,
                UxUpdate::Activity {
                    target: UxActivityTarget::StackSection {
                        pane_node_id,
                        section_id,
                    },
                    activity,
                    ..
                } if pane_node_id.as_str() == DeviceController::NODE_ID
                    && section_id == DeviceController::SECTION_DEVICE
                    && activity.title == "Opening device session"
            )
        }));
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

    #[test]
    fn retarget_activity_updates_rewrites_activity_target() {
        let updates = Rc::new(RefCell::new(Vec::new()));
        let sink = UxUpdateSink::new({
            let updates = Rc::clone(&updates);
            move |update| {
                updates.borrow_mut().push(update);
            }
        });
        let target = UxActivityTarget::stack_section(
            DeviceController::NODE_ID,
            DeviceController::SECTION_DEVICE,
        );
        let retargeted = retarget_activity_updates(sink, target.clone());

        retargeted.emit(UxUpdate::Activity {
            target: UxActivityTarget::pane("studio|server"),
            status: UiStatus::working("Connecting"),
            activity: UiActivityView::new("Connecting ESP32 server"),
        });

        assert!(matches!(
            updates.borrow().as_slice(),
            [UxUpdate::Activity {
                target: actual_target,
                ..
            }] if *actual_target == target
        ));
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
        let path = TreePath::parse("/demo.project/orbit.shader").unwrap();
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

    fn device_section_ids(view: &UiStudioView) -> Vec<&str> {
        let device_pane = view
            .panes
            .iter()
            .find(|pane| pane.node_id.as_str() == DeviceController::NODE_ID)
            .expect("device pane should exist");
        let UiViewContent::Stack(stack) = &device_pane.body else {
            panic!("device pane should render stack");
        };
        stack
            .sections
            .iter()
            .map(|section| section.id.as_str())
            .collect()
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
                                }),
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
}
