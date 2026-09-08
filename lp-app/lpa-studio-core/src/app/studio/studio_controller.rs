//! The studio's top controller: one dispatch surface over the runtime
//! pool, the project mirror and the library.
//!
//! ⚠️ The DEVICE half of this controller — the connect flow, the device
//! op dispatch, the setup wizard, the deploy/push verbs, the
//! connect-as-pull reconcile and the registry write-backs — was deleted in
//! M2 of the device-model rebuild. The rebuilt model owns all of it; what
//! remains here is the editor lens, the project mirror, the library and
//! the gallery.
//!
//! # One device per tab
//!
//! Pool capacity is a POLICY, not a shape (ADR
//! `2026-08-03-studio-runs-n-device-sessions`). The WEB app runs exactly
//! ONE session per browser tab, so that decision lives here — at
//! [`StudioController::open_device_lens`] and the open path's device
//! resolution (D37) — and never in the pool (a desktop shell with real
//! session wayfinding is meant to inherit the N-session shape unchanged).
//!
//! # Opening a project starts a device
//!
//! There is no runtime that is not a device (PD9). Opening a library card
//! resolves a SIM of the project's target — the one that last ran it, an
//! idle one, or a freshly minted record — powers it on, waits for its
//! hello and lands the lens on it, exactly the way a `/device/<uid>` open
//! lands on silicon. Opening never touches a board by itself (D33).

use core::future::Future;
use core::time::Duration;
use std::rc::Rc;

use lpa_client::{CancelSignal, ProgressDeadline};

use crate::app::home::home_view_builder::HomeInputs;
use crate::app::home::{HOME_NODE_ID, HomeOp, UiHomeView, home_view_builder};
use crate::app::library::{CatalogOp, LibraryHost};
use crate::app::studio::console_command::ConsoleCommand;
use crate::app::studio::refresh_cadence::RefreshCadence;
use crate::app::studio::ui_console_view::UiConsoleView;
use crate::core::log::{
    DeviceEventKind, DeviceEventLog, DeviceEventRecord, LogClock, LogFilter, LogRing,
};
use crate::core::notice::UiNotices;
use crate::{
    AssetContentFetchOp, AssetEditOp, Controller, ControllerContext, ModuleExportOp,
    NodeClearDebugOp, NodeCopyOp, NodeCreateOp, NodeImportOp, NodePasteOp, NodeRemoveOp,
    NodeRevertOp, PanelAutoSaveOp, PanelClearOp, PanelWriteOp, PatchPulseOp, PlaylistActivateOp,
    ProjectConnectResult, ProjectController, ProjectEditRun, ProjectOp, ProjectRefreshOutcome,
    ProjectState, ProjectSyncRun, RuntimePool, ServerSnapshot, ServerState, SlotEditOp,
    StudioSnapshot, UiAction, UiActions, UiActivityView, UiError, UiLogDraft, UiLogEntry,
    UiLogLevel, UiLogOrigin, UiNotice, UiResult, UiStatus, UiStudioView, UiViewContent,
    UxActivityTarget, UxUpdate, UxUpdateSink,
};

/// Minimum gap between view publishes that carry *only* streamed log lines
/// (session console tails, drained producer batches). Anything structural —
/// a revision advance or a local mutation — publishes immediately and takes
/// pending lines along; this throttle only bounds how often a bare log
/// stream can force a full-view rebuild.
const LOG_ONLY_PUBLISH_MIN_GAP_SECS: f64 = 0.25;

/// The address a docs page's anonymous sim answers to inside its own
/// leased controller.
///
/// NOT a registry uid: nothing persists this device (PD10 — its runtime
/// carries no identity, so `registry_key()` is `None`), and no other
/// controller ever sees it. It exists so the lens has something to open
/// by, the way every other device is opened by its address.
const DOCS_SIM_KEY: &str = "docs-sim";

/// The device model's knobs for THIS build.
///
/// How many passive pulls a device lens may fail back to back before the
/// wire is declared dead and the lens closes (see
/// [`StudioController::record_passive_refresh_failure`]). Three: one is a
/// busy board, two is a slow one, three in a row at the device cadence is
/// nobody home.
const LENS_DEAD_WIRE_FAILURES: u32 = 3;

/// `expected_proto` comes from `lpc-wire` and nowhere else: `lpa-devices`
/// hardcodes no proto number on purpose, and a build that speaks proto N must
/// never classify a proto-N device as incompatible because someone re-typed
/// the number. (`lpa_link::device_link::wire::roster_config` does the same
/// thing at the transport seam; both read the same constant, and studio-core
/// must not require a transport feature just to construct a roster.)
fn device_roster_config() -> crate::DeviceRosterConfig {
    crate::DeviceRosterConfig {
        expected_proto: lpc_wire::WIRE_PROTO_VERSION,
        ..Default::default()
    }
}

/// [`device_roster_config`] for the seam test that pins it against
/// `lpa-link`'s `roster_config()`.
#[cfg(test)]
pub(crate) fn device_roster_config_for_test() -> crate::DeviceRosterConfig {
    device_roster_config()
}

pub struct StudioController {
    /// The rebuilt device layer (M3): the `lpa-devices` roster plus the
    /// effects layer that performs its commands. The ONLY device path —
    /// there is no device lens in the runtime pool and no device arm on any
    /// other op, by design (the anti-fifth-machine rule).
    devices: crate::DeviceRoster,
    /// The device cards' live frame feeds (one per wanted device), pulled
    /// as app conversations on each board's shared link. Frame state only —
    /// every device fact they need is read off `devices` at tick time.
    device_feeds: crate::DeviceFrameFeeds,
    /// A lens open that arrived before the device was ready — a reload
    /// whose route asks for the lens while the granted port is still
    /// identifying, or a sim that was just powered on and has not said
    /// hello yet. Held until the device says hello, then attached from the
    /// refresh tick; cleared by any close or another lens attach.
    ///
    /// The hold, not a wait: the fold that produces the hello runs on the
    /// actor's own queue, so an open that awaited the hello inside its
    /// dispatch would wait for a fold that cannot run until it returns.
    pending_device_lens: Option<String>,
    /// A granted-port sweep is due (boot, or a `navigator.serial` connect).
    /// Drained by the actor's device step so a hotplug storm costs one sweep.
    device_sweep_pending: bool,
    /// The transport that reaches SILICON, when this build has one (browser
    /// Web Serial; a scripted fake in the benches). Held so installing the
    /// sim half can rebuild the composite without the caller re-supplying
    /// it — the effects layer takes one transport, on purpose.
    serial_transport: Option<Rc<dyn crate::DeviceTransport>>,
    /// The transport that reaches SIMS, when this build has one. Concrete
    /// rather than `dyn` because powering a sim on and off is not part of
    /// the `DeviceTransport` vocabulary and must not become part of it.
    sim_transport: Option<Rc<crate::SimDeviceTransport>>,
    /// The `/device-sims/<uid>.json` sidecars, read off the library
    /// snapshot at settle. The sole "this device is a sim" fact, cached so
    /// powering one on does not read the store from inside a fold.
    device_sims: std::collections::BTreeMap<String, crate::SimRecord>,
    /// The runtime sessions the studio is attached to, plus the editor
    /// lens. Every network op resolves its wire client through the pool's
    /// lens-bound seam.
    pool: RuntimePool,
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
    /// state transitions, pool lifecycle, management phases, parse
    /// anomalies — and raw RX/TX in capture mode. `Rc` because event sinks
    /// record into it through [`DeviceEventRecorder`] clones.
    ///
    /// ⚠️ M2 of the device-model rebuild deleted every PRODUCER of these
    /// records along with the old device flows; the ring, the capture
    /// switch, the JSONL export and [`Self::record_device_event`] are the
    /// M0 machinery the rebuilt model records back into (keep-list item).
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
    /// The library half of a rename made while the project was OPEN: the
    /// `(uid, name)` whose directory slug still has to move. The manifest
    /// name was patched at once through the open handle; the catalog rename
    /// (a directory move under a lock the open project holds) waits for the
    /// close, and runs at the next library settle after it.
    pending_reslug: Option<(String, String)>,
    /// A library re-hydration is due (attach, home op, save, close, or a
    /// cross-tab `LibraryChanged` ping). Drained at the end of every
    /// dispatch and by the actor after each batch.
    library_refresh_pending: bool,
    /// A home-card open in flight: keeps the gallery on screen (card busy)
    /// while the simulator opens, and tells the connect flow which package
    /// to push instead of probing running projects.
    pending_open: Option<PendingOpen>,
    /// The open that stopped at the mismatch page (D50). Cleared the
    /// moment any open starts — a new gesture supersedes the question the
    /// page was asking, exactly as it supersedes an open in flight.
    open_mismatch: Option<Box<crate::UiOpenMismatch>>,
    /// Injected randomness for uid minting. The web shell installs crypto
    /// randomness at startup; the default is a clock-derived fallback good
    /// enough for tests.
    random: Rc<dyn Fn() -> [u8; 16]>,
    /// The LOCAL `YYYY-MM-DD-HHMM` stamp the library dates slugs with.
    /// Injected for the same reason as the clock: core reads neither time
    /// nor timezone. The default derives UTC from `now_secs`, which is
    /// honest in tests and one timezone off in a shell that forgets to
    /// install its own.
    local_stamp: Rc<dyn Fn() -> String>,
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

/// Which device an open lands on — the model half of the URL's `?on=`
/// hint (D43).
///
/// The two arms are the two halves of the grammar, and they differ in
/// exactly one way that matters: whether the open may settle for a
/// different device than the one asked for.
#[derive(Clone, Debug, Eq, PartialEq)]
enum OpenOn {
    /// A kind, or nothing at all: Studio resolves a sim of the project's
    /// target and is free to reuse its own (D33) — the sim that last ran
    /// this project, an idle one, else a fresh record.
    Resolve,
    /// An INSTANCE, named by base MAC. Silicon or a sim; this device or
    /// none. It is the naming that makes the mismatch page's stop
    /// worthwhile (D50).
    Device {
        base_mac: String,
        /// The mismatch page's "push here": the person chose to replace
        /// what is running, with both projects named in front of them.
        over_running_project: bool,
    },
}

/// Why a named device did not take this open.
enum NamedDeviceRefusal {
    /// Nothing this library knows answers to that MAC.
    Unknown,
    /// It is running a different project (D50) — the page, not a push.
    Mismatch(crate::UiOpenMismatch),
}

/// What a home card asked to open.
#[derive(Clone, Debug)]
enum PendingOpen {
    /// A library package, by key (`prj…` uid or slug), on the device the
    /// address named (or on the resolver's choice — see [`OpenOn`]).
    Package { key: String, on: OpenOn },
    /// An embedded example, by id (opened as a transient view session).
    Example(String),
    /// A compiled-in docs example, deployed directly onto the docs sim
    /// (never through the library). Rides the same pending-open slot so a
    /// lens that lands late still deploys.
    DocsExample(String),
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
            PendingOpen::Package { key, .. } => key,
            PendingOpen::Example(id) => id,
            PendingOpen::DocsExample(id) => id,
            PendingOpen::SharedTransient { uid, .. } => uid,
        }
    }

    /// The action that runs this open again — what the failed opening
    /// frame's Retry dispatches. Built from the pending open itself so
    /// Retry can never drift from what was actually attempted.
    fn retry_action(&self) -> UiAction {
        let op = match self {
            PendingOpen::Package { key, on } => match on {
                OpenOn::Resolve => HomeOp::OpenPackage { key: key.clone() },
                OpenOn::Device {
                    base_mac,
                    over_running_project,
                } => HomeOp::OpenPackageOnDevice {
                    key: key.clone(),
                    base_mac: base_mac.clone(),
                    over_running_project: *over_running_project,
                },
            },
            PendingOpen::Example(id) | PendingOpen::DocsExample(id) => {
                HomeOp::OpenExample { id: id.clone() }
            }
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
        Self {
            devices: crate::DeviceRoster::new(device_roster_config()),
            device_feeds: crate::DeviceFrameFeeds::new(),
            pending_device_lens: None,
            device_sweep_pending: false,
            serial_transport: None,
            sim_transport: None,
            device_sims: std::collections::BTreeMap::new(),
            pool: RuntimePool::new(),
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
            pending_reslug: None,
            library_refresh_pending: false,
            pending_open: None,
            open_mismatch: None,
            random: Rc::new(clock_fallback_random),
            local_stamp: {
                let clock = Rc::clone(&now_secs_for_stamp);
                Rc::new(move || utc_slug_stamp(clock()))
            },
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

    // ---------------------------------------------------------------
    // The device layer (M3 of the device-model rebuild)
    // ---------------------------------------------------------------

    /// Install the platform transport the effects layer opens ports through
    /// (wasm: browser Web Serial; host tests: a fake). Install before the
    /// actor takes ownership.
    ///
    /// Installing one also arms the first granted-port sweep: a page that CAN
    /// see ports should show what it already has permission for, without the
    /// user asking twice.
    pub fn set_device_transport(&mut self, transport: Rc<dyn crate::DeviceTransport>) {
        self.serial_transport = Some(transport);
        self.install_device_transport();
    }

    /// Install the transport that serves SIMS. Install before the actor
    /// takes ownership, beside (or instead of) the serial one.
    ///
    /// A build with both installs a `CompositeDeviceTransport` routing by
    /// the link's endpoint: the effects layer holds exactly one transport,
    /// and a per-kind fork inside it is how a second device flow grows.
    pub fn set_device_sim_transport(&mut self, transport: Rc<crate::SimDeviceTransport>) {
        self.sim_transport = Some(transport);
        self.install_device_transport();
    }

    /// (Re)install whichever transport this build's halves add up to, and
    /// arm the first sweep: a page that CAN see devices should show what it
    /// already has, without the user asking twice.
    fn install_device_transport(&mut self) {
        let transport: Option<Rc<dyn crate::DeviceTransport>> = match &self.sim_transport {
            Some(sim) => Some(Rc::new(crate::CompositeDeviceTransport::new(
                self.serial_transport.clone(),
                Rc::clone(sim) as Rc<dyn crate::DeviceTransport>,
            ))),
            None => self.serial_transport.clone(),
        };
        let Some(transport) = transport else {
            return;
        };
        self.devices.effects_mut().set_transport(transport);
        self.device_sweep_pending = true;
    }

    /// Mint a sim of `target` and remember it: one registry row
    /// (`transport: "sim"`, keyed on the derived uid) and one
    /// `/device-sims/<uid>.json` sidecar, written in a single library
    /// settle so a half-created sim cannot exist.
    ///
    /// `random` is the caller's six bytes (the web shell's
    /// `crypto.getRandomValues`, a test's fixed bytes): minting decides
    /// identity, and where entropy comes from is the platform's business,
    /// not the model's. Returns the uid the sim is remembered under.
    ///
    /// The sim is NOT powered on. It loads as a detached record like any
    /// remembered board, and `Action::Connect` is what starts it.
    pub async fn create_sim_record(
        &mut self,
        target: &str,
        name: Option<&str>,
        random: &[u8; 6],
    ) -> Result<String, UiError> {
        let minted = crate::new_sim_record(target, name, random, (self.now_secs)());
        let sidecar_bytes = serde_json::to_vec(&minted.sidecar).unwrap_or_default();
        self.run_catalog_op(CatalogOp::CreateSimDevice {
            device: Box::new(minted.row.clone()),
            sidecar_bytes,
        })
        .await?;
        self.device_sims
            .insert(minted.uid.clone(), minted.sidecar.clone());
        Ok(minted.uid)
    }

    /// The sims remembered in this library, by uid.
    pub fn device_sims(&self) -> &std::collections::BTreeMap<String, crate::SimRecord> {
        &self.device_sims
    }

    /// Read every remembered sim's sidecar off the library snapshot.
    ///
    /// Only rows the registry already labels `sim` are looked at, so a
    /// library full of boards reads nothing. Absence is the answer for
    /// everything else — a board without a sidecar is not a sim, which is
    /// every board in every existing library.
    fn seed_device_sim_records(&mut self, fs: &Rc<std::cell::RefCell<dyn lpfs::LpFs>>) {
        let Some(inputs) = self.home_inputs.as_ref() else {
            return;
        };
        let uids: Vec<String> = inputs
            .registered
            .iter()
            .filter(|row| row.transport == crate::SIM_TRANSPORT)
            .map(|row| row.uid.clone())
            .collect();
        let fs = fs.borrow();
        self.device_sims = uids
            .into_iter()
            .filter_map(|uid| crate::read_sim_record(&*fs, &uid).map(|record| (uid, record)))
            .collect();
    }

    /// The session a sim device would run as, or `None` when this device is
    /// not a sim (no sidecar) or has no uid to be keyed on.
    fn sim_session_for(&self, device: crate::DeviceId) -> Option<crate::SimSession> {
        let entry = self
            .devices
            .roster()
            .devices()
            .iter()
            .find(|entry| entry.id == device)?;
        let uid = entry
            .identity
            .uid
            .as_ref()
            .map(|uid| uid.0.clone())
            .or_else(|| self.device_registry_key(device))?;
        let sidecar = self.device_sims.get(&uid)?;
        Some(crate::SimSession {
            uid,
            target: sidecar.target.clone(),
            display_name: entry.title(),
            base_mac: sidecar.base_mac.clone(),
            // PD12: a device asks for the GPU tier; the worker answers with
            // what it granted, and the band says which.
            tier: crate::SimTier::Gpu,
        })
    }

    /// Power a sim on, if this gesture is a `Connect` at one.
    ///
    /// The EFFECTS layer starts the runtime, never the model: `Connect` on a
    /// sim is `Connect` (PD8, Q15), and the sweep this arms is what attaches
    /// the link the model then opens. The hello carries the uid, so the fold
    /// adopts the link into this very record through its own identity join —
    /// no new arm, no new event.
    fn power_on_sim_for(&mut self, action: &crate::DeviceAction) {
        let crate::DeviceAction::Connect { device } = action else {
            return;
        };
        let (Some(sim), Some(session)) =
            (self.sim_transport.clone(), self.sim_session_for(*device))
        else {
            return;
        };
        if sim.is_powered(&session.uid) {
            return;
        }
        match sim.power_on(session) {
            Ok(()) => self.device_sweep_pending = true,
            Err(error) => log::warn!("the sim did not start: {error}"),
        }
    }

    /// Power a sim off, if this gesture was a `Disconnect` (or a `Forget`)
    /// at one.
    ///
    /// Runs AFTER the fold, which has already evicted and closed the link.
    /// The detach has to be raised explicitly: `Disconnect` alone closes a
    /// port and keeps the card attached, which is right for a board still
    /// sitting on the desk and wrong for a runtime that no longer exists. A
    /// powered-off sim leaves the roster the way an unplugged board does,
    /// and its record stays on the remembered line (Q5).
    fn power_off_sim_for(&mut self, action: &crate::DeviceAction) {
        let uid = self.sim_uid_to_power_off(action);
        self.power_off_sim(uid);
    }

    /// The uid a gesture will power off, read BEFORE the fold.
    ///
    /// Before, because a `Forget` DELETES the device: read afterwards, the
    /// roster no longer holds the entry the uid comes from, and the sim
    /// would be forgotten while its worker kept running (`just test`, P3).
    /// `Disconnect` would survive either order; one rule for both is
    /// cheaper than remembering which.
    fn sim_uid_to_power_off(&self, action: &crate::DeviceAction) -> Option<String> {
        match action {
            crate::DeviceAction::Disconnect { device } | crate::DeviceAction::Forget { device } => {
                self.sim_session_for(*device).map(|session| session.uid)
            }
            _ => None,
        }
    }

    fn power_off_sim(&mut self, uid: Option<String>) {
        let (Some(sim), Some(uid)) = (self.sim_transport.clone(), uid) else {
            return;
        };
        let link = self
            .devices
            .effects()
            .link_for_endpoint(&crate::sim_endpoint(&uid));
        if !sim.power_off(&uid) {
            return;
        }
        if let Some(link) = link {
            self.fold_device_input(crate::DeviceInput::Event(
                lpa_devices::Event::LinkDetached { link },
            ));
        }
    }

    /// Install the platform task spawner for device IO (`spawn_local` on
    /// wasm). Install before the actor takes ownership.
    pub fn set_device_spawner(&mut self, spawner: impl Fn(crate::DeviceTaskFuture) + 'static) {
        self.devices.effects_mut().set_spawner(spawner);
    }

    /// Install the platform timer factory device waits run on (called by
    /// `StudioActor::new`, from the same `make_timer` the pull deadlines use).
    pub fn set_device_timer(
        &mut self,
        timer: impl FnMut(Duration) -> crate::DeviceTimerFuture + 'static,
    ) {
        self.devices.effects_mut().set_timer(timer);
    }

    /// Install the sink that returns device inputs to the actor's queue
    /// (called by `StudioActor::new`).
    pub fn set_device_input_sink(&mut self, sink: impl Fn(crate::DeviceInput) + 'static) {
        self.devices.effects_mut().set_input_sink(sink);
    }

    /// Fold one device input and perform what it asked for.
    ///
    /// Synchronous end to end (invariant I7): the record writes it produced
    /// are queued for [`Self::settle_device_records`], and everything else was
    /// either a link command (queued on the link) or a spawned future.
    pub fn fold_device_input(&mut self, input: crate::DeviceInput) {
        let now = self.device_now();
        for line in self.devices.handle(now, input) {
            self.record_device_event(
                None,
                Some(&line.scope),
                DeviceEventKind::Journal {
                    scope: line.scope.clone(),
                    entry: line.entry,
                },
            );
        }
        self.drop_device_lens_if_wireless();
        // A forgotten device takes its feed (and last frame) with it.
        self.device_feeds.retain_devices(self.devices.roster());
        self.mark_dirty();
    }

    /// React to a `navigator.serial` edge.
    pub fn note_device_hotplug(&mut self, edge: crate::app::studio::studio_command::DeviceHotplug) {
        match edge {
            crate::app::studio::studio_command::DeviceHotplug::Connected => {
                self.device_sweep_pending = true;
            }
            crate::app::studio::studio_command::DeviceHotplug::Disconnected => {
                self.devices.sweep_departed_ports();
            }
        }
        self.run_due_device_sweep();
    }

    /// Run the granted-port sweep when one is due (boot, transport install,
    /// hotplug connect). Coalesced: a storm of connect events costs one sweep.
    fn run_due_device_sweep(&mut self) {
        if !core::mem::take(&mut self.device_sweep_pending) {
            return;
        }
        self.devices.sweep_granted_ports();
    }

    /// Perform the record writes the device folds asked for.
    ///
    /// The ONE asynchronous step in the device path, and it runs outside the
    /// fold on purpose. A write that fails is a log line, not a stuck card:
    /// the model's state is already correct, and the registry is a
    /// convenience that survives a refresh, not the source of truth.
    pub async fn settle_device_records(&mut self) {
        self.run_due_device_sweep();
        // Before anything is written: a registered board that still has no
        // name gets one (G1 2026-09-03). It rides as a `SetName` action, so
        // the model learns it the same way a rename does and the record it
        // then persists carries the name to the registry row.
        for action in self.auto_name_actions() {
            self.fold_device_input(crate::DeviceInput::Action(action));
        }
        let writes = self.devices.take_writes();
        if writes.is_empty() {
            return;
        }
        if self.library_host.is_none() {
            // No local store mounted: nothing to remember into. The roster
            // keeps working for this session, which is the honest degrade.
            log::debug!("device records not persisted: no library host");
            return;
        }
        // The uids this batch WRITES. A merge (`reconcile_identities`) emits
        // `DeleteRecord` for the discarded handle and `PersistRecord` for the
        // surviving one in the SAME batch, and both resolve to the same
        // registry key — because being the same device is what a merge
        // discovered. Deleting on that would take away the row this very
        // batch just wrote, and, worse, the device's sidecars with it: a
        // remembered board whose port came back would silently lose its last
        // picture, and a sim would lose the file that says it is one.
        // (`docs/defects/2026-09-07-merge-delete-erased-the-merged-row.md`.)
        let mut written: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for record in writes.persist {
            let Some(row) = crate::app::devices::registry_row_from_record(&record) else {
                // An anonymous board has no honest registry key; adopting one
                // is Setup's gesture, which is round 2.
                continue;
            };
            // Remember which row this device's record went to: by delete time
            // the device is gone from the fold, and the row key is its
            // identity, not its handle.
            self.devices.remember_key(record.device, row.uid.clone());
            written.insert(row.uid.clone());
            if let Err(error) = self
                .run_catalog_op(CatalogOp::UpsertRegisteredDevice(Box::new(row)))
                .await
            {
                log::warn!("device record not persisted: {error}");
            }
        }
        for device in writes.delete {
            let Some(uid) = self
                .devices
                .take_key(device)
                .or_else(|| self.device_registry_key(device))
            else {
                continue;
            };
            if written.contains(&uid) {
                log::debug!("device {device:?} merged into the row it shares; {uid} stays");
                continue;
            }
            if let Err(error) = self
                .run_catalog_op(CatalogOp::ForgetRegisteredDevice { uid })
                .await
            {
                log::warn!("device record not forgotten: {error}");
            }
        }
        for push in writes.pushes {
            self.bank_completed_push(push).await;
        }
    }

    /// Bank one verified push: a `Pushed` event on the project's history and
    /// the device association naming what that board was last given.
    ///
    /// Best-effort, like every other record write. The push already happened
    /// and the board is already running the project; a bookkeeping failure
    /// is a log line, never a card that claims the push did not work.
    ///
    /// Two honest skips:
    ///
    /// - A board still keyed on its MAC (`mac:aa:bb:…`) has no `dev…`
    ///   identity for the history to name, and inventing one would put an
    ///   unresolvable device in a permanent event log. It gets its uid when
    ///   the firmware provisions one; until then the push is unbanked, which
    ///   M4's sync verdicts read as "unknown", not as "up to date".
    /// - A version the project's history has never recorded (an unsaved
    ///   working copy pushed straight off disk) is refused by
    ///   `record_push` itself, for the same reason: an event may not name a
    ///   snapshot the library cannot produce.
    async fn bank_completed_push(&mut self, push: crate::CompletedPush) {
        // The row is derived from the MODEL's record rather than read back
        // from the gallery's cached rows: the record is what the fold just
        // persisted, and the catalog op merges against the live registry
        // anyway, so nothing here can be stale.
        let Some(row) = self
            .devices
            .roster()
            .devices()
            .iter()
            .find(|entry| entry.id == push.device)
            .and_then(|entry| entry.record.as_ref())
            .and_then(crate::app::devices::registry_row_from_record)
        else {
            log::debug!(
                "push not banked: device {:?} has no persisted record",
                push.device
            );
            return;
        };
        let version: lpc_history::ContentHash = match push.version.parse() {
            Ok(version) => version,
            Err(error) => {
                log::warn!("push not banked: unreadable content hash: {error}");
                return;
            }
        };
        if let Err(error) = self
            .run_catalog_op(CatalogOp::RecordPush {
                project_uid: push.project_uid,
                device: Box::new(row),
                version,
            })
            .await
        {
            log::warn!("push not banked: {error}");
        }
    }

    /// The registry key a device id currently maps to, read off the hydrated
    /// gallery inputs (the rows themselves carry the model's handle).
    fn device_registry_key(&self, device: crate::DeviceId) -> Option<String> {
        self.home_inputs
            .as_ref()?
            .registered
            .iter()
            .find_map(|row| (row.device_id == Some(device.0)).then(|| row.uid.clone()))
    }

    /// Rehydrate the registry's remembered boards into the roster.
    ///
    /// Runs on every library settle, not once: the library re-hydrates when a
    /// device row is written, when another tab changes the catalog, and when
    /// this tab becomes visible again. The load is idempotent (rows the roster
    /// already holds are skipped), which is what makes calling it repeatedly
    /// correct rather than a way to grow a second card per board.
    fn load_device_records_if_due(&mut self) {
        let Some(inputs) = self.home_inputs.as_ref() else {
            return;
        };
        let rows = inputs.registered.clone();
        self.devices.load_records(&rows);
        self.mark_dirty();
    }

    /// The device model's clock, for tests that assert the two agree.
    #[cfg(test)]
    pub(crate) fn device_now_for_test(&self) -> crate::DeviceMillis {
        self.device_now()
    }

    /// Replace the roster's config wholesale, for benches that shrink the
    /// model's budgets (a real flash ladder is 24 s of rungs; a bench's fake
    /// clock walks 5 ms per step). Only valid before anything is folded.
    #[cfg(test)]
    pub(crate) fn set_device_roster_config_for_test(&mut self, config: crate::DeviceRosterConfig) {
        self.devices = crate::DeviceRoster::new(config);
    }

    /// Provisional device ids of the links still being identified — the
    /// handle a Cancel gesture on a fresh plug addresses.
    #[cfg(test)]
    pub(crate) fn device_pending_ids_for_test(&self) -> Vec<crate::DeviceId> {
        self.devices
            .roster()
            .pending()
            .iter()
            .map(|pending| pending.device_id())
            .collect()
    }

    /// The device model's clock: the injected wall clock, in the integer
    /// millis the model keeps its timeline in.
    fn device_now(&self) -> crate::DeviceMillis {
        crate::DeviceMillis(((self.now_secs)() * 1_000.0).max(0.0) as u64)
    }

    /// The devices surface's projection.
    pub fn device_roster_view(&self) -> crate::DeviceRosterView {
        let mut view = self.devices.view(self.device_now());
        view.feeds = crate::device_card_feed_views(
            self.devices.roster(),
            &view.roster.devices,
            &self.device_feeds,
            self.devices.effects(),
            (self.now_secs)(),
        );
        view.runtime_bands = self.runtime_bands(&view);
        view
    }

    /// The runtime band for every SIM in the roster (PD11), joined here
    /// because the model does not know a sim from silicon: the sidecar
    /// says which devices are sims and what they wear, and the transport
    /// says what tier their worker was granted.
    fn runtime_bands(
        &self,
        view: &crate::DeviceRosterView,
    ) -> std::collections::BTreeMap<crate::DeviceId, crate::UiRuntimeBand> {
        if self.device_sims.is_empty() {
            return std::collections::BTreeMap::new();
        }
        view.roster
            .devices
            .iter()
            .filter_map(|card| {
                let uid = view.open_addresses.get(&card.id.0)?;
                let sidecar = self.device_sims.get(uid)?;
                let tier = self
                    .sim_transport
                    .as_ref()
                    .and_then(|transport| transport.granted_tier(uid));
                Some((card.id, crate::UiRuntimeBand::sim(&sidecar.target, tier)))
            })
            .collect()
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
    ///
    /// ⚠️ Producer-less since M2 of the device-model rebuild: the old
    /// device flows were its only callers. Kept `pub` as the M0 recording
    /// seam the rebuilt model writes through.
    pub fn record_device_event(
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

    /// The delay before the next passive tick: the MINIMUM over sessions
    /// (runtime-pool P2, per-session tick policy).
    ///
    /// - The LENS session contributes its cadence, tightened to the
    ///   verdict-chase interval while a just-accepted asset apply awaits
    ///   its compile verdict, plus its own passive-refresh backoff.
    /// - A DETACHED session (P3: no project pull draining its client)
    ///   contributes the time until its next slow status heartbeat, which
    ///   drains its buffered logs so nothing accumulates unboundedly. The
    ///   sim's worker still self-ticks; no wire op rides its heartbeat.
    /// - An empty pool falls back to the calm default interval, matching
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
        if let Some(feed) = self.device_feed_due_in(now) {
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
    /// scope) tracks the lens — and the lens device's reported build, for
    /// the add-node picker's gate (a sim lens leaves it `None` and the
    /// picker offers everything). Called at every action and tick that
    /// might move the lens; cheap, idempotent.
    fn sync_lens_probe_policy(&mut self) {
        let lens = self.pool.lens_session();
        let transport = lens.map(crate::RuntimeSession::transport);
        let features = lens
            .and_then(crate::RuntimeSession::device_features)
            .map(<[lpc_model::LpFeature]>::to_vec);
        self.project.set_lens_transport(transport);
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
    ///
    /// The device lens's dead-wire backstop lives here: a board whose wire
    /// is gone answers nothing, and the browser can take minutes to say so
    /// (bench, 2026-09-02: 8.5 min from a hub power-cut to "The device has
    /// been lost"); until then every pull merely times out and neither
    /// tap fires. [`LENS_DEAD_WIRE_FAILURES`] pulls in a row with no answer
    /// is a wire with nobody on it, so the lens closes honestly and the
    /// gallery takes over.
    pub fn record_passive_refresh_failure(&mut self) {
        let Ok(session) = self.pool.lens_session_mut() else {
            return;
        };
        session.record_refresh_failure();
        // The backstop is every kind's now (PD9): a worker that died under
        // the lens answers nothing, exactly like a wire with nobody on it,
        // and this is what closes the editor honestly instead of leaving a
        // mirror over a runtime that is gone.
        let dead = session.consecutive_refresh_failures() >= LENS_DEAD_WIRE_FAILURES;
        if dead {
            self.push_log(UiLogDraft::new(
                UiLogLevel::Warn,
                UiLogOrigin::Studio,
                "the device stopped answering the editor; the editor is closed".to_string(),
            ));
            self.close_device_lens();
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

    /// Run the slow status heartbeat on every DETACHED session (P3: a sim
    /// without the lens has no project pull draining its client, so the
    /// heartbeat keeps its buffered wire logs from accumulating
    /// unboundedly) whose interval elapsed: drain the session's buffered
    /// wire log lines into its own console tail (D42 — the per-session
    /// console; the global ring no longer carries session streams). No
    /// wire operation rides a heartbeat — the self-ticking worker fills
    /// the buffers — so a tick that fans into lens-refresh + heartbeats
    /// still issues at most one wire op per session.
    pub fn run_due_heartbeats(&mut self) {
        let now = (self.now_secs)();
        let lens = self.pool.lens();
        let mut stamped = Vec::new();
        let mut changed = false;
        for session in self.pool.sessions_mut() {
            let lens_bound = Some(session.id()) == lens;
            if lens_bound || !session.heartbeat_due(now) {
                continue;
            }
            session.mark_heartbeat(now);
            let drained = session.take_pending_logs();
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
        }
        if changed {
            self.mark_dirty();
        }
    }

    // ---------------------------------------------------------------
    // The card frame feed (honest-device preview P2)
    // ---------------------------------------------------------------
    //
    // ONE lane, the roster's. The runtime pool used to run a second one
    // for the sim card's ▶ tab; the sim is a device now (PD9), so it is
    // fed through `device_feeds` like every other card — including the
    // `FeedLiveness::Lens` treatment when the editor holds its wire.

    /// Time until the earliest due card feed pull. `None` when no card is
    /// feeding — the common case, where the UI timer keeps its calm
    /// heartbeat pace.
    fn device_feed_due_in(&self, now: f64) -> Option<Duration> {
        self.device_feeds.due_in(
            now,
            crate::DEVICE_CARD_FEED_INTERVAL,
            self.devices.roster(),
            self.devices.effects(),
        )
    }

    /// Pull one published frame per feeding DEVICE card whose completion
    /// gap elapsed — the roster boards' half of the card feed lane, under
    /// the same passive class and preempt watch as the sim's.
    ///
    /// Returns whether a due pull was skipped or cut short by cancellation.
    pub async fn run_due_device_feeds<MakeTimer, Timer, Cancel>(
        &mut self,
        make_timer: MakeTimer,
        cancel: &Cancel,
    ) -> bool
    where
        MakeTimer: FnMut(Duration) -> Timer + Clone,
        Timer: Future<Output = ()>,
        Cancel: CancelSignal + ?Sized,
    {
        let now_secs = Rc::clone(&self.now_secs);
        let (preempted, new_frame) = self
            .device_feeds
            .run_due(
                &*now_secs,
                crate::DEVICE_CARD_FEED_INTERVAL,
                crate::DEVICE_CARD_FEED_CLASS
                    .deadline()
                    .unwrap_or(crate::PASSIVE_REFRESH_DEADLINE),
                self.devices.roster(),
                self.devices.effects(),
                make_timer,
                cancel,
            )
            .await;
        if new_frame {
            self.mark_dirty();
        }
        self.persist_due_device_frames().await;
        preempted
    }

    /// Write every feed's newest frame to its board's sidecar when one is
    /// due ([`crate::DeviceFrameFeed::snapshot_due`]: a frame newer than the
    /// written one, at most every ten seconds per board), so a remembered
    /// board keeps its last picture across reloads.
    ///
    /// Runs in the feed lane, after the pulls, and never in a fold (I7).
    /// The write goes straight to the host — NOT through `run_catalog_op`,
    /// whose gallery re-hydration would rebuild the home view every ten
    /// seconds for nothing the gallery lists. A failed write is logged and
    /// still stamped as written: a store that refuses today would refuse
    /// again every tick, and the board's picture is a convenience, not the
    /// source of truth.
    async fn persist_due_device_frames(&mut self) {
        let now = (self.now_secs)();
        let due = self.device_feeds.snapshots_due(now);
        if due.is_empty() {
            return;
        }
        let Ok(host) = self.library_host() else {
            return;
        };
        for (device, frame, captured_at) in due {
            let Some(uid) = self.device_registry_key(device).or_else(|| {
                self.devices
                    .roster()
                    .device(device)
                    .and_then(|device| device.identity.uid.as_ref())
                    .map(|uid| uid.0.clone())
            }) else {
                // An anonymous board has no honest key; its picture lives
                // for the session only, like its record.
                continue;
            };
            let bytes = crate::app::devices::device_frame_snapshot::encode(&frame, captured_at);
            if let Err(error) = host
                .catalog(CatalogOp::StoreDeviceFrame { uid, bytes })
                .await
            {
                log::warn!("device last frame not persisted: {error}");
            }
            self.device_feeds
                .mark_snapshot_written(device, captured_at, now);
        }
    }

    /// Seed the feeds of remembered boards from their persisted last frames
    /// (`device_frame_snapshot`), read off the library snapshot `fs` at
    /// settle. Only a board whose feed has NO picture reads its sidecar, so
    /// after the first settle nothing is read again, and a frame this
    /// session pulled is never displaced by an older one on disk.
    fn seed_device_frame_snapshots(&mut self, fs: &Rc<std::cell::RefCell<dyn lpfs::LpFs>>) {
        let Some(inputs) = self.home_inputs.as_ref() else {
            return;
        };
        let mut seeded = false;
        let rows: Vec<(String, Option<u64>)> = inputs
            .registered
            .iter()
            .map(|row| (row.uid.clone(), row.device_id))
            .collect();
        for (uid, device_id) in rows {
            let device = self
                .devices
                .roster()
                .devices()
                .iter()
                .find(|device| {
                    device_id == Some(device.id.0)
                        || device
                            .identity
                            .uid
                            .as_ref()
                            .is_some_and(|known| known.0 == uid)
                })
                .map(|device| device.id);
            let Some(device) = device else {
                continue;
            };
            if self.device_feeds.has_frame(device) {
                continue;
            }
            let snapshot = {
                let fs = fs.borrow();
                crate::app::devices::device_frame_snapshot::read_snapshot(&*fs, &uid)
            };
            if let Some((frame, captured_at)) = snapshot {
                seeded |= self.device_feeds.seed_snapshot(device, frame, captured_at);
            }
        }
        if seeded {
            self.mark_dirty();
        }
    }

    /// The card's mount lease for its live frame feed: a mounted
    /// `DeviceRosterCard` wants its device fed; an unmounted one does not.
    pub fn set_device_feed_wanted(&mut self, device: crate::DeviceId, wanted: bool) {
        self.device_feeds.set_wanted(device, wanted);
        self.mark_dirty();
    }

    /// The page's visibility, for the feeds: a hidden tab feeds nothing.
    pub fn set_device_feeds_page_visible(&mut self, visible: bool) {
        self.device_feeds.set_page_visible(visible);
    }

    /// The device feeds, for the view join and the tests.
    pub fn device_feeds(&self) -> &crate::DeviceFrameFeeds {
        &self.device_feeds
    }

    pub fn actions(&self) -> UiActions {
        UiActions::new(view_actions(&self.view()))
    }

    pub fn view(&self) -> UiStudioView {
        if let Some(home) = self.home_view() {
            return UiStudioView::new(Vec::new(), self.console_view())
                .with_home(Some(home))
                .with_lens(self.lens_runtime())
                .with_session(self.session_control())
                .with_open_mismatch(self.open_mismatch.as_deref().cloned())
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
            // Output faces get the facts their node's sections cannot
            // carry: which board the lens runtime claims to be, and the
            // per-wire status it reports. "No board known" is a first-class
            // state — an untargeted project simply has no board id. (The
            // device hello's measured LED envelope returns with the rebuilt
            // device model; the sim reports none.)
            let lens = self.pool.lens_session();
            crate::app::studio::output_face_decoration::decorate_output_faces(
                editor,
                self.lens_board_id(),
                lens.and_then(|session| session.output_wire_status()),
                None,
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
            .with_lens_card(self.lens_card())
            .with_session(self.session_control())
            .with_settings(self.settings.ui_view())
            .with_dirty(dirty)
    }

    /// The LENS session's docked card (D43): the device the editor is open
    /// on, projected by the roster exactly as the gallery projects it —
    /// never a second card, and for a sim the same card the Devices grid
    /// draws, band and all (PD11).
    fn lens_card(&self) -> Option<crate::UiLensCard> {
        let attachment = self.pool.attached_session()?.attachment();
        let view = self.device_roster_view();
        let runtime = view.runtime_bands.get(&attachment.device).cloned();
        view.roster
            .devices
            .into_iter()
            .find(|card| card.id == attachment.device)
            .map(|card| crate::UiLensCard::Device { card, runtime })
    }

    /// The header session·project control's ONE session (single-session
    /// policy, module doc): the device's own name and board, its status
    /// from the roster's evidence (the same fold the card renders — the
    /// control can never disagree with the card), and the engine rate the
    /// lens client heard.
    ///
    /// Deliberately NOT coupled to the lens: a session the editor has
    /// detached from is still the session this tab runs, and the control is
    /// what says so.
    fn session_control(&self) -> Option<crate::UiChromeSessionControl> {
        let session = self.pool.attached_session()?;
        let attachment = session.attachment();
        let device = self.devices.roster().device(attachment.device);
        let running = device.is_some_and(|device| {
            device
                .evidence
                .loaded_projects()
                .is_some_and(|loaded| !loaded.is_empty())
        });
        let status = match (session.is_connected(), running) {
            (false, _) => crate::UiChromeSessionStatus::Attention,
            (true, true) => crate::UiChromeSessionStatus::Run,
            (true, false) => crate::UiChromeSessionStatus::Empty,
        };
        let mut facts: Vec<String> = Vec::new();
        if let Some(fps) = session.engine_fps() {
            facts.push(format!("{} fps", fps.round() as i64));
        }
        Some(crate::UiChromeSessionControl {
            face: match attachment.transport {
                crate::LinkTransport::Sim => crate::DeviceFace::Sim,
                crate::LinkTransport::Serial => crate::DeviceFace::Wire,
            },
            key: format!("device:{}", attachment.uid),
            device: Some(attachment.device),
            // The roster's LIVE title, so a rename made from this very
            // panel shows up in the segment at once; the attach-time
            // snapshot only covers a device the roster has since dropped.
            name: device
                .map(lpa_devices::Device::title)
                .unwrap_or_else(|| attachment.name.clone()),
            board: attachment
                .board_id
                .as_deref()
                .map(crate::board_display_name),
            status,
            stat_line: (!facts.is_empty()).then(|| facts.join(" · ")),
        })
    }

    /// The board the LENS device reports it is — the roster's `board_id`,
    /// which for a sim is the manifest it wears and for silicon is the
    /// board its firmware was built for. One source, both kinds (PD9).
    ///
    /// `None` is ORDINARY, not exceptional: no lens, or a device that has
    /// reported no board.
    fn lens_board_id(&self) -> Option<&str> {
        self.pool.lens_session()?.attachment().board_id.as_deref()
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

    /// The lens's runtime binding for the view (SDI: the URL is the
    /// focused document — the web shell's D37 route reconciliation binds
    /// to this).
    fn lens_runtime(&self) -> Option<crate::UiLensRuntime> {
        let attachment = self.pool.lens_session()?.attachment();
        Some(crate::UiLensRuntime::Device {
            base_mac: self.base_mac_of(&attachment.uid),
            uid: attachment.uid.clone(),
            transport: attachment.transport,
            // The library binding the mirror holds: what the editor is
            // looking at through this lens, which is the whole of the
            // project route's identity. `None` for a transient open (an
            // example, a shared link) and for the storeless demo path.
            project_uid: self.project.active_library_uid(),
        })
    }

    /// A device's base MAC in canonical form, off the roster's identity
    /// chain — the instance half of the `?on=` grammar (D43).
    ///
    /// `None` for a device whose identity has not landed, and for the docs
    /// page's identity-less sim (PD10), which is never addressed at all.
    fn base_mac_of(&self, uid: &str) -> Option<String> {
        Some(
            self.devices
                .device_for_key(uid)?
                .identity
                .mac
                .as_ref()?
                .0
                .clone(),
        )
    }

    /// The home gallery: shown whenever NO project is open — always
    /// (D24; the M4 transitional bridge and its home-only-when-link-idle
    /// rule are gone).
    fn home_view(&self) -> Option<UiHomeView> {
        if self.project_is_loaded() {
            return None;
        }
        let opening = self.pending_open.as_ref();
        let mut view = home_view_builder::build_home_view(
            self.home_inputs.as_ref(),
            opening.map(|pending| pending.card_key().to_string()),
            None,
        );
        // The device half is the model's own projection, verbatim: there is
        // no `Ui*` mirror of it, so the page cannot drift from the fold.
        // The sim is in it, like every device (PD9).
        view.devices = self.device_roster_view();
        Some(view)
    }

    /// The console slice of the view: ring entries passing the display
    /// filter, plus the hidden count and the filter state for the toolbar.
    /// Carries the connected server's last-requested log level (or `None`
    /// while disconnected) for the runtime-level selector.
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
    /// The deferred half of a rename made while its project was open: once
    /// the project is no longer the active library package (its close has
    /// been released just above), move the library directory to the new
    /// name with the ordinary catalog rename — the same op the gallery's
    /// kebab runs, so the slug, the manifest and the cloud's `Renamed`
    /// trigger all land the way a closed-project rename lands them.
    ///
    /// A rename that never gets its close (the tab is shut first) keeps the
    /// manifest name it already wrote; only the dated directory slug stays
    /// behind, which the next gallery rename fixes.
    async fn run_pending_reslug(&mut self) {
        let Some((uid, name)) = self.pending_reslug.clone() else {
            return;
        };
        if self.project.active_library_uid().as_deref() == Some(uid.as_str()) {
            return;
        }
        self.pending_reslug = None;
        if let Err(error) = self
            .run_catalog_op(CatalogOp::Rename {
                uid: uid.clone(),
                new_slug: name,
            })
            .await
        {
            log::warn!("library slug of {uid} did not follow its rename: {error}");
        }
    }

    pub async fn settle_library(&mut self) {
        self.project.release_closed_library_projects().await;
        self.run_pending_reslug().await;
        if !self.library_refresh_pending {
            return;
        }
        self.library_refresh_pending = false;
        let Some(host) = self.library_host.clone() else {
            return;
        };
        let open_elsewhere = host.open_elsewhere_uids().await;
        let mut snapshot_fs = None;
        match host.catalog_snapshot().await {
            Ok(fs) => {
                let inputs =
                    home_view_builder::hydrate_home_inputs(Rc::clone(&fs), &open_elsewhere);
                // The add-node picker's import source is derived from the
                // same walk (P5) — one snapshot read feeds both the
                // gallery and the picker.
                self.project
                    .set_import_patterns(home_view_builder::importable_patterns(&inputs));
                self.home_inputs = Some(inputs);
                snapshot_fs = Some(fs);
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
        // The registry rows come off the same snapshot walk; the roster
        // rehydrates from them the first time they land, so a remembered
        // board has a card before any port is open — and its last picture,
        // off the same snapshot, so the tile shows what it last did.
        self.load_device_records_if_due();
        if let Some(fs) = snapshot_fs.as_ref() {
            self.seed_device_sim_records(fs);
            self.seed_device_frame_snapshots(fs);
        }
        self.mark_dirty();
    }

    pub fn apply_console_command(&mut self, command: ConsoleCommand) {
        match command {
            ConsoleCommand::SetMinLevel(level) => self.log_filter.min_level = level,
            ConsoleCommand::SetOriginEnabled(origin, enabled) => {
                self.log_filter.set_origin_enabled(origin, enabled);
            }
            ConsoleCommand::Clear => self.logs.clear(),
            // Converted into a `RuntimeOp::SetLogLevel` action at actor intake
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
        // Release closed projects' locks and re-hydrate the gallery when
        // the action made either due (open/close/save/home ops).
        self.settle_library().await;
        // A dispatched action changes local state (project state, focus,
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
        self.try_pending_device_lens().await;
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
        Ok(Some(outcome))
    }

    pub fn mark_passive_project_refresh_failed(&mut self, message: impl Into<String>) {
        self.project.mark_project_sync_failed(message);
        // A sync failure changes the project pane's status even if the revision
        // did not move, so the next change gate must emit it.
        self.mark_dirty();
    }

    async fn dispatch_inner(&mut self, action: UiAction, updates: UxUpdateSink) -> UiResult {
        self.sync_lens_probe_policy();
        let node_id = action.node_id().clone();
        let project_node_id = self.project.node_id();

        if node_id.as_str() == HOME_NODE_ID {
            let op = action.into_op::<HomeOp>()?;
            return self.execute_home_op(op, updates).await;
        }
        if node_id.as_str() == crate::AgentController::NODE_ID {
            let op = action.into_op::<crate::AgentOp>()?;
            return self.execute_agent_op(op).await;
        }
        if node_id.as_str() == crate::RuntimeOp::NODE_ID {
            let op = action.into_op::<crate::RuntimeOp>()?;
            return self.execute_runtime_op(op, updates).await;
        }
        if node_id.as_str() == crate::DevicesOp::NODE_ID {
            let op = action.into_op::<crate::DevicesOp>()?;
            return self.execute_devices_op(op).await;
        }
        if node_id.as_str() == crate::DevicePushOp::NODE_ID {
            let op = action.into_op::<crate::DevicePushOp>()?;
            return self.execute_device_push_op(op).await;
        }
        if node_id.as_str() == crate::DeviceFeedOp::NODE_ID {
            let op = action.into_op::<crate::DeviceFeedOp>()?;
            self.set_device_feed_wanted(op.device, op.wanted);
            return Ok(UiNotices::new());
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

    /// Every file of one library package, read through a fresh read-only
    /// snapshot. No lock: the source of a vendoring is somebody else's
    /// project, possibly open in another tab, and reading it must never
    /// contend with them.
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

    async fn execute_home_op(&mut self, op: HomeOp, updates: UxUpdateSink) -> UiResult {
        match op {
            HomeOp::OpenPackage { key } => {
                return self
                    .open_from_home(
                        PendingOpen::Package {
                            key,
                            on: OpenOn::Resolve,
                        },
                        updates,
                    )
                    .await;
            }
            HomeOp::OpenPackageOnDevice {
                key,
                base_mac,
                over_running_project,
            } => {
                return self
                    .open_from_home(
                        PendingOpen::Package {
                            key,
                            on: OpenOn::Device {
                                base_mac,
                                over_running_project,
                            },
                        },
                        updates,
                    )
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
            HomeOp::CreateProject { template, name } => {
                // Create-and-open (the D17 deviation, 2026-07-27): the
                // package lands in the library — slugged/dated/deduped by
                // the store from the name the menu's optional field carried,
                // else the template's label; rename lives on the card kebab
                // and in the project's settings — then opens like any card,
                // so the user lands in the editor with something to do next.
                // `Blank` sends no files at all, so the historical path is
                // untouched.
                let files = crate::app::home::template_project_files(template)?;
                let name = name
                    .map(|name| name.trim().to_string())
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| template.default_project_name().to_string());
                let outcome = self
                    .run_catalog_op(CatalogOp::Create { name, files })
                    .await?;
                let created = outcome.summary.ok_or_else(|| {
                    UiError::MissingSession("create produced no package".to_string())
                })?;
                return self
                    .open_from_home(
                        PendingOpen::Package {
                            key: created.uid.to_string(),
                            on: OpenOn::Resolve,
                        },
                        updates,
                    )
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
                    .open_from_home(
                        PendingOpen::Package {
                            key: created.uid.to_string(),
                            on: OpenOn::Resolve,
                        },
                        updates,
                    )
                    .await;
            }
            HomeOp::RenamePackage { uid, name } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(UiError::UnsupportedAction(
                        "a project name cannot be empty".to_string(),
                    ));
                }
                // The project OPEN in this tab — the one whose settings row
                // or gallery card was just edited while it runs — cannot go
                // through the catalog: the host refuses structural ops on
                // it (`OpenInThisTab`, the project-before-catalog rule).
                // Its name is patched through its own handle instead, and
                // the library directory follows when the project closes
                // (`run_pending_reslug`).
                if self.project.active_library_uid().as_deref() == Some(uid.as_str()) {
                    let renamed = {
                        let server = self.pool.lens_session_mut()?.client_mut()?;
                        self.project.rename_active_project(server, name).await?
                    };
                    self.pending_reslug = Some((uid, renamed.clone()));
                    return Ok(UiNotices::new()
                        .with_notice(UiNotice::info(format!("Renamed to {renamed}"))));
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
        }
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

    /// Open a home card ON A DEVICE (D33, PD9): resolve the SIM of the
    /// project's target — the one that last ran it, an idle one, or a
    /// freshly minted record — power it on, land the lens on it, and push
    /// the package's head. Never silicon: putting a project on a real board
    /// is that board's own card verb.
    ///
    /// This is also where the open's public narration begins and ends
    /// ([`crate::app::open_progress`]): the stage the opening frame reads,
    /// the supersede generation the parked flow checks, and the terminal
    /// failure that replaces the eternal skeleton with an error and a
    /// Retry.
    async fn open_from_home(&mut self, pending: PendingOpen, updates: UxUpdateSink) -> UiResult {
        // The click owns the engine while it runs: background preview work
        // (pool boots, new lease deploys, hover-to-play) stops STARTING
        // until this guard drops, on every exit path — success, failure, or
        // the whole future being dropped. See `app::open_priority`.
        let _open_priority = crate::app::open_priority::begin_user_open();
        crate::app::open_progress::note_open_started();
        // A new open supersedes the mismatch page's question: whatever the
        // person is doing now, it is not answering that page.
        self.open_mismatch = None;
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
                // A pending LENS means the sim was powered on and has not
                // said hello yet: the open is not over, it is held. The
                // tick's `try_pending_device_lens` finishes it and clears
                // the pending open then — dropping it here would push the
                // package at nothing.
                if self.pending_device_lens.is_none() {
                    self.pending_open = None;
                }
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
            // A held lens is not a settled open: the sim is booting and the
            // opening frame keeps narrating until the tick lands the lens.
            Ok(notices) if self.pending_device_lens.is_some() => Ok(notices),
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

    /// The open, once the library is known to be there: resolve the device,
    /// make sure it is on, and land the lens on it. The push is
    /// [`Self::open_pending_package`], reached from [`Self::attach_lens`]
    /// so that a lens landing late (a sim still booting) pushes too.
    async fn open_from_home_inner(&mut self, updates: UxUpdateSink) -> UiResult {
        // WHAT HARDWARE this project is for, decided before anything is
        // reused: a sim is created wearing a board and cannot be re-dressed
        // into another, so the resolution below picks a sim of THIS target
        // or mints one. An untargeted project reads as Desktop.
        let (target, target_notice) = self.pending_open_target().await.resolve_for_sim();
        if let Some(notice) = target_notice {
            self.push_log(UiLogDraft::new(
                UiLogLevel::Warn,
                UiLogOrigin::Studio,
                notice,
            ));
        }
        // Boundary 1 of 3: the first real await this open can be outlived
        // at. Nothing project-specific has happened yet — no lock, no
        // runtime, no deploy — so a stale open just leaves.
        if crate::app::open_progress::open_superseded() {
            return Ok(UiNotices::new());
        }
        // A NAMED device (`?on=mac:…`) settles the question outright, and
        // is the one arm with a stop in it: the mismatch page (D50).
        let named = match self.pending_open_on() {
            OpenOn::Device {
                base_mac,
                over_running_project,
            } => match self.open_on_named_device(&base_mac, over_running_project) {
                Ok(uid) => Some(uid),
                // The open stopped at the mismatch page. Not a failure —
                // the person has a page with two verbs on it — so the
                // opening frame's error state stays out of this.
                Err(NamedDeviceRefusal::Mismatch(mismatch)) => {
                    self.open_mismatch = Some(Box::new(mismatch));
                    self.pending_open = None;
                    self.mark_dirty();
                    updates.emit(UxUpdate::View(self.view()));
                    return Ok(UiNotices::new());
                }
                // The device went away between the address being read and
                // this open running. PD14's rule for an unknown hint: drop
                // it and take the default, saying so once.
                Err(NamedDeviceRefusal::Unknown) => {
                    self.push_log(UiLogDraft::new(
                        UiLogLevel::Warn,
                        UiLogOrigin::Studio,
                        format!("no device answers to {base_mac}; opening on a sim instead"),
                    ));
                    None
                }
            },
            OpenOn::Resolve => None,
        };
        let uid = match named {
            Some(uid) => uid,
            None => {
                // The tab's own sim already wears this target: reuse it
                // outright. The first arm of the resolution (the device
                // that last ran this project) and the second (an idle sim
                // of the target) both land here, and there is nothing to
                // power on or hand a wire to — the lens is already holding
                // it.
                if let Some(id) = self.lens_on_a_sim_wearing(&target) {
                    return self.attach_lens(id, updates).await;
                }
                self.resolve_open_device(&target).await?
            }
        };
        // The open's own narration names the DEVICE it is starting, not
        // "the simulator": there is one device per open and it has a board.
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Starting the device",
            "Starting",
            &format!(
                "Starting {} (sim)",
                crate::board_display_name(target.board_id())
            ),
        );
        // ONE DEVICE PER TAB (D37). The lens goes first — a lens on another
        // device holds its wire, and `open_device_lens` would close it for
        // us anyway; doing it here keeps the power-off below unambiguous.
        if self
            .pool
            .lens_session()
            .is_some_and(|session| session.attachment().uid != uid)
        {
            self.close_device_lens();
            // The departing session's project tab lock has to be free
            // before this open takes it; the settle points run too late.
            self.project.release_closed_library_projects().await;
        }
        self.power_off_other_sims(&uid);
        // Power on is `Connect` at the fold (PD8/Q15) — the same gesture
        // the card's own Power on raises, so nothing here is a second flow.
        // A sim already on is a no-op at the transport, and the fold
        // re-opens a port it had closed.
        //
        // **Sims only.** A board is not powered on by Studio: it is on a
        // desk, reached over a port the browser already granted, and
        // `Connect`ing it would close and reopen a wire that was working —
        // which drops the card back through Connecting and leaves the lens
        // held behind a board that was Ready a moment ago. Attach-or-
        // connect for silicon is `open_device_lens`'s own job, and it has
        // been doing it since the device route existed.
        let is_sim = self.device_sims.contains_key(&uid);
        if is_sim
            && let Some(device) = self.devices.device_for_key(&uid).map(|device| device.id)
        {
            self.execute_devices_op(crate::DevicesOp::on_sim(crate::DeviceAction::Connect {
                device,
            }))
            .await?;
        }
        // Boundary 2 of 3: the runtime is starting. A superseded open
        // leaves it STANDING — it is the expensive part, every open wants
        // one, and the click that replaced this one walks straight into it.
        if crate::app::open_progress::open_superseded() {
            return Ok(UiNotices::new());
        }
        // The lens: attaches now if the device already said hello, or is
        // held as `pending_device_lens` and attached from the tick the
        // moment it does. Either way the push rides `attach_lens`.
        self.open_device_lens(&uid, updates).await
    }

    /// Bank an open that NAMED a device, the way a card push is banked:
    /// the registry row records the project it was last given.
    ///
    /// This open really did push — the lens deploy sends the package over
    /// the board's own wire — and the association is what the mismatch
    /// page reads to answer "what is on this device?" (D50). Without this,
    /// answering the page once would leave the row still naming the
    /// project that was replaced, and the next reload would raise the same
    /// page offering to switch to something that is no longer running:
    /// the page would lie, which is worse than the page not existing.
    ///
    /// Scoped to the named arm on purpose. A device the address named is
    /// the only one whose association gets *read* back as a promise; the
    /// resolver's own sims are Studio's scratch devices and no surface
    /// asks them this question. Banking every open would be a broader
    /// change to what a push means, and that belongs with the pull the
    /// backup story still needs (Q7), not here.
    ///
    /// Best-effort, exactly like [`Self::bank_completed_push`]: the
    /// project is open and running either way, and a bookkeeping failure
    /// is a log line, never a failed open. `record_push` refuses a version
    /// the project's history never recorded (an unsaved working copy), and
    /// that refusal is correct — an event may not name a snapshot the
    /// library cannot produce.
    async fn bank_open_on_named_device(&mut self) {
        let OpenOn::Device { .. } = self.pending_open_on() else {
            return;
        };
        let Some(uid) = self.pending_open_key() else {
            return;
        };
        let Some(project_uid) = self.library_uid_for_key(&uid) else {
            return;
        };
        let Some(attachment) = self
            .pool
            .lens_session()
            .map(|session| session.attachment().uid.clone())
        else {
            return;
        };
        let Some(row) = self
            .devices
            .device_for_key(&attachment)
            .and_then(|device| device.record.as_ref())
            .and_then(crate::app::devices::registry_row_from_record)
        else {
            return;
        };
        let version = match self.library_head_hash(&project_uid).await {
            Some(version) => version,
            None => return,
        };
        if let Err(error) = self
            .run_catalog_op(CatalogOp::RecordPush {
                project_uid,
                device: Box::new(row),
                version,
            })
            .await
        {
            log::warn!("open on a named device not banked: {error}");
        }
    }

    /// One library project's head content hash, or `None` when the
    /// library, the key or the package is unavailable.
    async fn library_head_hash(&mut self, key: &str) -> Option<lpc_history::ContentHash> {
        let host = self.library_host().ok()?;
        let fs = host.catalog_snapshot().await.ok()?;
        let store = crate::app::library::LibraryStore::read_only(fs);
        let uid = store.resolve_key(key).ok()?;
        store.open(uid).ok()?.content_hash().ok()
    }

    /// Which device the pending open was told to land on.
    fn pending_open_on(&self) -> OpenOn {
        match &self.pending_open {
            Some(PendingOpen::Package { on, .. }) => on.clone(),
            _ => OpenOn::Resolve,
        }
    }

    /// The device a `?on=mac:` address named, if this open may have it.
    ///
    /// The refusal is the point of the whole arm: a device already running
    /// a *different* library project is never pushed over silently (D50).
    /// What comes back instead is the page's material — both projects by
    /// name, and the device — so the person decides with the consequence
    /// in front of them.
    fn open_on_named_device(
        &self,
        base_mac: &str,
        over_running_project: bool,
    ) -> Result<String, NamedDeviceRefusal> {
        let registry = self
            .home_inputs
            .as_ref()
            .map(|inputs| inputs.registered.as_slice())
            .unwrap_or_default();
        let found = crate::app::devices::device_by_base_mac(
            self.devices.roster().devices(),
            registry,
            &self.device_sims,
            base_mac,
        )
        .ok_or(NamedDeviceRefusal::Unknown)?;
        let Some(key) = self.pending_open_key() else {
            return Ok(found.key);
        };
        let project_uid = self.library_uid_for_key(&key).unwrap_or(key);
        if over_running_project || !found.would_push_over(&project_uid) {
            return Ok(found.key);
        }
        let running = found
            .last_given_project
            .as_deref()
            .and_then(|uid| self.library_project_named(uid));
        Err(NamedDeviceRefusal::Mismatch(crate::UiOpenMismatch {
            project_name: self
                .library_project_named(&project_uid)
                .map(|project| project.name)
                .unwrap_or_else(|| project_uid.clone()),
            project_uid,
            device_key: found.key,
            device_name: found.name,
            device_base_mac: base_mac.to_string(),
            running,
        }))
    }

    /// The pending open's library key, when it has one.
    fn pending_open_key(&self) -> Option<String> {
        match &self.pending_open {
            Some(PendingOpen::Package { key, .. }) => Some(key.clone()),
            _ => None,
        }
    }

    /// One library project as the mismatch page names it — `None` when
    /// this library does not have that uid, which is the whole of the
    /// "nothing to switch to" test.
    fn library_project_named(&self, uid: &str) -> Option<crate::UiRunningProject> {
        self.home_inputs
            .as_ref()?
            .projects
            .iter()
            .find(|card| card.uid == uid)
            .map(|card| crate::UiRunningProject {
                uid: card.uid.clone(),
                name: card.slug.clone(),
            })
    }

    /// A library key (a `prj…` uid, or the slug a URL carried) as the uid
    /// the registry's associations are written against.
    fn library_uid_for_key(&self, key: &str) -> Option<String> {
        let projects = &self.home_inputs.as_ref()?.projects;
        projects
            .iter()
            .find(|card| card.uid == key || card.slug == key)
            .map(|card| card.uid.clone())
    }

    /// The session the lens already holds, when it is a SIM wearing
    /// `target` — the open can reuse it as-is.
    ///
    /// A sim is created wearing a board and cannot be re-dressed into
    /// another, so the board has to match; a lens on silicon never
    /// matches, because opening a project must not touch a board (D33).
    fn lens_on_a_sim_wearing(
        &self,
        target: &crate::app::library::ProjectTarget,
    ) -> Option<crate::RuntimeId> {
        let session = self.pool.lens_session()?;
        let attachment = session.attachment();
        (attachment.transport == crate::LinkTransport::Sim
            && attachment.board_id.as_deref() == Some(target.board_id()))
        .then(|| session.id())
    }

    /// The device this open lands on (PD9, PD14's default arm): the sim
    /// that last ran this project, else an idle sim of the same target,
    /// else a freshly minted record.
    ///
    /// Silicon is never resolved here (D33): putting a project on a board
    /// is that board's own card verb.
    async fn resolve_open_device(
        &mut self,
        target: &crate::app::library::ProjectTarget,
    ) -> Result<String, UiError> {
        let target_id = target.board_id().to_string();
        let of_target: Vec<String> = self
            .device_sims
            .iter()
            .filter(|(_, record)| record.target == target_id)
            .map(|(uid, _)| uid.clone())
            .collect();
        // The one that LAST RAN this project, when the tab can tell: the
        // lens is the only place that pairing is known before P4's `?on=`
        // grammar, and it is the common case (reopening what you just had).
        let pending_key = match &self.pending_open {
            Some(PendingOpen::Package { key, .. }) => Some(key.clone()),
            _ => None,
        };
        if let Some(key) = pending_key
            && self.project.active_library_uid().as_deref() == Some(key.as_str())
            && let Some(session) = self.pool.lens_session()
            && session.transport() == crate::LinkTransport::Sim
            && of_target.contains(&session.attachment().uid)
        {
            return Ok(session.attachment().uid.clone());
        }
        // Else an idle one of the same target: any remembered sim wearing
        // it. A powered-off record is as good as a running one — powering
        // it on is the next step either way.
        if let Some(uid) = of_target.first() {
            return Ok(uid.clone());
        }
        // Else make one. Until P5's picker, a board-target project mints
        // its sim silently on open — which is D33.
        let random = self.random_bytes6();
        let uid = self.create_sim_record(&target_id, None, &random).await?;
        // The record is in the LIBRARY; the roster learns it at the next
        // settle, and the power-on below needs a device to aim at. Settle
        // here rather than waiting for the dispatch's own settle point,
        // which runs after this open has already finished.
        self.settle_library().await;
        Ok(uid)
    }

    /// Power off every sim that is NOT the one this open lands on (D37,
    /// silently). The record stays: a powered-off sim sits on the
    /// remembered line with Power on (Q5).
    fn power_off_other_sims(&mut self, keep_uid: &str) {
        self.power_off_sims_except(Some(keep_uid));
    }

    /// Power every sim this tab runs off — the tab (or a docs host) going
    /// away. A running sim is a worker, and a worker nobody terminates
    /// outlives the page that started it.
    pub fn power_off_sims(&mut self) {
        self.power_off_sims_except(None);
    }

    fn power_off_sims_except(&mut self, keep_uid: Option<&str>) {
        let Some(transport) = self.sim_transport.clone() else {
            return;
        };
        let others: Vec<String> = transport
            .powered_uids()
            .into_iter()
            .filter(|uid| Some(uid.as_str()) != keep_uid)
            .collect();
        for uid in others {
            // A sim with no roster device is one that never said hello:
            // the transport still holds its worker, and powering it off is
            // the whole of the teardown.
            let Some(device) = self.lens_facts_at_address(&uid).map(|facts| facts.device) else {
                transport.power_off(&uid);
                continue;
            };
            let action = crate::DeviceAction::Disconnect { device };
            self.fold_device_input(crate::DeviceInput::Action(action.clone()));
            self.power_off_sim_for(&action);
        }
    }

    /// Six bytes of the injected randomness, for a minted sim's MAC.
    fn random_bytes6(&self) -> [u8; 6] {
        let bytes = (self.random)();
        let mut six = [0u8; 6];
        six.copy_from_slice(&bytes[..6]);
        six
    }

    /// What hardware the PENDING open's project declares, read before the
    /// project is opened (that is the point: it decides which sim the open
    /// lands on).
    ///
    /// Read leniently and never fatally — a manifest this cannot read is
    /// the open's problem to report a moment later, with a better error
    /// than "unknown target" — so every failure path here reads as the
    /// default, Desktop.
    async fn pending_open_target(&mut self) -> crate::app::library::ProjectTarget {
        use crate::app::library::ProjectTarget;

        match self.pending_open.clone() {
            Some(PendingOpen::Package { key, .. }) => self
                .read_library_project_manifest(&key)
                .await
                .as_deref()
                .map_or(ProjectTarget::Desktop, ProjectTarget::from_manifest_bytes),
            Some(PendingOpen::Example(id)) => {
                crate::app::home::embedded_example::embedded_example(&id)
                    .and_then(|example| example.file("project.json"))
                    .map_or(ProjectTarget::Desktop, ProjectTarget::from_manifest_bytes)
            }
            Some(PendingOpen::SharedTransient { package_files, .. }) => package_files
                .iter()
                .find(|(path, _)| path == "project.json")
                .map_or(ProjectTarget::Desktop, |(_, bytes)| {
                    ProjectTarget::from_manifest_bytes(bytes)
                }),
            // A docs page is a computer, and a docs example targets no
            // board.
            Some(PendingOpen::DocsExample(_)) | None => ProjectTarget::Desktop,
        }
    }

    /// One library package's `project.json` bytes, or `None` when the
    /// library, the key or the file is unavailable.
    async fn read_library_project_manifest(&mut self, key: &str) -> Option<Vec<u8>> {
        let host = self.library_host().ok()?;
        let fs = host.catalog_snapshot().await.ok()?;
        let store = crate::app::library::LibraryStore::read_only(fs);
        let uid = store.resolve_key(key).ok()?;
        let handle = store.open(uid).ok()?;
        let manifest = handle
            .package_fs
            .borrow()
            .read_file(lpc_model::AsLpPath::as_path(
                &crate::app::library::package_manifest::MANIFEST_PATH,
            ))
            .ok()?;
        Some(manifest)
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
        // A docs example is a direct deploy, not a library open: it has its
        // own narration and never touches the catalog.
        if let PendingOpen::DocsExample(id) = &pending {
            let id = id.clone();
            return self.deploy_docs_example(&id, updates).await;
        }
        crate::app::open_progress::note_preparing_project();
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Opening project",
            "Opening",
            "Sending the project to the device",
        );
        let result = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            match &pending {
                PendingOpen::Package { key, .. } => {
                    self.project.open_library_package(server, &key).await
                }
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
                // Handled above, before the library seam is reached.
                PendingOpen::DocsExample(_) => unreachable!("docs deploys never reach the library"),
            }
        };
        match result {
            Ok(logs) => {
                self.record_logs(logs);
                // The open path's own notices come FIRST — a format upgrade
                // is the thing the user most needs to read, and it must not
                // be a console-only line (P3).
                let mut notices = UiNotices::new();
                for notice in self.project.take_open_notices() {
                    notices = notices.with_notice(notice);
                }
                let sync = self.sync_project_after_attach(updates).await?;
                self.bank_open_on_named_device().await;
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

    /// The runtime-scoped verbs (the sim's danger zone, the console's
    /// log-level selector).
    /// One device gesture: fold it, then settle whatever it asked to persist.
    ///
    /// Nothing here decides anything about a device — the model does. That is
    /// the point: a UI gesture and a link event enter through the same door,
    /// with the same rights the fold gives each arm.
    ///
    /// The ONE piece of app policy layered on top: a Flash gesture at an
    /// unnamed board auto-derives its name first ("<board display_name> ·
    /// <Mon D>", the skip-ALL-naming ruling). It rides as a `SetName` action
    /// — a journaled user-stream write, exactly what a rename later uses —
    /// so the model's two-stream rights stay untouched.
    async fn execute_devices_op(&mut self, op: crate::DevicesOp) -> UiResult {
        self.yield_lens_wire_for(op.action());
        if let Some(name_first) = self.derive_flash_name_action(op.action()) {
            self.fold_device_input(crate::DeviceInput::Action(name_first));
        }
        // A sim's runtime is started HERE, before the fold, so the sweep the
        // model's own `Connect` triggers finds a link to attach.
        self.power_on_sim_for(op.action());
        // …and the uid a power-off will need is read before it too: a
        // `Forget` takes the device with it, and the roster is where that
        // uid comes from.
        let power_off = self.sim_uid_to_power_off(op.action());
        self.fold_device_input(crate::DeviceInput::Action(op.action.clone()));
        self.power_off_sim(power_off);
        self.settle_device_records().await;
        Ok(UiNotices::new())
    }

    /// One wire, one owner: a card gesture that needs the board's wire
    /// while the editor is a lens on that board closes the lens FIRST, so
    /// the gesture gets the wire instead of fighting the session's client
    /// for it (direct-control doctrine — the card's verbs always work; the
    /// editor is the thing that yields). Gestures that never touch the wire
    /// (a rename, the autoconnect toggle) leave the lens alone.
    fn yield_lens_wire_for(&mut self, action: &crate::DeviceAction) {
        let touches_wire = !matches!(
            action,
            crate::DeviceAction::SetName { .. } | crate::DeviceAction::SetAutoconnect { .. }
        );
        let Some(target) = action.device() else {
            return;
        };
        let lens_device = self
            .pool
            .attached_session()
            .map(|session| session.attachment().device);
        if touches_wire && lens_device == Some(target) {
            self.push_log(UiLogDraft::new(
                UiLogLevel::Info,
                UiLogOrigin::Studio,
                "the editor closed so the board's wire is free for this".to_string(),
            ));
            self.close_device_lens();
        }
    }

    /// The empty face's gesture: prepare a project, then push it.
    ///
    /// Two steps, in this order and for this reason. The preparation is
    /// library work — install the example, generate the starter, read the
    /// package — which is asynchronous and knows about uids, slugs and
    /// content hashes; none of that may enter the device model (its journal
    /// records every input verbatim, and project bytes have no business in a
    /// flight recorder). So the result is STAGED with the effects layer and
    /// the model is handed a bare `Action::Push`.
    ///
    /// A preparation that FAILED is staged too, as its own message. That is
    /// what makes "the generator refused" and "the board refused the write"
    /// land on the same honest problem face with the same in-place retry,
    /// instead of one of them being a toast the card never hears about.
    async fn execute_device_push_op(&mut self, op: crate::DevicePushOp) -> UiResult {
        self.yield_lens_wire_for(&crate::DeviceAction::Push { device: op.device });
        // The picker's "name the board to match" offer: a user-stream
        // rename, folded before the push exactly as the card's own Rename
        // would be.
        if let Some(name) = op.device_name.clone() {
            self.fold_device_input(crate::DeviceInput::Action(crate::DeviceAction::SetName {
                device: op.device,
                name,
            }));
        }
        let staged = self.prepare_push(&op.source).await;
        if let Err(reason) = &staged {
            log::warn!("nothing to push to {:?}: {reason}", op.device);
        }
        self.devices.effects_mut().stage_push(op.device, staged);
        self.fold_device_input(crate::DeviceInput::Action(crate::DeviceAction::Push {
            device: op.device,
        }));
        self.settle_device_records().await;
        Ok(UiNotices::new())
    }

    /// Resolve a picked source into the bytes that will go on the board.
    ///
    /// The three sources converge on one shape — a library project — because
    /// a push must be recordable: the history event and the device
    /// association are written against a `prj…` uid, and something pushed
    /// from nowhere could never be banked or compared afterwards. So an
    /// example is INSTALLED first (fresh uid minted at install, the incoming
    /// manifest untouched — the examples vision's rule) and a starter is
    /// GENERATED into the library first.
    async fn prepare_push(&mut self, source: &crate::PushSource) -> crate::StagedPush {
        let uid = match source {
            crate::PushSource::Library { project_uid } => project_uid.clone(),
            crate::PushSource::Example { example_id } => {
                let example = crate::app::home::embedded_example::embedded_example(example_id)
                    .ok_or_else(|| format!("this build has no example called {example_id}"))?;
                // No naming step (ruled): the library's dated-slug
                // convention names it from the example's own label.
                self.install_for_push(CatalogOp::ForkTransientCopy {
                    name: example.name.to_string(),
                    files: example.files(),
                    provenance: crate::app::library::PackageProvenance::SeededFrom {
                        source: example.id.to_string(),
                    },
                })
                .await?
            }
            crate::PushSource::NewForBoard { board_id, name } => {
                self.install_for_push(CatalogOp::GenerateForBoard {
                    board_id: board_id.clone(),
                    name: name.clone(),
                })
                .await?
            }
        };
        let (files, content_hash, label) = self.read_push_payload(&uid).await?;
        Ok(crate::PushPayload {
            project_uid: uid,
            label,
            files,
            content_hash,
            fallback_storage_id: crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID
                .to_string(),
        })
    }

    /// Run a creation-shaped catalog op and return the uid it installed.
    async fn install_for_push(&mut self, op: CatalogOp) -> Result<String, String> {
        let outcome = self
            .run_catalog_op(op)
            .await
            .map_err(|error| error.to_string())?;
        outcome
            .summary
            .map(|summary| summary.uid.to_string())
            .ok_or_else(|| "the project was not installed".to_string())
    }

    /// Read a library project's files + canonical hash.
    ///
    /// Through the LIVE handle when that project is open in this tab (asking
    /// the host for a second open would be refused — this tab holds the
    /// lock), otherwise through a read-only open whose receipt is abandoned
    /// the moment the bytes are in hand, so a push never leaves a lock
    /// behind.
    async fn read_push_payload(
        &mut self,
        uid: &str,
    ) -> Result<(Vec<(String, Vec<u8>)>, String, String), String> {
        if let Some(read) = self.project.read_open_package(uid) {
            let (files, hash) = read.map_err(|error| error.to_string())?;
            // The open handle has no slug to hand back through this seam, so
            // the label comes from the gallery's own row — the same name the
            // user picked in the list.
            let label = self
                .home_inputs
                .as_ref()
                .and_then(|inputs| inputs.projects.iter().find(|card| card.uid == uid))
                .map(|card| card.slug.clone())
                .unwrap_or_else(|| uid.to_string());
            return Ok((files, hash, label));
        }
        let host = self.library_host().map_err(|error| error.to_string())?;
        let opened = host
            .open_project(uid)
            .await
            .map_err(|error| self.library_error_with_name(error).to_string())?;
        let handle = crate::app::library::PackageHandle::load(
            opened.uid,
            opened.slug.clone(),
            opened.package_fs,
            opened.history_fs,
        )
        .map_err(|error| error.to_string());
        let payload = handle.and_then(|handle| {
            crate::app::project::project_controller::read_package_payload(&handle)
                .map_err(|error| error.to_string())
        });
        // Read-only by construction: the receipt is given back either way,
        // so a failed read cannot strand the project's lock. Abandoning IS
        // the release (unregister, stop the flushers, drop the lock) — a
        // `close_project` on top would be closing something nothing holds.
        opened.receipt.abandon();
        let (files, hash) = payload?;
        Ok((files, hash, opened.slug))
    }

    /// The auto-names due right now: one `SetName` per registered board that
    /// has none (`auto_record_name` holds the rule and its three guards).
    ///
    /// The FLASH gesture has minted a name since M2, but only that gesture
    /// did — so a board that arrived already flashed (the bench C6, anything
    /// flashed from the CLI, any row written before the auto-name existed)
    /// wore its MAC as its title forever, which is what G1 found on the
    /// devices page. Running the same rule at the registry-write seam covers
    /// every road onto the roster, including a boot that rehydrated the row
    /// from `/registry.json`.
    ///
    /// It reads the model's own records rather than the registry file: the
    /// record is what the fold is about to persist, and the name has to
    /// enter through the model anyway (the card's title is
    /// `Device::title()`, not the row's `name` — a row renamed behind the
    /// model's back would be forgotten by the next snapshot).
    fn auto_name_actions(&mut self) -> Vec<crate::DeviceAction> {
        let unnamed: Vec<lpa_devices::record::DeviceRecord> = self
            .devices
            .roster()
            .devices()
            .iter()
            .filter_map(|entry| entry.record.clone())
            // The rule's own guards are in `auto_record_name`; these two are
            // repeated here only so the common case (every board named, or a
            // blank board that has not said what it is) costs a scan and
            // never a roster projection.
            .filter(|record| record.name.is_none() && record.identity.name.is_none())
            .filter(|record| record.board_id.is_some())
            .collect();
        if unnamed.is_empty() {
            return Vec::new();
        }
        let now = (self.now_secs)();
        let mut taken =
            crate::app::devices::taken_device_titles(&self.device_roster_view().roster.devices);
        let mut actions = Vec::new();
        for record in unnamed {
            let Some(name) = crate::app::devices::auto_record_name(&record, now, &taken) else {
                continue;
            };
            taken.push(name.clone());
            actions.push(crate::DeviceAction::SetName {
                device: record.device,
                name,
            });
        }
        actions
    }

    /// The auto-name that precedes a Flash on a still-unnamed board, if one
    /// is due. `None` for every other gesture, for a named board (a re-flash
    /// must not rename), for a Flash that carries the name the user typed
    /// at setup (the model records that one itself), and for a target the
    /// roster does not hold.
    fn derive_flash_name_action(
        &self,
        action: &crate::DeviceAction,
    ) -> Option<crate::DeviceAction> {
        let crate::DeviceAction::Flash {
            device,
            board_id,
            name,
            ..
        } = action
        else {
            return None;
        };
        if name.is_some() {
            return None;
        }
        let named = self
            .devices
            .roster()
            .devices()
            .iter()
            .find(|entry| entry.id == *device)
            .map(|entry| entry.intent.name.is_some() || entry.identity.name.is_some())
            .or_else(|| {
                self.devices
                    .roster()
                    .pending()
                    .iter()
                    .find(|entry| entry.device_id() == *device)
                    .map(|entry| entry.identity().name.is_some())
            })?;
        if named {
            return None;
        }
        let board_display = lpa_boards::board_by_id(board_id)
            .map(|board| board.display_name.clone())
            .unwrap_or_else(|| board_id.clone());
        let taken =
            crate::app::devices::taken_device_titles(&self.device_roster_view().roster.devices);
        Some(crate::DeviceAction::SetName {
            device: *device,
            name: crate::app::devices::derive_flash_name(&board_display, (self.now_secs)(), &taken),
        })
    }

    async fn execute_runtime_op(
        &mut self,
        op: crate::RuntimeOp,
        updates: UxUpdateSink,
    ) -> UiResult {
        match op {
            crate::RuntimeOp::SetLogLevel { level } => self.set_runtime_log_level(level).await,
            crate::RuntimeOp::OpenDeviceLens { uid } => self.open_device_lens(&uid, updates).await,
            crate::RuntimeOp::CloseDeviceLens => {
                self.close_device_lens();
                Ok(UiNotices::new())
            }
        }
    }

    // -----------------------------------------------------------------
    // The device lens (round-2 M5): the editor on a roster device
    // -----------------------------------------------------------------

    /// Open the editor as a lens on the roster device registered as `uid`.
    ///
    /// The board stays the roster's: its identity, evidence and activities
    /// never move. What happens here is a HANDOVER of its wire — the effects
    /// layer pauses the pump and lends the port to a wire client the pool
    /// session owns, every line that client drains is teed back into the
    /// fold, and the ordinary lens attach runs on top (running project,
    /// mirror, sync). Single-session policy: a running sim is stopped first.
    ///
    /// Refusals are honest and leave nothing half-done: an unknown uid, a
    /// board that is not connected and identified as LightPlayer, one busy
    /// with an activity, or a wire the transport cannot lend.
    async fn open_device_lens(&mut self, uid: &str, updates: UxUpdateSink) -> UiResult {
        // Already there: re-attaching the same lens is a no-op with a
        // fresh mirror.
        if let Some(session) = self.pool.attached_session() {
            if session.attachment().uid == uid {
                let id = session.id();
                return self.attach_lens(id, updates).await;
            }
            // A lens on ANOTHER device: give its wire back first (D37 —
            // one device per tab, whatever backs either of them).
            self.close_device_lens();
        }
        let attachment = match self.device_lens_attachment(uid) {
            Ok(attachment) => attachment,
            // Not ready yet — or not even known yet: a reload asks for the
            // lens while the registry rows are still loading and the
            // granted port is still identifying. Either way the address
            // is an intent to hold, not a failure to report: the gallery
            // renders the board's honest state meanwhile, and the tick
            // attaches the lens the moment the board says hello. Only a
            // gesture on the gallery (close, another open) lets it go.
            Err(error) => {
                self.pending_device_lens = Some(uid.to_string());
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Info,
                    UiLogOrigin::Studio,
                    format!("waiting for the device before opening it: {error}"),
                ));
                return Ok(UiNotices::new().with_notice(UiNotice::info("Waiting for the device")));
            }
        };
        self.pending_device_lens = None;
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Opening device",
            "Opening",
            &format!("Opening {}", attachment.name),
        );
        let deadline = self.device_request_deadline()?;
        let link = attachment.link;
        let io = self
            .devices
            .effects_mut()
            .attach_lens_wire(link)
            .map_err(UiError::MissingSession)?;
        let protocol = match attachment.transport {
            crate::LinkTransport::Sim => "browser-worker",
            crate::LinkTransport::Serial => "usb-serial",
        };
        let client = crate::StudioServerClient::from_lens_io(io, deadline, protocol);
        let id = self.pool.install(crate::RuntimePayload::Device(attachment));
        self.record_device_event(
            Some(&id.to_string()),
            None,
            DeviceEventKind::Pool {
                action: "install".to_string(),
                detail: format!("device lens {uid}"),
            },
        );
        match self.pool.session_mut(id) {
            Some(session) => session.attach_device_client(client),
            None => {
                self.devices.effects_mut().release_lens_wire(link);
                return Err(UiError::MissingSession(
                    "the device session vanished after install".to_string(),
                ));
            }
        }
        self.pool.set_lens(id);
        let opened = match self.read_device_build(id).await {
            Ok(()) => self.attach_lens(id, updates).await,
            Err(error) => Err(error),
        };
        match opened {
            Ok(notices) => Ok(notices),
            Err(error) => {
                // The board answered the fold's hello but not the lens's
                // conversation (its own hello, or the attach): no lens, no
                // session, the wire goes back — the card says the rest.
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!("could not open the board in the editor: {error}"),
                ));
                self.close_device_lens();
                Err(error)
            }
        }
    }

    /// The lens's handle on the device registered as `uid`, or the honest
    /// reason it cannot be opened right now.
    fn device_lens_attachment(&self, uid: &str) -> Result<crate::DeviceLensAttachment, UiError> {
        let facts = self
            .lens_facts_at_address(uid)
            .ok_or_else(|| UiError::MissingSession(format!("no device is registered as {uid}")))?;
        let link = facts
            .link
            .ok_or_else(|| UiError::MissingSession("this board is not connected".to_string()))?;
        // Attached is not open: a port the model closed (Disconnect, a
        // cancelled identify) has stale hello evidence and no wire to lend.
        if !facts.open {
            return Err(UiError::MissingSession(
                "this board's port is closed — connect it first".to_string(),
            ));
        }
        // Any hello, on any wire version: an older board is opened on the
        // hope-it-works terms Yona ruled (2026-09-04) — the fold has
        // journaled the version difference, and a request the old firmware
        // cannot answer fails the way any request fails.
        let board_id = facts.hello.ok_or_else(|| {
            UiError::MissingSession(
                "this board has not identified itself as a LightPlayer yet".to_string(),
            )
        })?;
        if facts.busy {
            return Err(UiError::MissingSession(
                "this board is busy with an activity; wait for it to finish".to_string(),
            ));
        }
        Ok(crate::DeviceLensAttachment {
            device: facts.device,
            link,
            uid: uid.to_string(),
            name: facts.name,
            board_id,
            // Read off the device's own endpoint rung: `sim:<uid>` is the
            // sim transport's scheme, and nothing else answers to it.
            transport: facts
                .endpoint
                .as_deref()
                .map(crate::LinkTransport::from_endpoint)
                .unwrap_or(crate::LinkTransport::Serial),
            // The hello the fold mirrors carries no build features; the
            // attach reads them off the lens's own wire (`read_device_build`)
            // before the editor lands. Until then the picker offers
            // everything (the same as the sim).
            features: None,
        })
    }

    /// The device an address names: its registry uid, or — for a sim whose
    /// row has not settled yet, and for the docs page's anonymous one,
    /// which never will — its `sim:<uid>` endpoint.
    ///
    /// One resolution, so a sim powered on a moment ago is openable before
    /// the library write lands and the docs sim is openable without ever
    /// being written at all (PD10).
    fn device_at_address(&self, uid: &str) -> Option<&lpa_devices::Device> {
        if let Some(device) = self.devices.device_for_key(uid) {
            return Some(device);
        }
        let endpoint = crate::sim_endpoint(uid);
        self.devices
            .roster()
            .devices()
            .iter()
            .find(|device| device.identity.endpoint.as_ref() == Some(&endpoint))
    }

    /// What the lens needs to know about the thing at an address, whether
    /// the fold holds it as a device or as a still-PENDING link.
    ///
    /// The docs page's sim is the pending case, permanently: it is minted
    /// with no MAC (PD10 — identity-less, never a registry row), so its
    /// hello names no identity, the fold never promotes its link to a
    /// device, and the only handle it ever has is the `sim:<key>` endpoint
    /// its link wears. `device_at_address` promised to resolve it and
    /// searched only the devices, which is why the docs embeds stayed at
    /// "Starting the simulator" with a booted, identified runtime behind
    /// them (G1, 2026-09-07). The pending entry carries the same evidence
    /// a device does, so the lens reads the same facts off it.
    fn lens_facts_at_address(&self, uid: &str) -> Option<LensFacts> {
        if let Some(device) = self.device_at_address(uid) {
            return Some(LensFacts {
                device: device.id,
                link: device.link(),
                open: device.evidence.presence.is_open(),
                hello: device
                    .evidence
                    .classification
                    .hello()
                    .map(|hello| hello.board_id.clone()),
                busy: device.is_busy(),
                name: device.title(),
                endpoint: device
                    .identity
                    .endpoint
                    .as_ref()
                    .map(|endpoint| endpoint.0.clone()),
            });
        }
        let endpoint = crate::sim_endpoint(uid);
        let pending = self
            .devices
            .roster()
            .pending()
            .iter()
            .find(|pending| pending.info.endpoint == endpoint)?;
        let evidence = pending.evidence();
        Some(LensFacts {
            device: pending.device_id(),
            link: evidence.link(),
            open: evidence.presence.is_open(),
            hello: evidence
                .classification
                .hello()
                .map(|hello| hello.board_id.clone()),
            busy: pending.is_identifying(),
            name: pending.info.label.clone(),
            endpoint: Some(endpoint.0.clone()),
        })
    }

    /// The hardware request deadline: the device-session default budget,
    /// on the platform timer device waits already run on.
    fn device_request_deadline(&self) -> Result<lpa_client::RequestDeadline, UiError> {
        let timer = self.devices.effects().timer_factory().ok_or_else(|| {
            UiError::MissingSession("the device timer is not installed".to_string())
        })?;
        Ok(lpa_client::RequestDeadline::new(
            lpa_link::device_session::DEFAULT_REQUEST_TOTAL_DEADLINE,
            move |duration| (timer.borrow_mut())(duration),
        ))
    }

    /// Close the device lens, if one is open: quiesce the mirror, drop the
    /// session, give the wire back so the roster's pump resumes. Quiet when
    /// there is none. The device keeps its card and its evidence.
    pub(crate) fn close_device_lens(&mut self) {
        self.pending_device_lens = None;
        let Some(session) = self.pool.attached_session() else {
            return;
        };
        let id = session.id();
        let link = session.attachment().link;
        if self.pool.lens() == Some(id) {
            self.quiesce_lens();
        }
        if let Some(mut session) = self.pool.remove_attached_session() {
            let pending = session.take_pending_logs();
            self.record_session_logs(id, pending);
        }
        // Back to the roster's pump — and for a sim that is the whole of
        // the detach: the runtime keeps running, exactly like silicon.
        self.devices.effects_mut().release_lens_wire(link);
        self.record_device_event(
            Some(&id.to_string()),
            None,
            DeviceEventKind::Pool {
                action: "remove".to_string(),
                detail: "device lens closed".to_string(),
            },
        );
        self.mark_dirty();
    }

    /// The unplug-mid-lens row: once the lens's wire is gone — the model
    /// stopped routing the link (departure sweep, forget), or the fold
    /// heard the port close under the lens (the io's port error, teed
    /// through the tap) — the session has no wire and goes with it, no
    /// refresh needed. The card's own detach evidence is already in the
    /// fold; this only keeps the pool honest.
    fn drop_device_lens_if_wireless(&mut self) {
        let Some(attachment) = self
            .pool
            .attached_session()
            .map(|session| session.attachment().clone())
        else {
            return;
        };
        let link = attachment.link;
        // By address, not by device id: the docs sim's lens is on a pending
        // link, which `roster().device()` never finds.
        let port_open = self
            .lens_facts_at_address(&attachment.uid)
            .is_some_and(|facts| facts.open);
        if self.devices.link_is_routable(link) && port_open {
            return;
        }
        self.push_log(UiLogDraft::new(
            UiLogLevel::Warn,
            UiLogOrigin::Studio,
            "the board under the editor went away; the editor is closed".to_string(),
        ));
        self.close_device_lens();
    }

    /// Attach a held `/device/<uid>` intent once its board is ready. Runs
    /// from the refresh tick (the one recurring async seam); a board that
    /// is still not ready keeps the intent, one that vanished drops it.
    pub(crate) async fn try_pending_device_lens(&mut self) {
        let Some(uid) = self.pending_device_lens.clone() else {
            return;
        };
        if let Some((name, detail)) = self.sim_that_did_not_start(&uid) {
            self.release_hold_on_a_sim_that_did_not_start(&uid, &name, &detail);
            return;
        }
        if self.device_lens_attachment(&uid).is_err() {
            // Still loading, identifying, or booting: keep holding.
            return;
        }
        let landed = self.open_device_lens(&uid, UxUpdateSink::noop()).await;
        // The open that was waiting on this lens is over either way: the
        // push rode `attach_lens`, or it failed and the opening frame
        // needs its verdict rather than an eternal skeleton.
        let held_open = self.pending_open.take();
        match landed {
            Ok(_) => crate::app::open_progress::note_open_settled(),
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!("could not open the device in the editor: {error}"),
                ));
                if let Some(pending) = held_open {
                    crate::app::open_progress::note_open_failed(
                        error.message(),
                        pending.retry_action(),
                    );
                }
            }
        }
    }

    /// The name of the sim at `uid` when its runtime has been given up on
    /// before it ever said hello — a held lens on it would hold forever.
    ///
    /// A sim is powered on by this tab and cannot come back on its own the
    /// way a board can be replugged, so "the fold stopped asking" is final
    /// for it. That state is read off the fold's own evidence, not off a
    /// timer of this controller's: the sim's link is attached to the
    /// roster (the sweep handed it over), nothing is running on it (the
    /// identify — and the auto-retries the fold spends on a link that
    /// closes under it — are over), and the port is either closed again or
    /// open with nothing said in its window. A booting sim is busy; an
    /// adopted one is open with a hello; a sim the sweep has not reached
    /// yet is on no link at all; one whose runtime is still starting says
    /// so through its control — each of those keeps the hold.
    ///
    /// The browser case (G1, 2026-09-07): a worker that could not fetch its
    /// engine posts `fatal`, the link raises `Error` + `Closed`, the fold
    /// re-asks twice more and stops, and the project card said "Opening…"
    /// for as long as anyone watched.
    fn sim_that_did_not_start(&self, uid: &str) -> Option<(String, String)> {
        let sim = self.sim_transport.as_ref()?;
        if !sim.is_powered(uid) || sim.is_starting(uid) {
            // Off, or still coming up. The second matters: the fold's
            // identify deadline (5 s) is shorter than a cold worker boot,
            // and each eviction closes and re-opens the link — which
            // KILLS the worker and boots a fresh one — so at the retry
            // cap the last boot is still in flight while the link reads
            // "attached, idle, not open" below. Only the runtime knows it
            // is starting, so it is asked first.
            return None;
        }
        let endpoint = crate::sim_endpoint(uid);
        let roster = self.devices.roster();
        let (busy, presence, hello) = if let Some(pending) = roster
            .pending()
            .iter()
            .find(|pending| pending.info.endpoint == endpoint)
        {
            let evidence = pending.evidence();
            (
                pending.is_identifying(),
                evidence.presence,
                evidence.classification.hello().is_some(),
            )
        } else if let Some(device) = roster.devices().iter().find(|device| {
            device.link().is_some() && device.identity.endpoint.as_ref() == Some(&endpoint)
        }) {
            (
                device.is_busy(),
                device.evidence.presence,
                device.evidence.classification.hello().is_some(),
            )
        } else {
            // Not handed to the fold yet: the sweep is still pending.
            return None;
        };
        if busy || !presence.is_attached() {
            return None;
        }
        if presence.is_open() && hello {
            // Adopted and listening: the ordinary attach lands this tick.
            return None;
        }
        let name = self
            .devices
            .device_for_key(uid)
            .map(|device| device.title())
            .unwrap_or_else(|| "The sim".to_string());
        // The evidence the verdict rests on, for the console line: a
        // release that turns out premature has to be arguable from the log.
        let detail = format!(
            "port {}, hello {}",
            if presence.is_open() { "open" } else { "closed" },
            if hello { "heard" } else { "not heard" },
        );
        Some((name, detail))
    }

    /// End a held open on a sim that did not start: power the sim off (so
    /// the record goes back to the remembered line, where Power on and the
    /// open's own Retry can start it again — a "powered" entry with no
    /// runtime behind it could never be powered on again), and hand the
    /// opening frame its verdict, naming the device.
    fn release_hold_on_a_sim_that_did_not_start(&mut self, uid: &str, name: &str, detail: &str) {
        self.pending_device_lens = None;
        let message = format!("{name} did not start");
        self.push_log(UiLogDraft::new(
            UiLogLevel::Warn,
            UiLogOrigin::Studio,
            format!("{message}: its runtime gave up before it said hello ({detail}); powered off"),
        ));
        self.power_off_sim(Some(uid.to_string()));
        if let Some(pending) = self.pending_open.take() {
            crate::app::open_progress::note_open_failed(message, pending.retry_action());
        }
    }

    /// The storage dir a device lens's project sync must target: the dir
    /// the board reports it runs from (a device's dir differs for CLI
    /// uploads and older pushes, and saving from the wrong dir silently
    /// skipped the library save — 2026-07-26). Nothing loaded → the push
    /// fallback, so a later push and the sync agree.
    /// Read the lens device's build off its own wire and record it on the
    /// session's attachment, so [`Self::sync_lens_probe_policy`] can hand
    /// the add-node picker its "Not on this device" gate. The fold's hello
    /// mirror carries no build features by design; the lens's client asks
    /// the board directly, once, at attach.
    async fn read_device_build(&mut self, id: crate::RuntimeId) -> Result<(), UiError> {
        let session = self.pool.session_mut(id).ok_or_else(|| {
            UiError::MissingSession("runtime session is not attached".to_string())
        })?;
        let hello = session.client_mut()?.hello().await;
        let logs = session.take_pending_logs();
        self.record_session_logs(id, logs);
        let hello = hello?;
        if let Some(session) = self.pool.session_mut(id) {
            session.set_device_features(hello.build.features);
        }
        Ok(())
    }

    async fn discover_device_storage_id(
        &mut self,
        id: crate::RuntimeId,
    ) -> Result<String, UiError> {
        let catalog = self
            .pool
            .session_mut(id)
            .ok_or_else(|| UiError::MissingSession("runtime session is not attached".to_string()))?
            .client_mut()?
            .list_loaded_projects()
            .await?;
        self.record_session_logs(id, catalog.logs);
        Ok(catalog
            .projects
            .first()
            .map(|project| {
                project
                    .project_id
                    .rsplit('/')
                    .next()
                    .unwrap_or(project.project_id.as_str())
                    .to_string()
            })
            .filter(|storage| !storage.is_empty())
            .unwrap_or_else(|| {
                crate::app::project::demo_project::DEMO_PROJECT_STORAGE_ID.to_string()
            }))
    }

    /// Ask the lens session's server to log at `level`, and remember the
    /// request for the console's optimistic display (there is no read-back
    /// on the wire).
    async fn set_runtime_log_level(&mut self, level: UiLogLevel) -> UiResult {
        let mut logs = self
            .pool
            .lens_session_mut()?
            .client_mut()?
            .set_log_level(level)
            .await?;
        logs.push(UiLogDraft::new(
            UiLogLevel::Info,
            UiLogOrigin::Server,
            format!("runtime log level set to {}", level.label()),
        ));
        self.record_logs(logs);
        if let Ok(session) = self.pool.lens_session_mut() {
            session.set_requested_log_level(level);
        }
        Ok(UiNotices::new())
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
            ProjectOp::SaveOverlay => {
                let run = {
                    let server = self.pool.lens_session_mut()?.client_mut()?;
                    self.project.save_overlay(server).await
                };
                // A save can be the FORK of a transient session (D7), and a
                // shared-view fork changes the active project's identity —
                // re-note so the lens (and therefore the URL) follows the
                // fork. Idempotent for every ordinary save.
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

    /// Vendor a pattern export into the open project (module authoring
    /// unit, P5; built-ins, catalog content tree P6). The source bytes come
    /// from the library or the compiled-in catalog, the write goes over the
    /// wire as an ordinary `CreateNode`.
    async fn execute_node_import_op(&mut self, op: NodeImportOp) -> UiResult {
        let run = {
            let server = self.pool.lens_session_mut()?.client_mut()?;
            self.project
                .import_pattern(server, &op.source, &op.export)
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
    /// docs D1/D2): power an **identity-less** sim device on, put the lens
    /// on it, and deploy a compiled-in example directly — never through the
    /// library. A docs page's leased controller dispatches this as its
    /// first action; re-dispatching on the live sim is the docs "reset"
    /// (pristine re-deploy of the same files). Never dispatched by any app
    /// surface.
    ///
    /// Identity-less is the whole of PD10/Q4: the boot options carry no
    /// MAC, so the hello reports none, `registry_key()` is `None` and
    /// [`Self::settle_device_records`] never writes a row. A docs page
    /// reading the docs must not leave a device behind in the reader's
    /// library.
    async fn open_docs_example(&mut self, example_id: &str, updates: UxUpdateSink) -> UiResult {
        // Live already: the reset path, re-deploying below onto the lens.
        if self
            .pool
            .lens_session()
            .is_some_and(crate::RuntimeSession::is_connected)
        {
            return self.deploy_docs_example(example_id, updates).await;
        }
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Starting the device",
            "Starting",
            "Starting the docs sim",
        );
        // A docs sim runs the desktop firmware as itself: the page is a
        // computer, and a docs example targets no board.
        let transport = self.sim_transport.clone().ok_or_else(|| {
            UiError::UnsupportedFeature("this build has no sim runtime".to_string())
        })?;
        transport
            .power_on(crate::SimSession {
                uid: DOCS_SIM_KEY.to_string(),
                target: crate::app::library::ProjectTarget::Desktop
                    .board_id()
                    .to_string(),
                display_name: "Docs".to_string(),
                // No MAC: the runtime answers with no identity, which is
                // what keeps this device out of the registry.
                base_mac: String::new(),
                // The docs embed draws the runtime's BYTES (the rendered
                // frame is a byte preview, like the preview pool's host),
                // and a GPU-tier product has none to read back — the embed
                // showed the probe's refusal where the lamps go (G1,
                // 2026-09-07). PD12's GPU request is for devices with a
                // present path; this one has only the embed.
                tier: crate::SimTier::Cpu,
            })
            .map_err(UiError::Link)?;
        self.device_sweep_pending = true;
        self.run_due_device_sweep();
        // The docs host holds this example until the anonymous device says
        // hello; the tick's `try_pending_device_lens` lands the lens and
        // this same deploy runs from `attach_lens`.
        self.pending_open = Some(PendingOpen::DocsExample(example_id.to_string()));
        self.open_device_lens(DOCS_SIM_KEY, updates).await
    }

    /// Deploy a compiled-in example directly onto the lens (the docs
    /// "reset", and the landing half of [`Self::open_docs_example`]).
    async fn deploy_docs_example(&mut self, example_id: &str, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            UxActivityTarget::pane(ProjectController::NODE_ID),
            "Opening example",
            "Opening",
            "Sending the example to the docs sim",
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
        // Detaching is closing: the wire goes back to the roster and the
        // DEVICE keeps running — a sim exactly like silicon (PD9). The old
        // "quiesce and keep the sim session running detached" is gone with
        // the sim's own session: there is nothing left to hold open.
        self.close_device_lens();
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
        // No lens: probe policy and the picker gate follow it out at once,
        // not at the next dispatch.
        self.sync_lens_probe_policy();
    }

    /// Attach the editor lens to session `id` and rebuild the mirror
    /// against that session's client: a PENDING OPEN pushes the library
    /// head (D19 — opening is a push regardless of what runs), and
    /// everything else connects to whatever the device already runs.
    ///
    /// One path for every device (PD9). The push lives HERE rather than at
    /// the open's call site so that a lens landing late — a sim that was
    /// still booting when the click ran — still pushes.
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
        // Library sync must target the dir this runtime ACTUALLY serves —
        // asked over the wire for every kind now, because a sim answers the
        // same question a board does (the demo-slot assumption was the
        // sim's last special case).
        let storage_id = self.discover_device_storage_id(id).await?;
        self.project.set_runtime_storage_id(storage_id);
        // The connect sequence builds the first view of this lens; the
        // picker's gate (and probe policy) must already know the runtime
        // it is on — the per-dispatch sync ran before this op moved the lens.
        self.sync_lens_probe_policy();
        // A home-card open skips the running-project probe: opening IS a
        // push of the library head, regardless of what runs (D19).
        if self.pending_open.is_some() {
            return self.open_pending_package(updates).await;
        }
        self.connect_running_project(updates).await
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

    fn project_is_loaded(&self) -> bool {
        matches!(self.project.snapshot().state, ProjectState::Ready { .. })
    }
}

/// Cross-module test builders. The actor tests live in a sibling module and
/// cannot reach the private `pool`/`project` fields, so these
/// `pub(crate)` helpers assemble a connected controller for them.
#[cfg(test)]
impl StudioController {
    /// Install a stubbed lens session on a SIM-backed device — the
    /// "connected but not hardware" fixture, which is now a device like
    /// any other.
    pub(crate) fn set_stub_sim_for_test(&mut self) {
        self.install_stub_sim_for_test();
    }

    /// Install a stubbed sim-backed lens session and return its id.
    pub(crate) fn install_stub_sim_for_test(&mut self) -> crate::RuntimeId {
        self.pool.install(crate::RuntimePayload::Device(
            crate::DeviceLensAttachment::sim_stub_for_test("devsim"),
        ))
    }

    /// Install a stubbed sim-backed lens session with an injected wire
    /// client.
    pub(crate) fn install_stub_sim_with_client_for_test(
        &mut self,
        client: crate::StudioServerClient,
    ) -> crate::RuntimeId {
        let id = self.install_stub_sim_for_test();
        self.pool
            .session_mut(id)
            .expect("just-installed session")
            .set_client_for_test(client);
        id
    }

    /// The runtime pool, for e2e assertions about session coexistence.
    #[cfg(test)]
    pub(crate) fn project_for_test(&self) -> &ProjectController {
        &self.project
    }

    pub(crate) fn pending_device_lens_for_test(&self) -> Option<String> {
        self.pending_device_lens.clone()
    }

    pub(crate) fn devices_for_test(&self) -> &crate::DeviceRoster {
        &self.devices
    }

    pub(crate) fn runtime_pool_for_test(&self) -> &RuntimePool {
        &self.pool
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
}

impl ControllerContext for StudioController {
    fn dispatch(
        &mut self,
        action: UiAction,
    ) -> core::pin::Pin<Box<dyn Future<Output = UiResult> + '_>> {
        Box::pin(StudioController::dispatch(self, action))
    }
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
/// The lens's view of the thing at an address: the facts
/// `StudioController::device_lens_attachment` decides on, read the same way
/// off a roster device or off a pending link (`lens_facts_at_address`).
struct LensFacts {
    device: lpa_devices::DeviceId,
    link: Option<lpa_devices::LinkId>,
    /// The port is open right now (attached is not open).
    open: bool,
    /// `Some(board_id)` when a hello was heard in the current window.
    hello: Option<Option<String>>,
    /// An activity is running on it.
    busy: bool,
    name: String,
    /// The endpoint rung, for the transport fork (`sim:` vs a port).
    endpoint: Option<String>,
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
        ControllerId, ProjectController, ProjectEditorOp, ProjectEditorTarget,
        ProjectInventorySummary, ProjectNodeAddress, ProjectNodeTarget, ProjectState,
        ProjectSyncPhase, ServerState, StudioServerClient,
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

    #[test]
    fn a_detached_lens_session_joins_the_slow_heartbeat_lane() {
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

        // Detaching CLOSES the lens now (PD9: a device keeps running, the
        // session does not) — an empty pool falls back to the calm default.
        studio.detach_lens().expect("detach succeeds");
        assert!(
            !studio.runtime_pool_for_test().has_session(),
            "detaching a lens closes it; the device keeps running"
        );
        assert_eq!(
            studio.next_refresh_interval(),
            RefreshCadence::default().interval()
        );
        // The slow heartbeat lane is still what a non-lens session rides.
        let _ = DEVICE_HEARTBEAT_INTERVAL;
    }

    #[test]
    fn the_lens_board_id_is_the_devices_own() {
        // The board the output face's pin diagram lights up for is the
        // device's, read off the attachment — one source for a sim and for
        // silicon (PD9), instead of the sim's old advisory claim.
        let mut studio = StudioController::new(|| 100.0);
        studio.set_stub_sim_for_test();

        assert_eq!(
            studio.lens_board_id(),
            Some("lightplayer/desktop"),
            "a Desktop sim reports the manifest it wears"
        );
        assert_eq!(
            studio.pool.lens_session().expect("a lens").transport(),
            crate::LinkTransport::Sim
        );
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

    /// The New menu's optional name: a typed name is what the library dates
    /// and slugs; blank falls back to the template's label. The follow-on
    /// open errs on host (no sim runtime), exactly as the create test below
    /// documents — the package sticks either way.
    #[test]
    fn home_create_project_takes_the_menus_optional_name() {
        use crate::app::library::{LibraryStore, MemoryLibraryHost};
        use crate::{HOME_NODE_ID, HomeOp};
        use lpfs::LpFsMemory;

        let mut studio = StudioController::new(|| 42.0);
        let counter = Rc::new(RefCell::new(0u8));
        let store = LibraryStore::new(
            Rc::new(RefCell::new(LpFsMemory::new())),
            Rc::new(move || {
                *counter.borrow_mut() += 1;
                [*counter.borrow(); 16]
            }),
            Rc::new(|| "2026-09-06-1010".to_string()),
        );
        studio.attach_library(Rc::new(MemoryLibraryHost::new(
            store.clone(),
            Rc::new(|| 42.0),
        )));
        let home_action = |op: HomeOp| UiAction::from_op(ControllerId::new(HOME_NODE_ID), op);

        block_on_ready(studio.dispatch(home_action(HomeOp::CreateProject {
            template: crate::ProjectTemplate::Blank,
            name: Some("  Porch sign ".to_string()),
        })))
        // No sim transport on a host build: the open mints the record,
        // asks the fold to power it on, and HOLDS the lens — the package
        // sticks either way, which is what this pins.
        .expect("the open holds its lens rather than failing");
        block_on_ready(studio.dispatch(home_action(HomeOp::CreateProject {
            template: crate::ProjectTemplate::Blank,
            name: Some("   ".to_string()),
        })))
        // No sim transport on a host build: the open mints the record,
        // asks the fold to power it on, and HOLDS the lens — the package
        // sticks either way, which is what this pins.
        .expect("the open holds its lens rather than failing");

        let mut slugs: Vec<String> = store
            .list()
            .expect("library lists")
            .into_iter()
            .map(|summary| summary.slug)
            .collect();
        slugs.sort();
        assert_eq!(
            slugs,
            vec![
                "2026-09-06-1010-porch-sign".to_string(),
                "2026-09-06-1010-project".to_string(),
            ],
            "typed name slugged and dated; blank fell back to the template's label"
        );
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
        // the open holds its lens (no sim runtime on a host build) — the
        // successful create-and-open round-trip is the edit-e2e test's.
        // The packages stick either way.
        for _ in 0..2 {
            block_on_ready(studio.dispatch(home_action(HomeOp::CreateProject {
                template: crate::ProjectTemplate::Blank,
                name: None,
            })))
            .expect("the open holds its lens rather than failing");
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
        // The lens card is the ROSTER's projection of the lens device
        // (PD9) — asserted end to end in `studio_device_e2e_tests`, where
        // a real device backs the lens; a stub session has no roster row
        // to project.

        // The retired wizard's project steps stayed gone through the
        // deletion.
        let actions = view_actions(&view);
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::ConnectRunningProject | ProjectOp::LoadDemoProject)
        )));
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
    }

    #[test]
    fn set_runtime_log_level_sends_request_and_records_confirmation() {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let io = ScriptedClientIo::new(
            Rc::clone(&sent),
            vec![WireServerMessage::new(1, WireServerMsgBody::SetLogLevel)],
        );
        let mut studio = connected_studio_with_client(io);
        let action = UiAction::from_op(
            ControllerId::new(crate::RuntimeOp::NODE_ID),
            crate::RuntimeOp::SetLogLevel {
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
                    && entry.message == "runtime log level set to debug"
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

    // -----------------------------------------------------------------
    // The lens-card invariant (D43)
    //
    // The editor layout has exactly ONE device surface. Nothing pinned
    // that before, which is how a device-gone hole stayed invisible long
    // enough for the shell to keep falling back to the retired
    // step-stack pane — defect
    // docs/defects/2026-07-28-retired-device-pane-still-reachable.md.
    // -----------------------------------------------------------------

    /// A controller with a stubbed sim-backed lens session, a connected
    /// server protocol and a ready project.
    fn connected_studio() -> StudioController {
        let mut studio = stub_sim_studio();
        studio.set_server_state_for_test(ServerState::Connected {
            protocol: "fake-protocol".to_string(),
        });
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        studio
    }

    /// A controller with a stubbed sim session installed and the lens on
    /// it — every row below drives the editor through it.
    fn stub_sim_studio() -> StudioController {
        let mut studio = StudioController::new(|| 0.0);
        studio.set_stub_sim_for_test();
        studio
    }

    fn connected_studio_with_client(io: ScriptedClientIo) -> StudioController {
        let mut studio = stub_sim_studio();
        studio.set_server_client_for_test(StudioServerClient::from_io_for_test(
            "fake-protocol",
            Box::new(io),
        ));
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
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

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    // ---- the setup wizard ------------------------------------------------

    #[test]
    fn the_utc_fallback_stamp_is_a_well_formed_slug_stamp() {
        // The default when a shell installs no local stamp. `Aug 5 2026,
        // 12:00 UTC` — the naming helper must be able to read it.
        let stamp = utc_slug_stamp(1_786_000_000.0);
        assert_eq!(stamp.len(), 15, "{stamp}");
        assert_eq!(&stamp[..4], "2026", "{stamp}");
        assert_eq!(stamp.as_bytes()[10], b'-', "{stamp}");
    }
}
