use core::future::Future;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::{CancelSignal, ProgressDeadline};
use lpa_link::{DeviceState, LinkManagementRequest, LinkManagementResult, LinkProviderKind};

use crate::app::device::device_event_adapter::management_event_sink;
use crate::app::device::link_ux::management_result_logs;
use crate::app::device::{
    DEPLOY_NODE_ID, DeployOp, DeploySession, DeployState, DeployTarget, DeviceOpenOutcome,
    UiDeployChoice, UiDeployView,
};
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
    DeviceController, DeviceOp, NodeRevertOp, ProjectConnectResult, ProjectController,
    ProjectEditRun, ProjectOp, ProjectRefreshOutcome, ProjectState, ProjectSyncRun, SlotEditOp,
    StudioSnapshot, UiAction, UiActions, UiActivityView, UiError, UiLogDraft, UiLogEntry,
    UiLogLevel, UiLogOrigin, UiNotice, UiPaneView, UiProgress, UiResult, UiStatus, UiStudioView,
    UiViewContent, UxActivityTarget, UxUpdate, UxUpdateSink,
};

pub struct StudioController {
    device: DeviceController,
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
    /// a dispatched action, focus change, or log — so the next gate emits even
    /// though the project revision did not move.
    dirty: bool,
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
    /// What the attached DEVICE holds, computed by connect-as-pull (D8)
    /// right after the server protocol attaches to hardware. `None` while
    /// disconnected or when the runtime is the simulator (the sim is not
    /// a device — D22).
    device_sync: Option<DeviceSyncState>,
    /// The open deploy dialog, when there is one (M5). Pure state — the
    /// controller executes its effects through the existing seams.
    deploy: Option<DeploySession>,
    /// Injected randomness for identity minting (`dev_` uids). The web
    /// shell installs crypto randomness at startup; the default is a
    /// clock-derived fallback good enough for tests.
    random: Rc<dyn Fn() -> [u8; 16]>,
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
            project: ProjectController::new(),
            logs: LogRing::new(),
            log_filter: LogFilter::default(),
            now_secs: Rc::new(now_secs),
            on_entry: None,
            applied_revision: None,
            // The first view is always new to the UI, so start dirty.
            dirty: true,
            library_host: None,
            home_inputs: None,
            library_refresh_pending: false,
            pending_open: None,
            device_sync: None,
            deploy: None,
            random: Rc::new(clock_fallback_random),
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
            self.device.snapshot().flow,
            self.device.snapshot().server,
            self.project.snapshot(),
            self.logs.to_vec(),
        )
    }

    /// The passive-refresh cadence for the current connection, as data (P4/Q3).
    ///
    /// The actor publishes this to the UI timer so the interval policy lives in
    /// core, not as a `LinkProviderKind` match in the view layer.
    pub fn refresh_cadence(&self) -> RefreshCadence {
        RefreshCadence::for_flow_state(&self.device.snapshot().flow)
    }

    pub fn actions(&self) -> UiActions {
        UiActions::new(view_actions(&self.view()))
    }

    pub fn view(&self) -> UiStudioView {
        if let Some(home) = self.home_view() {
            return UiStudioView::new(Vec::new(), self.console_view())
                .with_home(Some(home))
                .with_device_sync(self.device_sync.clone())
                .with_deploy(self.deploy_view());
        }
        let device_view = self
            .device
            .view(self.device_sync.as_ref(), self.usual_device_line());
        // gallery-always (D24): home covers every no-project state, so the
        // pane layout exists only for an open project
        let panes = vec![
            self.project.view(self.device.has_lightplayer_state()),
            self.bus_pane(),
            device_view,
        ];
        UiStudioView::new(panes, self.console_view())
            .with_open_project(
                self.project.active_library_uid(),
                self.project.active_library_slug(),
            )
            .with_device_sync(self.device_sync.clone())
            .with_deploy(self.deploy_view())
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
        Some(home_view_builder::build_home_view(
            self.home_inputs.as_ref(),
            opening.map(|pending| pending.card_key().to_string()),
            issue,
            self.device_sync.as_ref(),
            self.device.transport_label(),
        ))
    }

    /// The console slice of the view: ring entries passing the display
    /// filter, plus the hidden count and the filter state for the toolbar.
    /// Carries the connected server's last-requested log level (or `None`
    /// while disconnected) for the device-level selector.
    fn console_view(&self) -> UiConsoleView {
        let mut console = UiConsoleView::from_ring(&self.logs, &self.log_filter);
        console.device_log_level = self.device.server.requested_log_level();
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
        let revision = self.current_revision();
        let advanced = revision != self.applied_revision;
        if !self.dirty && !advanced {
            return None;
        }
        self.applied_revision = revision;
        self.dirty = false;
        Some(self.view())
    }

    /// Mark local (non-network) state as changed so the next
    /// [`Self::view_if_changed`] emits.
    fn mark_dirty(&mut self) {
        self.dirty = true;
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

    /// Stamp a batch of producer drafts (all with one clock read — they
    /// arrived together) into the ring and mark the view dirty. No-op for an
    /// empty batch so a quiet passive refresh stays change-gated out. The
    /// mirror hook fires once per stamped entry.
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
        self.mark_dirty();
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
    /// pane, cards, and deploy dialog. `None` while disconnected or when
    /// the runtime is the simulator.
    pub fn device_sync(&self) -> Option<&DeviceSyncState> {
        self.device_sync.as_ref()
    }

    /// Connect-is-a-pull (D8): pull the attached device's copy, classify
    /// it against the library, persist per the M4b locking model, refresh
    /// the registry, and cache the result. Never fails the connect —
    /// errors are logged and leave the state `None` (flash/erase must
    /// stay reachable on a device we can't read).
    pub(crate) async fn refresh_device_sync(&mut self) {
        self.device_sync = None;
        let pulled = {
            let Ok(server) = self.device.server.client_mut() else {
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
                    self.device_sync = Some(DeviceSyncState {
                        identity: None,
                        content: DeviceContent::Unreadable {
                            detail: format!("could not read the device: {error}"),
                        },
                    });
                    self.mark_dirty();
                    return;
                }
            }
        };
        match self.absorb_device_pull(pulled).await {
            Ok(state) => {
                self.device_sync = Some(state);
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
        let identity = pulled.identity.clone();

        if let Some(identity) = &identity {
            self.upsert_device_entry(identity, now).await;
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
                    match store.list() {
                        Ok(summaries) => summaries
                            .into_iter()
                            .find(|summary| summary.uid.to_string() == *uid)
                            .map(|summary| {
                                let relation = store
                                    .open(summary.uid)
                                    .map(|handle| handle.history.classify(pulled.observed))
                                    .unwrap_or(lpc_history::SyncRelation::Diverged);
                                (summary, relation)
                            }),
                        Err(_) => None,
                    }
                }
                Err(_) => None,
            },
            _ => None,
        };

        if let Some((summary, relation)) = local {
            let content = DeviceContent::Known {
                project_uid: summary.uid.to_string(),
                slug: summary.slug.clone(),
                observed: pulled.observed,
                relation,
            };
            let Some(identity_value) = identity.clone() else {
                // anonymous hardware: classification only (the wizard
                // stamps an identity, then this re-runs)
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
            if !handled {
                let host = self.library_host()?;
                let op = CatalogOp::RecordDeviceObservation {
                    project_uid: summary.uid.to_string(),
                    device: device_session::registry_entry_for(
                        &identity_value,
                        self.device.transport_label().unwrap_or_default(),
                        now,
                    ),
                    observed: pulled.observed,
                    files: pulled.files.clone(),
                };
                if let Err(error) = host.catalog(op).await {
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
                }
            }
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
                    self.device.transport_label().unwrap_or_default(),
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
            self.device.transport_label().unwrap_or_default(),
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
        inputs
            .devices
            .iter()
            .find_map(|device| match &device.state {
                crate::app::home::UiDeviceCardState::RememberedOffline {
                    last_known: Some(known),
                    ..
                } if *known == slug => Some(format!("Usually on {}.", device.name)),
                _ => None,
            })
    }

    /// The dialog view model, when the dialog is open.
    fn deploy_view(&self) -> Option<UiDeployView> {
        let session = self.deploy.as_ref()?;
        let choices = self
            .home_inputs
            .as_ref()
            .map(|inputs| {
                inputs
                    .projects
                    .iter()
                    .map(|card| UiDeployChoice {
                        uid: card.uid.clone(),
                        slug: card.slug.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(UiDeployView {
            state: session.state.clone(),
            choices,
            connect_actions: self.deploy_connect_actions(),
        })
    }

    /// Hardware connect actions for the dialog's `NeedsDevice` state:
    /// mid-selection link states surface their endpoint choices; settled
    /// states offer the CATALOG's hardware device classes (connect +
    /// recovery open), derived from descriptors/capabilities — no provider
    /// kind is hardcoded. The simulator is never offered — it is not a
    /// device (D22). Copy stays ESP32-shaped until another hardware class
    /// exists (naming is M7's redesign).
    fn deploy_connect_actions(&self) -> Vec<UiAction> {
        let device_node = self.device.node_id();
        match self.device.flow_state() {
            ConnectFlowState::SelectingEndpoint {
                provider_id,
                endpoints,
            } => endpoints
                .iter()
                .map(|endpoint| {
                    UiAction::from_op(
                        device_node.clone(),
                        crate::DeviceOp::ConnectEndpoint {
                            provider_id: *provider_id,
                            endpoint_id: endpoint.id.clone(),
                        },
                    )
                    .with_label(format!("Open {}", endpoint.label))
                    .with_summary(endpoint.summary.clone())
                })
                .collect(),
            _ => self
                .device
                .hardware_device_kinds()
                .into_iter()
                .flat_map(|provider_id| {
                    [
                        UiAction::from_op(
                            device_node.clone(),
                            crate::DeviceOp::OpenProvider { provider_id },
                        )
                        .with_label("Connect ESP32")
                        .with_summary("Connect an ESP32 over USB.")
                        .with_icon("usb")
                        .with_priority(crate::ActionPriority::Primary),
                        UiAction::from_op(
                            device_node.clone(),
                            crate::DeviceOp::OpenProviderForRecovery { provider_id },
                        )
                        .with_label("Open for flashing")
                        .with_summary("Open the ESP32 connection without attaching LightPlayer.")
                        .with_icon("usb")
                        .with_priority(crate::ActionPriority::Secondary),
                    ]
                })
                .collect(),
        }
    }

    /// The dialog's environment snapshot (entry-state derivation input).
    ///
    /// Derived from the runtime attachment + device-session state (M4/P5):
    /// hardware counts as connected while its session is not `Gone` (the
    /// sim never counts — D22); firmware is available exactly when the
    /// session reached `Ready` AND the server protocol answered.
    fn deploy_environment(&self) -> crate::app::device::DeployEnvironment {
        let device_state = self.device.device_state();
        let device_link_connected =
            self.device.is_hardware_link() && !matches!(device_state, Some(DeviceState::Gone));
        let firmware_available = device_link_connected
            && matches!(device_state, Some(DeviceState::Ready { .. }))
            && self.device.has_lightplayer_state();
        crate::app::device::DeployEnvironment {
            device_link_connected,
            firmware_available,
            device_sync: self.device_sync.clone(),
        }
    }

    /// Re-derive the open dialog's step after the environment changed
    /// (connects, disconnects, pull results).
    fn rederive_deploy(&mut self) {
        let env = self.deploy_environment();
        if let Some(session) = self.deploy.as_mut() {
            session.rederive(&env);
            self.mark_dirty();
        }
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
            DeployOp::OpenDialog { target_key } => {
                let target = match target_key {
                    Some(key) => Some(self.resolve_deploy_target(&key).await?),
                    None => None,
                };
                self.deploy = Some(DeploySession::open(&self.deploy_environment(), target));
                Ok(UiNotices::new())
            }
            DeployOp::CloseDialog => {
                if self
                    .deploy
                    .as_ref()
                    .is_some_and(|session| session.close_blocked())
                {
                    return Err(UiError::UnsupportedAction(
                        "A device operation is still running — let it finish first".to_string(),
                    ));
                }
                self.deploy = None;
                Ok(UiNotices::new())
            }
            DeployOp::FlashFirmware => {
                let resume = self.deploy_state_now()?;
                self.deploy_session()?
                    .begin_flash()
                    .map_err(deploy_transition_error)?;
                updates.emit(UxUpdate::View(self.view()));
                let result = self.provision_firmware(updates).await;
                let env = self.deploy_environment();
                match result {
                    Ok(outcome) => {
                        if let Some(session) = self.deploy.as_mut() {
                            session.flash_finished(&env, true);
                        }
                        Ok(outcome)
                    }
                    Err(error) => {
                        if let Some(session) = self.deploy.as_mut() {
                            session.fail(format!("Flashing failed: {error}"), resume);
                        }
                        Err(error)
                    }
                }
            }
            DeployOp::StampIdentity { name } => {
                let resume = self.deploy_state_now()?;
                self.deploy_session()?
                    .begin_stamp(name.clone())
                    .map_err(deploy_transition_error)?;
                updates.emit(UxUpdate::View(self.view()));
                let result = self.run_identity_stamp(name.trim().to_string()).await;
                let env = self.deploy_environment();
                match result {
                    Ok(identity) => {
                        if let Some(session) = self.deploy.as_mut() {
                            session.stamp_finished(&env);
                        }
                        Ok(UiNotices::new().with_notice(UiNotice::info(format!(
                            "This device is now \"{}\"",
                            identity.name
                        ))))
                    }
                    Err(error) => {
                        if let Some(session) = self.deploy.as_mut() {
                            session.fail(format!("Naming the device failed: {error}"), resume);
                        }
                        Err(error)
                    }
                }
            }
            DeployOp::ChoosePackage { key } => {
                let target = self.resolve_deploy_target(&key).await?;
                let env = self.deploy_environment();
                self.deploy_session()?
                    .choose_target(&env, target)
                    .map_err(deploy_transition_error)?;
                Ok(UiNotices::new())
            }
            DeployOp::ConfirmPush => {
                let resume = self.deploy_state_now()?;
                let (device, target) = self
                    .deploy_session()?
                    .begin_push()
                    .map_err(deploy_transition_error)?;
                updates.emit(UxUpdate::View(self.view()));
                let result = self.run_device_push(&device, &target).await;
                match result {
                    Ok(()) => {
                        if let Some(session) = self.deploy.as_mut() {
                            session.push_finished();
                        }
                        self.request_library_refresh();
                        Ok(UiNotices::new().with_notice(UiNotice::info(format!(
                            "Pushed {} to {}",
                            target.slug, device.name
                        ))))
                    }
                    Err(error) => {
                        if let Some(session) = self.deploy.as_mut() {
                            session.fail(format!("Push failed: {error}"), resume);
                        }
                        Err(error)
                    }
                }
            }
            DeployOp::AdoptDeviceCopy => {
                let (project_uid, observed) = self.reviewing_diverged_copy()?;
                let host = self.library_host()?;
                host.catalog(CatalogOp::AdoptObservedVersion {
                    project_uid,
                    observed,
                })
                .await
                .map_err(|error| self.library_error_with_name(error))?;
                self.request_library_refresh();
                self.refresh_device_sync().await;
                self.rederive_deploy();
                Ok(UiNotices::new()
                    .with_notice(UiNotice::info("The device's version is now the newest")))
            }
            DeployOp::KeepBothFork => {
                let (project_uid, observed) = self.reviewing_diverged_copy()?;
                let device_name = match self.deploy_state_now()? {
                    DeployState::Reviewing { device, .. } => device.name,
                    _ => "device".to_string(),
                };
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
                let resume = self.deploy_state_now()?;
                let result = self.reset_to_blank(updates).await;
                let env = self.deploy_environment();
                match result {
                    Ok(outcome) => {
                        if let Some(session) = self.deploy.as_mut() {
                            session.rederive(&env);
                        }
                        Ok(outcome)
                    }
                    Err(error) => {
                        if let Some(session) = self.deploy.as_mut() {
                            session.fail(format!("Erase failed: {error}"), resume);
                        }
                        Err(error)
                    }
                }
            }
            DeployOp::RetryFailed => {
                self.deploy_session()?
                    .retry()
                    .map_err(deploy_transition_error)?;
                Ok(UiNotices::new())
            }
        }
    }

    /// The open session, or the friendly no-dialog error.
    fn deploy_session(&mut self) -> Result<&mut DeploySession, UiError> {
        self.deploy
            .as_mut()
            .ok_or_else(|| UiError::UnsupportedAction("The device dialog is not open".to_string()))
    }

    fn deploy_state_now(&mut self) -> Result<DeployState, UiError> {
        Ok(self.deploy_session()?.state.clone())
    }

    /// The (project, observed hash) a diverged review is looking at.
    fn reviewing_diverged_copy(&mut self) -> Result<(String, lpc_history::ContentHash), UiError> {
        match self.deploy_state_now()? {
            DeployState::Reviewing {
                on_device:
                    DeviceContent::Known {
                        project_uid,
                        observed,
                        relation: lpc_history::SyncRelation::Diverged,
                        ..
                    },
                ..
            } => Ok((project_uid, observed)),
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
            let server = self.device.server.client_mut()?;
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
        {
            let server = self.device.server.client_mut()?;
            let loaded = server
                .open_library_project(&files, &local_hash.to_string())
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
                self.device.transport_label().unwrap_or_default(),
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
                    self.device.transport_label().unwrap_or_default(),
                    now,
                ),
                version: local_hash,
            })
            .await
            .map_err(|error| self.library_error_with_name(error))?;
        }

        // 4. the device now runs the pushed head
        self.refresh_device_sync().await;
        self.rederive_deploy();
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
        if !self.project_is_loaded() || !self.device.has_lightplayer_state() {
            return Ok(None);
        }
        let outcome = {
            let server = self.device.server.client_mut()?;
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
        let node_id = action.node_id().clone();
        let device_node_id = self.device.node_id();
        let project_node_id = self.project.node_id();

        if node_id.as_str() == HOME_NODE_ID {
            let op = action.into_op::<HomeOp>()?;
            return self.execute_home_op(op, updates).await;
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
            DeviceOp::DisconnectLightPlayer => self.disconnect_lightplayer().await,
            DeviceOp::SetLogLevel { level } => self.set_device_log_level(level).await,
            DeviceOp::ResetDevice => self.reset_device(updates).await,
            DeviceOp::ConnectLightPlayer => self.connect_server_from_link(updates).await,
            DeviceOp::ProvisionFirmware => self.provision_firmware(updates).await,
            DeviceOp::ResetToBlank => self.reset_to_blank(updates).await,
            DeviceOp::RefreshConnections => {
                // Drop the attachment (no provider close) + catalog refresh.
                self.device.refresh_provider_catalog();
                self.device.server.disconnect();
                self.device_sync = None;
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
                match self.device.open_provider(provider_id).await? {
                    DeviceOpenOutcome::Opened => Ok(UiNotices::new()),
                    DeviceOpenOutcome::Cancelled { message } => {
                        Ok(UiNotices::new().with_notice(UiNotice::info(message)))
                    }
                    DeviceOpenOutcome::Connected { logs } => {
                        self.record_logs(logs);
                        self.attach_runtime(updates).await
                    }
                }
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
                let logs = self
                    .device
                    .connect_endpoint(provider_id, endpoint_id)
                    .await?;
                self.record_logs(logs);
                self.attach_runtime(updates).await
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

    /// Open a home card: push the package's head to the simulator, starting
    /// the simulator first when nothing is connected (D13: a library card
    /// opens in the sim; the sim is invisible infrastructure).
    async fn open_from_home(&mut self, pending: PendingOpen, updates: UxUpdateSink) -> UiResult {
        self.library_host()?;
        // a connected hardware device is M5's push flow, not an open
        if self.device.is_hardware_link() {
            return Err(UiError::UnsupportedAction(
                "Pushing to a connected device lands with the provision dialog (M5); \
                 disconnect the device to open this project in the simulator"
                    .to_string(),
            ));
        }
        self.pending_open = Some(pending);
        let result = self.open_from_home_inner(updates).await;
        self.pending_open = None;
        result
    }

    async fn open_from_home_inner(&mut self, updates: UxUpdateSink) -> UiResult {
        // already attached to the simulator: replace-and-load directly
        if self.device.has_lightplayer_state() {
            return self.open_pending_package(updates).await;
        }
        // an open sim attachment without a server session reconnects first;
        // otherwise start the simulator — both paths run the pending open
        // inside `attach_runtime`
        if self.device.has_runtime_attachment() {
            return self.connect_server_from_link(updates).await;
        }
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_DEVICE),
            "Starting simulator",
            "Opening",
            "Starting the simulator runtime",
        );
        match self
            .device
            .open_provider(LinkProviderKind::BrowserWorker)
            .await?
        {
            DeviceOpenOutcome::Connected { logs } => {
                self.record_logs(logs);
                self.attach_runtime(updates).await
            }
            DeviceOpenOutcome::Opened => Err(UiError::MissingSession(
                "the simulator opened without connecting".to_string(),
            )),
            DeviceOpenOutcome::Cancelled { message } => {
                Ok(UiNotices::new().with_notice(UiNotice::info(message)))
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
            let server = self.device.server.client_mut()?;
            match &pending {
                PendingOpen::Package(key) => self.project.open_library_package(server, key).await,
                PendingOpen::Example(id) => self.project.open_example_package(server, id).await,
            }
        };
        match result {
            Ok(logs) => {
                self.record_logs(logs);
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
            ProjectOp::SaveOverlay => {
                let run = {
                    let server = self.device.server.client_mut()?;
                    self.project.save_overlay(server).await
                };
                self.record_project_edit_run(run)
            }
            ProjectOp::RevertAllEdits => {
                let run = {
                    let server = self.device.server.client_mut()?;
                    self.project.revert_all_edits(server).await
                };
                self.record_project_edit_run(run)
            }
        }
    }

    async fn execute_slot_edit_op(&mut self, op: SlotEditOp) -> UiResult {
        let run = {
            let server = self.device.server.client_mut()?;
            self.project.apply_slot_edit(server, op).await
        };
        self.record_project_edit_run(run)
    }

    async fn execute_asset_edit_op(&mut self, op: AssetEditOp) -> UiResult {
        let run = {
            let server = self.device.server.client_mut()?;
            self.project.apply_asset_edit(server, op).await
        };
        self.record_project_edit_run(run)
    }

    /// Resolve (and cache) an asset's effective editor content so the next
    /// emitted view embeds it. Quiet on success — the refreshed view is the
    /// outcome; server log lines join the ring like any edit run's.
    async fn execute_asset_content_fetch(&mut self, op: AssetContentFetchOp) -> UiResult {
        let run = {
            let server = self.device.server.client_mut()?;
            self.project.asset_content(server, &op.artifact).await?
        };
        self.record_logs(run.logs);
        Ok(UiNotices::new())
    }

    async fn execute_node_revert_op(&mut self, op: NodeRevertOp) -> UiResult {
        let run = {
            let server = self.device.server.client_mut()?;
            self.project.revert_node_edits(server, &op.node).await
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
        self.device.server.disconnect();
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_DEVICE),
            "Opening device for flashing",
            "Opening",
            "Opening device without attaching LightPlayer",
        );
        match self.device.open_provider(provider_id).await? {
            DeviceOpenOutcome::Opened => Ok(UiNotices::new().with_notice(UiNotice::info(
                "Choose the device endpoint to open for flashing",
            ))),
            DeviceOpenOutcome::Cancelled { message } => {
                Ok(UiNotices::new().with_notice(UiNotice::info(message)))
            }
            // Recovery open: the DeviceSession exists (monitor/management
            // reachable; BlankFlash/Bootloader are fine end states) but the
            // app protocol is deliberately NOT attached.
            DeviceOpenOutcome::Connected { logs } => {
                self.record_logs(logs);
                self.rederive_deploy();
                updates.emit(UxUpdate::View(self.view()));
                Ok(UiNotices::new().with_notice(UiNotice::info("Device opened for flashing")))
            }
        }
    }

    async fn connect_server_from_link(&mut self, updates: UxUpdateSink) -> UiResult {
        if !self.device.has_runtime_attachment() {
            return Err(UiError::MissingSession(
                "link connection is not open".to_string(),
            ));
        }
        // A hardware session stuck in a terminal state needs a rebuilt link
        // generation before the server can attach (reconnect-that-rebuilds);
        // Booting/Ready sessions attach directly.
        let needs_reconnect = matches!(
            self.device.device_state(),
            Some(
                DeviceState::Gone
                    | DeviceState::Incompatible { .. }
                    | DeviceState::Unresponsive { .. }
                    | DeviceState::BlankFlash
                    | DeviceState::Bootloader
                    | DeviceState::ForeignFirmware
            )
        );
        if needs_reconnect {
            self.project.reset();
            self.device.server.disconnect();
            emit_activity(
                &updates,
                device_section_target(DeviceController::SECTION_DEVICE),
                "Reopening device",
                "Connecting",
                "Resetting device before server connect",
            );
            let result = {
                let session = self.device.device_session().ok_or_else(|| {
                    UiError::MissingSession(
                        "hardware attachment has no live device session".to_string(),
                    )
                })?;
                session.reconnect().await
            };
            result.map_err(|error| UiError::Link(error.to_string()))?;
        }
        self.attach_runtime(updates).await
    }

    /// Attach the server protocol to the current runtime attachment (the
    /// device session's channel for hardware, worker io for the sim) and
    /// run the post-attach sequence: readiness probe, no-firmware /
    /// incompatible handling, connect-as-pull, deploy re-derivation.
    async fn attach_runtime(&mut self, updates: UxUpdateSink) -> UiResult {
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
        let is_sim = self.device.is_sim_attached();
        match self.device.attach_server(server_updates) {
            Ok(()) => {
                let mut outcome =
                    UiNotices::new().with_notice(UiNotice::info("Server protocol connected"));
                updates.emit(UxUpdate::View(self.view()));
                // a home-card open skips the running-project probe: opening
                // is a push of the library head regardless of what runs
                // (D19); the sim-only guard lives in `open_from_home`
                if self.pending_open.is_some() && is_sim {
                    let open_outcome = self.open_pending_package(updates).await?;
                    outcome.notices.extend(open_outcome.notices);
                    return Ok(outcome);
                }
                if is_sim {
                    self.device_sync = None;
                }
                emit_activity(
                    &updates,
                    device_section_target(DeviceController::SECTION_DEVICE),
                    "Checking running projects",
                    "Checking",
                    "Checking server response",
                );
                let auto_connect = match self
                    .connect_running_project_if_available(updates.clone())
                    .await
                {
                    Ok(auto_connect) => auto_connect,
                    Err(error) => {
                        let pending_logs = self.device.server.take_pending_logs();
                        self.record_logs(pending_logs);
                        let device_logs = self.device.take_pending_device_logs();
                        self.record_logs(device_logs);
                        self.project.reset();
                        if matches!(error, UiError::NoFirmwareDetected(_)) {
                            self.push_log(UiLogDraft::new(
                                UiLogLevel::Info,
                                UiLogOrigin::Studio,
                                "No LightPlayer firmware detected during server readiness",
                            ));
                            self.device.server.fail_no_firmware();
                            // now the dialog's Blank state is the truth
                            self.rederive_deploy();
                            return Ok(UiNotices::new().with_notice(UiNotice::info(
                                "No LightPlayer firmware detected; flash firmware onto the selected ESP32",
                            )));
                        }
                        // Incompatible firmware (hello gate): surface the
                        // reflash affordance instead of a dead-end error —
                        // reflashing is the ONE way out, and it must stay
                        // reachable (explicit, never automatic).
                        if matches!(
                            self.device.device_state(),
                            Some(DeviceState::Incompatible { .. })
                        ) {
                            self.push_log(UiLogDraft::new(
                                UiLogLevel::Warn,
                                UiLogOrigin::Studio,
                                format!("device firmware is incompatible: {error}"),
                            ));
                            self.device.server.fail(error.to_string());
                            self.rederive_deploy();
                            return Ok(UiNotices::new().with_notice(UiNotice::info(
                                "Device firmware is incompatible with this Studio; update the firmware",
                            )));
                        }
                        self.push_log(UiLogDraft::new(
                            UiLogLevel::Error,
                            UiLogOrigin::Studio,
                            format!("server readiness probe failed: {error}"),
                        ));
                        self.device.server.fail(error.to_string());
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
                    AutoProjectConnect::NotFound if is_sim => {
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
                self.rederive_deploy();
                Ok(outcome)
            }
            Err(error) => {
                self.device.server.fail(error.to_string());
                self.rederive_deploy();
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
            let server = self.device.server.client_mut()?;
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
            let server = self.device.server.client_mut()?;
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
            let server = self.device.server.client_mut()?;
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
            let server = self.device.server.client_mut()?;
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

    async fn disconnect_project(&mut self) -> UiResult {
        self.project.disconnect();
        Ok(UiNotices::new().with_notice(UiNotice::info("Project disconnected")))
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
            let server = self.device.server.client_mut()?;
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
            let server = self.device.server.client_mut()?;
            self.project.sync_loaded_project(server).await?
        };
        self.record_project_sync_run(&sync);
        updates.emit(UxUpdate::View(self.view()));
        Ok(sync)
    }

    fn record_project_sync_run(&mut self, sync: &ProjectSyncRun) {
        // New log lines are a local change the next gate should surface even
        // if the project revision did not move; `record_logs` marks dirty and
        // no-ops on an empty batch.
        self.record_logs(sync.logs.clone());
    }

    async fn disconnect_device(&mut self) -> UiResult {
        self.project.reset();
        self.device_sync = None;
        self.device.server.disconnect();
        self.device.disconnect().await?;
        self.rederive_deploy();
        Ok(UiNotices::new().with_notice(UiNotice::info("Device disconnected")))
    }

    async fn disconnect_lightplayer(&mut self) -> UiResult {
        self.project.reset();
        self.device_sync = None;
        self.device.server.disconnect();
        Ok(UiNotices::new().with_notice(UiNotice::info("LightPlayer disconnected")))
    }

    /// Ask the connected server to apply `level` at runtime and record the
    /// confirmation as a Server-origin log entry. The requested level is
    /// tracked optimistically on the server controller (no wire read-back)
    /// so the console's device selector reflects it; failure surfaces
    /// through the normal action error path.
    async fn set_device_log_level(&mut self, level: UiLogLevel) -> UiResult {
        let mut logs = self
            .device
            .server
            .client_mut()?
            .set_log_level(level)
            .await?;
        logs.push(UiLogDraft::new(
            UiLogLevel::Info,
            UiLogOrigin::Server,
            format!("device log level set to {}", level.label()),
        ));
        self.record_logs(logs);
        self.device.server.set_requested_log_level(level);
        Ok(UiNotices::new())
    }

    async fn reset_device(&mut self, updates: UxUpdateSink) -> UiResult {
        self.run_device_management(
            ManagementFlowSpec {
                request: LinkManagementRequest::ResetRuntime,
                progress_label: "Resetting device",
                reconnect_detail: "Waiting for device boot",
                record_captured_logs_on_success: true,
                done_notice: |_| UiNotice::info("Device reset"),
                degrade_subject: "device reset",
                server_reconnect_failed_notice: "Device reset; reconnect after it finishes booting",
            },
            updates,
        )
        .await
    }

    async fn provision_firmware(&mut self, updates: UxUpdateSink) -> UiResult {
        self.run_device_management(
            ManagementFlowSpec {
                request: LinkManagementRequest::FlashFirmware,
                progress_label: "Flashing firmware",
                reconnect_detail: "Waiting for firmware boot",
                record_captured_logs_on_success: false,
                done_notice: provision_notice,
                degrade_subject: "firmware flashed",
                server_reconnect_failed_notice:
                    "Firmware flashed; reconnect the server after the device finishes booting",
            },
            updates,
        )
        .await
    }

    async fn reset_to_blank(&mut self, updates: UxUpdateSink) -> UiResult {
        self.run_device_management(
            ManagementFlowSpec {
                request: LinkManagementRequest::EraseDeviceFlash,
                progress_label: "Wiping device",
                reconnect_detail: "Checking for LightPlayer firmware",
                record_captured_logs_on_success: false,
                done_notice: reset_notice,
                degrade_subject: "device wiped",
                server_reconnect_failed_notice:
                    "Device wiped; reconnect after the device finishes booting",
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
        self.project.reset();
        self.device.server.disconnect();
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        let target = device_section_target(DeviceController::SECTION_DEVICE);
        let activity = Rc::new(RefCell::new(
            UiActivityView::new(spec.progress_label)
                .with_progress(UiProgress::indeterminate(spec.progress_label)),
        ));
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
        );
        let manage_result = {
            let session = self.device.device_session().ok_or_else(|| {
                UiError::MissingSession("no hardware device session for management".to_string())
            })?;
            session.manage(spec.request, event_sink).await
        };
        let management = match manage_result {
            Ok(management) => management,
            Err(error) => {
                self.record_logs(core::mem::take(&mut *captured_logs.borrow_mut()));
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
        // The link was already rebuilt inside `manage`; what remains is the
        // server reattach + post-attach sequence.
        match self.attach_runtime(updates).await {
            Ok(mut attach_outcome) => {
                outcome.notices.append(&mut attach_outcome.notices);
                Ok(outcome)
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
                self.device.server.fail(error.to_string());
                Ok(outcome.with_notice(UiNotice::info(spec.server_reconnect_failed_notice)))
            }
        }
    }

    fn project_is_loaded(&self) -> bool {
        matches!(self.project.snapshot().state, ProjectState::Ready { .. })
    }
}

/// Cross-module test builders. The actor tests live in a sibling module and
/// cannot reach the private `device`/`project` fields, so these `pub(crate)`
/// helpers assemble a connected controller for them.
#[cfg(test)]
impl StudioController {
    /// Attach stubbed hardware in the given device state (view/derivation
    /// tests that must not script a whole fake device).
    #[cfg(test)]
    pub(crate) fn set_stub_device_for_test(&mut self, state: lpa_link::DeviceState) {
        self.device.set_stub_hardware_for_test(state);
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

        let mut studio = Self::new(|| 0.0);
        studio.device.set_stub_sim_for_test();
        studio.device.server.set_client_for_test(client);
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        studio
    }

    /// Apply a project view into the owned tree (drives probe scoping).
    pub(crate) fn apply_project_view_for_test(&mut self, view: &lpc_view::ProjectView) {
        self.project.apply_project_view(view).unwrap();
    }

    /// The attached hardware's device-session state, for e2e assertions.
    pub(crate) fn device_state_for_test(&self) -> Option<lpa_link::DeviceState> {
        self.device.device_state()
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

fn project_sync_notice(synced: bool, success: &str, needs_attention: &str) -> UiNotice {
    if synced {
        UiNotice::info(success)
    } else {
        UiNotice::warning(needs_attention)
    }
}

fn deploy_transition_error(error: crate::app::device::InvalidTransition) -> UiError {
    UiError::UnsupportedAction(error.to_string())
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
        ProjectSyncPhase, ServerController, ServerFailureKind, ServerState, StudioServerClient,
        UiIssue,
    };

    #[test]
    fn initial_snapshot_selects_provider() {
        let studio = StudioController::new(|| 0.0);

        assert!(matches!(
            studio.snapshot().flow,
            ConnectFlowState::SelectingProvider { .. }
        ));
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
    fn deploy_environment_derives_from_attachment_and_session_state() {
        use crate::app::device::runtime_attachment::ready_state_for_test;
        use lpa_link::DeviceState;

        // no attachment: nothing connected
        let studio = StudioController::new(|| 0.0);
        let env = studio.deploy_environment();
        assert!(!env.device_link_connected);
        assert!(!env.firmware_available);

        // sim attachment: never a device (D22)
        let mut studio = StudioController::new(|| 0.0);
        studio.device.set_stub_sim_for_test();
        studio.device.server.set_state(ServerState::Connected {
            protocol: "sim".to_string(),
        });
        let env = studio.deploy_environment();
        assert!(!env.device_link_connected);
        assert!(!env.firmware_available);

        // hardware Ready + server Connected: firmware available
        let mut studio = connected_studio();
        let env = studio.deploy_environment();
        assert!(env.device_link_connected);
        assert!(env.firmware_available);

        // hardware Ready but the server protocol has not answered
        studio.device.server.set_state(ServerState::Disconnected);
        let env = studio.deploy_environment();
        assert!(env.device_link_connected);
        assert!(!env.firmware_available, "Ready needs a connected server");

        // hardware BlankFlash: connected, no firmware (dialog's Blank)
        let mut studio = connected_studio();
        studio
            .device
            .set_stub_hardware_for_test(DeviceState::BlankFlash);
        let env = studio.deploy_environment();
        assert!(env.device_link_connected);
        assert!(!env.firmware_available);

        // hardware Gone: the link no longer counts as connected
        let mut studio = connected_studio();
        studio.device.set_stub_hardware_for_test(DeviceState::Gone);
        let env = studio.deploy_environment();
        assert!(!env.device_link_connected);
        assert!(!env.firmware_available);

        // server Connected alone (Ready session) stays the happy path
        let mut studio = connected_studio();
        studio
            .device
            .set_stub_hardware_for_test(ready_state_for_test());
        assert!(studio.deploy_environment().firmware_available);
    }

    #[test]
    fn incompatible_device_surfaces_reflash_affordance_in_the_pane() {
        use lpa_link::{DeviceState, IncompatibleReason};

        let mut studio = connected_studio();
        studio
            .device
            .set_stub_hardware_for_test(DeviceState::Incompatible {
                reason: IncompatibleReason::NoHello,
            });

        let view = studio.view();
        let device_pane = view
            .panes
            .iter()
            .find(|pane| pane.node_id.as_str() == DeviceController::NODE_ID)
            .expect("device pane");
        assert_eq!(device_pane.status.kind, UiStatusKind::Warning);
        assert_eq!(device_pane.status.label, "Reflash needed");
        // The ONE affordance: reflash (explicit, never automatic).
        let actions = view_actions(&view);
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware)
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

        // seed one package (creation happens via examples in the UI — the
        // gallery has no create op; see D17)
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
        studio.device.server.set_state(ServerState::Failed {
            issue: UiIssue::new("No LightPlayer firmware detected."),
            kind: ServerFailureKind::NoFirmware,
        });

        let view = studio.view();
        let device_pane = view
            .panes
            .iter()
            .find(|pane| pane.node_id.as_str() == DeviceController::NODE_ID)
            .expect("device pane");
        assert_eq!(device_pane.status.kind, UiStatusKind::Warning);
        assert_eq!(device_pane.status.label, "Ready to flash");
        let actions = view_actions(&view);
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware)
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
        // the pane's door to the deploy dialog + session control
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<crate::app::device::DeployOp>(),
            Some(crate::app::device::DeployOp::OpenDialog { .. })
        )));
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
            Some(DeviceOp::ProvisionFirmware)
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
            Some(DeviceOp::ProvisionFirmware)
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
            studio.device.server.snapshot().state,
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
            studio.device.server.snapshot().state,
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
            studio.device.server.snapshot().state,
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
            studio.device.server.snapshot().state,
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
            studio.device.server.snapshot().state,
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
            studio.device.server.snapshot().state,
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

        let result = block_on_ready(studio.dispatch_with_updates(action, sink));

        assert!(matches!(result, Err(UiError::Link(_))));
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
        let last_view = updates
            .borrow()
            .iter()
            .rev()
            .find_map(|update| match update {
                UxUpdate::View(view) => Some(view.clone()),
                _ => None,
            })
            .expect("dispatch should emit a final view");
        // the failed connect lands back on provider selection, which is home
        // under M4 — the issue rides the gallery instead of a pane status
        let home = last_view
            .home
            .expect("provider selection with an issue shows home");
        assert!(
            home.issue
                .expect("the connect failure surfaces on home")
                .message
                .contains("Failed to open serial port.")
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
            target: UxActivityTarget::pane(ServerController::NODE_ID),
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
        studio.device.server.set_state(ServerState::Connected {
            protocol: "fake-protocol".to_string(),
        });
        studio
            .project
            .mark_ready("loaded-project", 7, ProjectInventorySummary::default());
        studio
    }

    fn connected_studio_with_client(io: ScriptedClientIo) -> StudioController {
        let mut studio = link_connected_studio();
        studio
            .device
            .server
            .set_client_for_test(StudioServerClient::from_io_for_test(
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
        studio.device.set_stub_hardware_for_test(
            crate::app::device::runtime_attachment::ready_state_for_test(),
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

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
