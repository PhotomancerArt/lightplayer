use core::future::Future;
use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use lpa_client::{CancelSignal, ProgressDeadline};
use lpa_link::{
    LinkConnection, LinkConnectionKind, LinkManagementRequest, LinkManagementResult,
    LinkProviderKind,
};

use crate::app::studio::console_command::ConsoleCommand;
use crate::app::studio::refresh_cadence::RefreshCadence;
use crate::app::studio::ui_console_view::UiConsoleView;
use crate::core::log::{LogClock, LogFilter, LogRing};
use crate::core::notice::UiNotices;
use crate::{
    ConnectedLink, Controller, ControllerContext, DeviceController, DeviceOp, LinkOpenOutcome,
    NodeRevertOp, ProjectConnectResult, ProjectController, ProjectEditRun, ProjectOp,
    ProjectRefreshOutcome, ProjectState, ProjectSyncRun, SlotEditOp, StudioSnapshot, UiAction,
    UiActions, UiActivityView, UiError, UiLogDraft, UiLogEntry, UiLogLevel, UiLogOrigin, UiNotice,
    UiResult, UiStatus, UiStudioView, UiViewContent, UxActivityTarget, UxUpdate, UxUpdateSink,
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
        }
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
            self.device.snapshot().link,
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
        RefreshCadence::for_link_state(&self.device.snapshot().link.state)
    }

    pub fn actions(&self) -> UiActions {
        UiActions::new(view_actions(&self.view()))
    }

    pub fn view(&self) -> UiStudioView {
        let project_snapshot = self.project.snapshot();
        let project_actions = self.project.actions(self.device.has_lightplayer_state());
        let device_view = self.device.view(&project_snapshot.state, project_actions);
        let panes = if self.project_is_loaded() {
            vec![
                self.project.view(self.device.has_lightplayer_state()),
                device_view,
            ]
        } else {
            vec![device_view]
        };
        UiStudioView::new(panes, self.console_view())
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
                self.device.link.refresh_provider_catalog();
                self.device.server.disconnect();
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
                        device_section_target(DeviceController::SECTION_CONNECT_DEVICE),
                        "Opening device",
                        "Opening",
                        format!("Opening {}", provider_id.label()),
                    );
                }
                match self.device.link.open_provider(provider_id).await? {
                    LinkOpenOutcome::Opened => Ok(UiNotices::new()),
                    LinkOpenOutcome::Cancelled { message } => {
                        Ok(UiNotices::new().with_notice(UiNotice::info(message)))
                    }
                    LinkOpenOutcome::Connected(connected) => {
                        self.attach_connected_link(connected, updates).await
                    }
                }
            }
            DeviceOp::ConnectEndpoint {
                provider_id,
                endpoint_id,
            } => {
                emit_activity(
                    &updates,
                    device_section_target(DeviceController::SECTION_CONNECT_DEVICE),
                    "Opening device session",
                    "Connecting",
                    "Opening device endpoint",
                );
                let connected = self
                    .device
                    .link
                    .connect_endpoint(provider_id, endpoint_id)
                    .await?;
                self.attach_connected_link(connected, updates).await
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

    async fn attach_connected_link(
        &mut self,
        connected: ConnectedLink,
        updates: UxUpdateSink,
    ) -> UiResult {
        self.record_logs(connected.logs);
        self.connect_server_connection(&connected.connection, updates)
            .await
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
            device_section_target(DeviceController::SECTION_CONNECT_DEVICE),
            "Opening device for flashing",
            "Opening",
            "Opening device without attaching LightPlayer",
        );
        match self.device.link.open_provider(provider_id).await? {
            LinkOpenOutcome::Opened => Ok(UiNotices::new().with_notice(UiNotice::info(
                "Choose the device endpoint to open for flashing",
            ))),
            LinkOpenOutcome::Cancelled { message } => {
                Ok(UiNotices::new().with_notice(UiNotice::info(message)))
            }
            LinkOpenOutcome::Connected(connected) => {
                self.record_logs(connected.logs);
                updates.emit(UxUpdate::View(self.view()));
                Ok(UiNotices::new().with_notice(UiNotice::info("Device opened for flashing")))
            }
        }
    }

    async fn connect_server_from_link(&mut self, updates: UxUpdateSink) -> UiResult {
        let connection =
            self.device.link.active_connection().ok_or_else(|| {
                UiError::MissingSession("link connection is not open".to_string())
            })?;
        if should_reopen_before_server_connect(&connection) {
            self.project.reset();
            self.device.server.disconnect();
            emit_activity(
                &updates,
                device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
                "Reopening device",
                "Connecting",
                "Resetting device before server connect",
            );
            let connected = self.device.link.reopen_active_connection().await?;
            return self.attach_connected_link(connected, updates).await;
        }
        self.connect_server_connection(&connection, updates).await
    }

    async fn connect_server_connection(
        &mut self,
        connection: &LinkConnection,
        updates: UxUpdateSink,
    ) -> UiResult {
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
            "Connecting LightPlayer",
            "Connecting",
            "Opening server protocol",
        );
        let server_updates = retarget_activity_updates(
            updates.clone(),
            device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
        );
        match self.device.server.attach_link_connection(
            self.device.link.registry_handle(),
            connection,
            server_updates,
        ) {
            Ok(()) => {
                let mut outcome =
                    UiNotices::new().with_notice(UiNotice::info("Server protocol connected"));
                updates.emit(UxUpdate::View(self.view()));
                emit_activity(
                    &updates,
                    device_section_target(DeviceController::SECTION_OPEN_PROJECT),
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
                        self.project.reset();
                        if matches!(error, UiError::NoFirmwareDetected(_)) {
                            self.push_log(UiLogDraft::new(
                                UiLogLevel::Info,
                                UiLogOrigin::Studio,
                                "No LightPlayer firmware detected during server readiness",
                            ));
                            self.device.server.fail_no_firmware();
                            return Ok(UiNotices::new().with_notice(UiNotice::info(
                                "No LightPlayer firmware detected; flash firmware onto the selected ESP32",
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
                    AutoProjectConnect::NotFound if should_auto_load_demo_project(connection) => {
                        let demo_outcome = self.load_demo_project(updates).await?;
                        outcome.notices.extend(demo_outcome.notices);
                    }
                    AutoProjectConnect::NotFound => {}
                }
                Ok(outcome)
            }
            Err(error) => {
                self.device.server.fail(error.to_string());
                Err(error)
            }
        }
    }

    async fn connect_running_project(&mut self, updates: UxUpdateSink) -> UiResult {
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_OPEN_PROJECT),
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
            device_section_target(DeviceController::SECTION_OPEN_PROJECT),
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
            device_section_target(DeviceController::SECTION_OPEN_PROJECT),
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
            device_section_target(DeviceController::SECTION_OPEN_PROJECT),
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
        self.device.server.disconnect();
        self.device.link.disconnect().await?;
        Ok(UiNotices::new().with_notice(UiNotice::info("Device disconnected")))
    }

    async fn disconnect_lightplayer(&mut self) -> UiResult {
        self.project.reset();
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
        self.project.reset();
        self.device.server.disconnect();
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        let management_updates = capture_log_updates(
            retarget_activity_updates(
                updates.clone(),
                device_section_target(DeviceController::SECTION_CONNECT_DEVICE),
            ),
            Rc::clone(&captured_logs),
        );
        let management = match self
            .device
            .link
            .manage_with_updates(
                LinkManagementRequest::ResetRuntime,
                "Resetting device",
                management_updates,
            )
            .await
        {
            Ok(management) => management,
            Err(error) => {
                self.record_logs(core::mem::take(&mut *captured_logs.borrow_mut()));
                return Err(error);
            }
        };
        self.record_logs(core::mem::take(&mut *captured_logs.borrow_mut()));
        self.record_logs(management.logs);

        let mut outcome = UiNotices::new().with_notice(UiNotice::info("Device reset"));
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
            "Reconnecting device",
            "Connecting",
            "Waiting for device boot",
        );
        match self.device.link.reopen_active_connection().await {
            Ok(connected) => match self.attach_connected_link(connected, updates).await {
                Ok(mut attach_outcome) => {
                    outcome.notices.append(&mut attach_outcome.notices);
                    Ok(outcome)
                }
                Err(error) => {
                    self.push_log(UiLogDraft::new(
                        UiLogLevel::Warn,
                        UiLogOrigin::Studio,
                        format!("device reset but server reconnect failed: {error}"),
                    ));
                    self.device.server.fail(error.to_string());
                    Ok(outcome.with_notice(UiNotice::info(
                        "Device reset; reconnect after it finishes booting",
                    )))
                }
            },
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!("device reset but serial reopen failed: {error}"),
                ));
                self.device.server.fail(error.to_string());
                Ok(outcome.with_notice(UiNotice::info(
                    "Device reset; reconnect the device after it finishes booting",
                )))
            }
        }
    }

    async fn provision_firmware(&mut self, updates: UxUpdateSink) -> UiResult {
        self.project.reset();
        self.device.server.disconnect();
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        let management_updates = capture_log_updates(
            retarget_activity_updates(
                updates.clone(),
                device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
            ),
            Rc::clone(&captured_logs),
        );
        let management = match self
            .device
            .link
            .manage_with_updates(
                LinkManagementRequest::FlashFirmware,
                "Flashing firmware",
                management_updates,
            )
            .await
        {
            Ok(management) => management,
            Err(error) => {
                self.record_logs(core::mem::take(&mut *captured_logs.borrow_mut()));
                return Err(error);
            }
        };
        self.record_logs(management.logs);
        let mut outcome = UiNotices::new().with_notice(provision_notice(&management.result));
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
            "Reconnecting device",
            "Connecting",
            "Waiting for firmware boot",
        );
        match self.device.link.reopen_active_connection().await {
            Ok(connected) => match self.attach_connected_link(connected, updates).await {
                Ok(mut attach_outcome) => {
                    outcome.notices.append(&mut attach_outcome.notices);
                    Ok(outcome)
                }
                Err(error) => {
                    self.push_log(UiLogDraft::new(
                        UiLogLevel::Warn,
                        UiLogOrigin::Studio,
                        format!("firmware flashed but server reconnect failed: {error}"),
                    ));
                    self.device.server.fail(error.to_string());
                    Ok(outcome.with_notice(UiNotice::info(
                        "Firmware flashed; reconnect the server after the device finishes booting",
                    )))
                }
            },
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!("firmware flashed but serial reopen failed: {error}"),
                ));
                self.device.server.fail(error.to_string());
                Ok(outcome.with_notice(UiNotice::info(
                    "Firmware flashed; reconnect the device after it finishes booting",
                )))
            }
        }
    }

    async fn reset_to_blank(&mut self, updates: UxUpdateSink) -> UiResult {
        self.project.reset();
        self.device.server.disconnect();
        let captured_logs = Rc::new(RefCell::new(Vec::new()));
        let management_updates = capture_log_updates(
            retarget_activity_updates(
                updates.clone(),
                device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
            ),
            Rc::clone(&captured_logs),
        );
        let management = match self
            .device
            .link
            .manage_with_updates(
                LinkManagementRequest::EraseDeviceFlash,
                "Wiping device",
                management_updates,
            )
            .await
        {
            Ok(management) => management,
            Err(error) => {
                self.record_logs(core::mem::take(&mut *captured_logs.borrow_mut()));
                return Err(error);
            }
        };
        self.record_logs(management.logs);
        let mut outcome = UiNotices::new().with_notice(reset_notice(&management.result));
        emit_activity(
            &updates,
            device_section_target(DeviceController::SECTION_CONNECT_LIGHTPLAYER),
            "Reconnecting device",
            "Connecting",
            "Checking for LightPlayer firmware",
        );
        match self.device.link.reopen_active_connection().await {
            Ok(connected) => match self.attach_connected_link(connected, updates).await {
                Ok(mut attach_outcome) => {
                    outcome.notices.append(&mut attach_outcome.notices);
                    Ok(outcome)
                }
                Err(error) => {
                    self.push_log(UiLogDraft::new(
                        UiLogLevel::Warn,
                        UiLogOrigin::Studio,
                        format!("device wiped but server reconnect failed: {error}"),
                    ));
                    self.device.server.fail(error.to_string());
                    Ok(outcome.with_notice(UiNotice::info(
                        "Device wiped; reconnect after the device finishes booting",
                    )))
                }
            },
            Err(error) => {
                self.push_log(UiLogDraft::new(
                    UiLogLevel::Warn,
                    UiLogOrigin::Studio,
                    format!("device wiped but serial reopen failed: {error}"),
                ));
                self.device.server.fail(error.to_string());
                Ok(outcome.with_notice(UiNotice::info(
                    "Device wiped; reconnect the device after it finishes booting",
                )))
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
    /// A controller whose link + server are connected and whose project is
    /// `Ready`, with `client` wired as the server IO so a refresh sends reads.
    pub(crate) fn connected_with_client_for_test(client: crate::StudioServerClient) -> Self {
        use crate::{ConnectedDeviceSummary, LinkState, ProjectInventorySummary};
        use lpa_link::LinkProviderKind;

        let mut studio = Self::new(|| 0.0);
        studio.device.link.set_state(LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });
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

fn should_auto_load_demo_project(connection: &LinkConnection) -> bool {
    matches!(connection.kind, LinkConnectionKind::BrowserWorker { .. })
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

fn capture_log_updates(
    updates: UxUpdateSink,
    captured_logs: Rc<RefCell<Vec<UiLogDraft>>>,
) -> UxUpdateSink {
    UxUpdateSink::new(move |update| {
        if let UxUpdate::Log(log) = &update {
            captured_logs.borrow_mut().push(log.clone());
        }
        updates.emit(update);
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

fn should_reopen_before_server_connect(connection: &LinkConnection) -> bool {
    matches!(
        connection.kind,
        LinkConnectionKind::BrowserSerialEsp32 { .. }
    )
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
    use lpa_link::{
        LinkCapabilities, LinkConnection, LinkConnectionKind, LinkEndpoint, LinkEndpointId,
        LinkProviderKind, LinkSession,
    };
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
    use crate::core::view::steps_view::UiStepState;
    use crate::{
        ConnectedDeviceSummary, ControllerId, LinkController, LinkState, ProjectController,
        ProjectEditorOp, ProjectEditorTarget, ProjectInventorySummary, ProjectNodeAddress,
        ProjectNodeTarget, ProjectState, ProjectSyncPhase, ServerController, ServerFailureKind,
        ServerState, StudioServerClient, UiIssue,
    };

    #[test]
    fn initial_snapshot_selects_provider() {
        let studio = StudioController::new(|| 0.0);

        assert!(matches!(
            studio.snapshot().link.state,
            LinkState::SelectingProvider { .. }
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
    fn initial_view_exposes_device_pane() {
        let studio = StudioController::new(|| 0.0);

        let view = studio.view();

        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.panes[0].node_id.as_str(), DeviceController::NODE_ID);
        assert_eq!(device_section_ids(&view), vec!["select-connection"]);
    }

    #[test]
    fn connected_without_project_keeps_project_actions_in_device_pane() {
        let mut studio = connected_studio();
        studio.project.reset();

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.panes[0].node_id.as_str(), DeviceController::NODE_ID);
        assert_eq!(
            device_section_ids(&view),
            vec![
                "select-connection",
                "connect-device",
                "connect-lightplayer",
                "open-project"
            ]
        );
        assert!(actions.iter().any(|action| {
            matches!(
                action.op_as::<ProjectOp>(),
                Some(ProjectOp::ConnectRunningProject)
            )
        }));
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::LoadDemoProject)
        )));
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::DisconnectLightPlayer)
        )));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetToBlank)))
        );
        assert!(
            actions.iter().any(|action| matches!(
                action.op_as::<DeviceOp>(),
                Some(DeviceOp::DisconnectDevice)
            ))
        );
    }

    #[test]
    fn connected_link_without_server_hides_open_project_step() {
        let studio = link_connected_studio();

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(
            device_section_ids(&view),
            vec!["select-connection", "connect-device", "connect-lightplayer"]
        );
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ConnectLightPlayer)
        )));
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::ConnectRunningProject | ProjectOp::LoadDemoProject)
        )));
    }

    #[test]
    fn no_firmware_failure_hides_connect_lightplayer_action() {
        let mut studio = connected_studio();
        studio.project.reset();
        studio
            .device
            .link
            .set_active_session_for_test(management_capable_session());
        studio.device.server.set_state(ServerState::Failed {
            issue: UiIssue::new("No LightPlayer firmware detected."),
            kind: ServerFailureKind::NoFirmware,
        });

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(view.panes[0].status.kind, UiStatusKind::Warning);
        assert_eq!(view.panes[0].status.label, "Ready to flash");
        assert_eq!(
            device_section_ids(&view),
            vec!["select-connection", "connect-device", "connect-lightplayer"]
        );
        let UiViewContent::Stack(stack) = &view.panes[0].body else {
            panic!("device pane should render a stack view");
        };
        let lightplayer_section = stack
            .sections
            .iter()
            .find(|section| section.id == "connect-lightplayer")
            .expect("connect lightplayer section should exist");
        assert_eq!(lightplayer_section.title, "LightPlayer unavailable");
        assert_eq!(lightplayer_section.state, UiStepState::Active);
        assert!(matches!(lightplayer_section.body, UiViewContent::Text(_)));
        let device_section = stack
            .sections
            .iter()
            .find(|section| section.id == "connect-device")
            .expect("connect device section should exist");
        assert!(device_section.actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware)
        )));
        assert!(
            device_section
                .actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetToBlank)))
        );
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware)
        )));
        assert!(
            actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetToBlank)))
        );
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ConnectLightPlayer)
        )));
    }

    #[test]
    fn loaded_project_gets_project_pane() {
        let studio = connected_studio();

        let view = studio.view();
        let actions = view_actions(&view);

        assert_eq!(view.panes.len(), 2);
        assert_eq!(view.panes[0].node_id.as_str(), ProjectController::NODE_ID);
        assert_eq!(view.panes[1].node_id.as_str(), DeviceController::NODE_ID);
        assert_eq!(
            device_section_ids(&view),
            vec![
                "select-connection",
                "connect-device",
                "connect-lightplayer",
                "open-project"
            ]
        );
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::ConnectRunningProject | ProjectOp::LoadDemoProject)
        )));
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::DisconnectLightPlayer)
        )));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetToBlank)))
        );
        assert!(
            actions.iter().any(|action| matches!(
                action.op_as::<DeviceOp>(),
                Some(DeviceOp::DisconnectDevice)
            ))
        );
    }

    #[test]
    fn connected_lightplayer_offers_non_destructive_device_reset() {
        let mut studio = connected_studio();
        studio
            .device
            .link
            .set_active_session_for_test(management_capable_session());

        let actions = view_actions(&studio.view());

        assert!(
            actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetDevice)))
        );
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::DisconnectLightPlayer)
        )));
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware)
        )));
        assert!(
            actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetToBlank)))
        );
        assert!(
            actions.iter().any(|action| matches!(
                action.op_as::<DeviceOp>(),
                Some(DeviceOp::DisconnectDevice)
            ))
        );
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ConnectLightPlayer)
        )));
    }

    #[test]
    fn loaded_project_keeps_management_recovery_actions_visible() {
        let mut studio = connected_studio();
        studio
            .device
            .link
            .set_active_session_for_test(management_capable_session());

        let actions = view_actions(&studio.view());

        assert!(actions.iter().any(|action| matches!(
            action.op_as::<ProjectOp>(),
            Some(ProjectOp::DisconnectProject)
        )));
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::DisconnectLightPlayer)
        )));
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ProvisionFirmware)
        )));
        assert!(
            actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetDevice)))
        );
        assert!(
            actions
                .iter()
                .any(|action| matches!(action.op_as::<DeviceOp>(), Some(DeviceOp::ResetToBlank)))
        );
        assert!(
            actions.iter().any(|action| matches!(
                action.op_as::<DeviceOp>(),
                Some(DeviceOp::DisconnectDevice)
            ))
        );
        assert!(!actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ConnectLightPlayer)
        )));
    }

    #[test]
    fn open_provider_for_recovery_skips_server_attach() {
        let mut studio = StudioController::new(|| 0.0);
        studio.device.link = LinkController::with_registry(registry_with_fake_endpoint());

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
            studio.device.link.snapshot().state,
            LinkState::SelectingEndpoint { .. }
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
            studio.device.link.snapshot().state,
            LinkState::Connected { .. }
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
            studio.device.link.snapshot().state,
            LinkState::Connected { .. }
        ));
        let actions = view_actions(&studio.view());
        assert!(actions.iter().any(|action| matches!(
            action.op_as::<DeviceOp>(),
            Some(DeviceOp::ConnectLightPlayer)
        )));
        assert!(
            actions.iter().any(|action| matches!(
                action.op_as::<DeviceOp>(),
                Some(DeviceOp::DisconnectDevice)
            ))
        );
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
            studio.device.link.snapshot().state,
            LinkState::SelectingProvider { .. }
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
            studio.device.link.snapshot().state,
            LinkState::SelectingProvider { .. }
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
            studio.device.link.snapshot().state,
            LinkState::Connected { .. }
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
        let mut studio = StudioController::new(|| 0.0);
        studio.device.link = LinkController::with_registry(registry_with_fake_connect_error(
            "Failed to open serial port.",
        ));
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
                    && section_id == DeviceController::SECTION_CONNECT_DEVICE
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
        assert_eq!(last_view.panes[0].status.kind, UiStatusKind::Error);
        assert_eq!(last_view.panes[0].status.label, "Needs attention");
    }

    #[test]
    fn only_browser_worker_connections_auto_load_demo_project() {
        let browser_worker = LinkConnection::browser_worker("browser-worker-worker-1", "session-1");
        let fake = LinkConnection::fake("fake-runtime", "fake-session");

        assert!(should_auto_load_demo_project(&browser_worker));
        assert!(!should_auto_load_demo_project(&fake));
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
            DeviceController::SECTION_CONNECT_LIGHTPLAYER,
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
        studio.device.link.set_state(LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::Fake,
                "fake-runtime",
                "fake-session",
                "Fake runtime",
            ),
        });
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

    fn management_capable_session() -> LinkSession {
        LinkSession::new(
            "fake-session",
            LinkProviderKind::Fake,
            "fake-runtime",
            LinkConnectionKind::Fake,
            LinkCapabilities::esp32_serial_base()
                .with_flash()
                .with_device_erase(),
        )
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
