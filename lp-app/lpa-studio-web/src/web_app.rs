use std::cell::Cell;
use std::rc::Rc;

use crate::app::StudioShell;
use crate::studio_url;
use dioxus::prelude::*;
use futures_util::future::{Either, select};
use futures_util::{FutureExt, pin_mut};
use gloo_timers::future::TimeoutFuture;
use lpa_studio_core::core::view::steps_view::UiStepState;
use lpa_studio_core::{
    DeviceOp, LinkProviderKind, LinkState, ProjectEditorOp, ProjectOp, ProjectSyncRun, ServerOp,
    StudioController, UiAction, UiActivityView, UiError, UiLogEntry, UiLogLevel, UiNotice,
    UiNoticeLevel, UiResult, UiStatus, UiStudioView, UiViewContent, UxActivityTarget, UxUpdate,
    UxUpdateSink,
};

const STYLE: &str = include_str!("style.css");
const DEVICE_PROJECT_REFRESH_INTERVAL_MS: u32 = 750;
const SIMULATOR_PROJECT_REFRESH_INTERVAL_MS: u32 = 16;
const DEVICE_PASSIVE_REFRESH_TIMEOUT_MS: u32 = 6_000;
const SIMULATOR_PASSIVE_REFRESH_TIMEOUT_MS: u32 = 1_000;
const PASSIVE_REFRESH_CANCEL_POLL_MS: u32 = 25;
const PASSIVE_REFRESH_FAILURE_BACKOFF_MS: u32 = 3_000;
const PASSIVE_REFRESH_TIMEOUT_MESSAGE: &str = "passive project refresh timed out";
const CONTROL_PRODUCT_PROBE_COMPATIBILITY_REASON: &str = "Control product previews are disabled after a protocol timeout. Update device firmware to enable them.";
const ACTION_STOP_POLL_MS: u32 = 25;
const CONNECT_LIGHTPLAYER_TIMEOUT_MS: u32 = 12_000;
const PROJECT_ACTION_TIMEOUT_MS: u32 = 8_000;
const PROJECT_LOAD_TIMEOUT_MS: u32 = 20_000;
const PROJECT_EDITOR_ACTION_TIMEOUT_MS: u32 = 6_000;

#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn App() -> Element {
    #[cfg(feature = "stories")]
    if crate::stories::story_book::should_show_story_book() {
        return rsx! {
            style { "{STYLE}" }
            document::Stylesheet { href: asset!("/assets/tailwind.css") }
            crate::stories::story_book::StoryBook {}
        };
    }

    let model = use_signal(StudioWebModel::new);
    let startup_intent = use_hook(studio_url::read_connection_intent);
    let startup_model = model;
    let _startup_task = use_future(move || async move {
        if let Some(action) = startup_intent.and_then(|intent| intent.startup_action()) {
            execute_action(startup_model, action).await;
        }
    });
    let refresh_model = model;
    let _refresh_task = use_future(move || async move {
        loop {
            wait_for_next_project_refresh(refresh_model).await;
            execute_refresh_tick(refresh_model).await;
        }
    });
    let view = model.read().view.clone();
    let running = model.read().running;
    let on_action = move |action: UiAction| {
        spawn(async move {
            execute_action(model, action).await;
        });
    };

    rsx! {
        style { "{STYLE}" }
        document::Stylesheet { href: asset!("/assets/tailwind.css") }
        StudioShell {
            view,
            running,
            on_action,
        }
    }
}

struct StudioWebModel {
    ux: Option<StudioController>,
    view: UiStudioView,
    running: bool,
    running_action: Option<RunningAction>,
    action_generation: u64,
    foreground_cancel_requested: bool,
    pending_recovery_action: Option<UiAction>,
    refreshing: bool,
    refresh_generation: u64,
    refresh_cancel_requested: bool,
    refresh_backoff_ms: u32,
    passive_refresh_timeout_logged: bool,
    console_logs: Vec<UiLogEntry>,
}

impl StudioWebModel {
    fn new() -> Self {
        let ux = StudioController::new();
        let view = ux.view();
        Self {
            ux: Some(ux),
            view,
            running: false,
            running_action: None,
            action_generation: 0,
            foreground_cancel_requested: false,
            pending_recovery_action: None,
            refreshing: false,
            refresh_generation: 0,
            refresh_cancel_requested: false,
            refresh_backoff_ms: 0,
            passive_refresh_timeout_logged: false,
            console_logs: Vec::new(),
        }
    }

    fn refresh_from_ux(&mut self) {
        if let Some(ux) = &self.ux {
            self.view = ux.view();
            self.append_console_logs_to_view();
        }
    }

    fn apply_update(&mut self, update: UxUpdate) {
        match update {
            UxUpdate::View(mut view) => {
                view.logs.extend(self.console_logs.clone());
                self.view = view;
            }
            UxUpdate::Activity {
                target,
                status,
                activity,
            } => {
                self.apply_activity_update(target, status, activity);
            }
            UxUpdate::Log(log) => {
                log_to_js_console(&log);
                self.view.logs.push(log);
            }
        }
    }

    fn push_console_log(&mut self, log: UiLogEntry) {
        log_to_js_console(&log);
        self.console_logs.push(log.clone());
        if self.console_logs.len() > 80 {
            let remove_count = self.console_logs.len() - 80;
            self.console_logs.drain(0..remove_count);
        }
        self.view.logs.push(log);
    }

    fn append_console_logs_to_view(&mut self) {
        self.view.logs.extend(self.console_logs.clone());
    }

    fn project_refresh_cadence(&self) -> ProjectRefreshCadence {
        self.ux
            .as_ref()
            .map(StudioController::snapshot)
            .map(|snapshot| project_refresh_cadence_for_link_state(&snapshot.link.state))
            .unwrap_or(ProjectRefreshCadence::Device)
    }

    fn next_project_refresh_delay_ms(&mut self) -> u32 {
        let base = project_refresh_interval_ms(self.project_refresh_cadence());
        let backoff = self.refresh_backoff_ms;
        self.refresh_backoff_ms = 0;
        base.saturating_add(backoff)
    }

    fn begin_project_refresh(&mut self) -> Option<ProjectRefreshStart> {
        if self.running || self.refreshing {
            return None;
        }
        let cadence = self.project_refresh_cadence();
        let ux = self.ux.take()?;
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        self.refreshing = true;
        self.refresh_cancel_requested = false;
        Some(ProjectRefreshStart {
            ux,
            id: self.refresh_generation,
            cadence,
        })
    }

    fn finish_project_refresh(&mut self, id: u64, ux: StudioController) {
        self.ux = Some(ux);
        self.refresh_from_ux();
        if self.refresh_generation == id {
            self.refreshing = false;
            self.refresh_cancel_requested = false;
        }
    }

    fn request_project_refresh_cancel(&mut self) -> bool {
        if !self.refreshing || self.refresh_cancel_requested {
            return false;
        }
        self.refresh_cancel_requested = true;
        true
    }

    fn project_refresh_cancel_requested(&self, id: u64) -> bool {
        self.refreshing && self.refresh_generation == id && self.refresh_cancel_requested
    }

    fn delay_next_project_refresh(&mut self, delay_ms: u32) {
        self.refresh_backoff_ms = self.refresh_backoff_ms.max(delay_ms);
    }

    fn begin_foreground_action(&mut self, action: &UiAction) -> Option<ForegroundActionStart> {
        if self.running || self.refreshing {
            return None;
        }
        let ux = self.ux.take()?;
        self.action_generation = self.action_generation.wrapping_add(1);
        self.running = true;
        self.foreground_cancel_requested = false;
        self.pending_recovery_action = None;
        let running_action = RunningAction::from_action(action);
        self.running_action = Some(running_action.clone());
        Some(ForegroundActionStart {
            ux,
            id: self.action_generation,
            running_action,
        })
    }

    fn finish_foreground_action(
        &mut self,
        id: u64,
        ux: StudioController,
        run_pending: bool,
    ) -> Option<UiAction> {
        self.ux = Some(ux);
        self.refresh_from_ux();
        if self.action_generation == id {
            self.running = false;
            self.running_action = None;
            self.foreground_cancel_requested = false;
            if run_pending {
                self.pending_recovery_action.take()
            } else {
                self.pending_recovery_action = None;
                None
            }
        } else {
            None
        }
    }

    fn request_foreground_action_cancel(&mut self, next_action: UiAction) -> bool {
        if !self.running || self.foreground_cancel_requested {
            return false;
        }
        self.foreground_cancel_requested = true;
        self.pending_recovery_action = Some(next_action);
        true
    }

    fn foreground_action_cancel_requested(&self, id: u64) -> bool {
        self.running && self.action_generation == id && self.foreground_cancel_requested
    }

    fn apply_activity_update(
        &mut self,
        target: UxActivityTarget,
        status: UiStatus,
        activity: UiActivityView,
    ) {
        let Some(pane) = self
            .view
            .panes
            .iter_mut()
            .find(|pane| pane.node_id.as_str() == target.pane_node_id().as_str())
        else {
            return;
        };
        pane.status = status;

        match target {
            UxActivityTarget::Pane { .. } => {
                pane.body = UiViewContent::Activity(activity);
            }
            UxActivityTarget::StackSection { section_id, .. } => {
                if let UiViewContent::Stack(stack) = &mut pane.body {
                    if let Some(section) = stack
                        .sections
                        .iter_mut()
                        .find(|section| section.id == section_id)
                    {
                        section.state = UiStepState::Active;
                        section.body = UiViewContent::Activity(activity);
                        section.actions.clear();
                        return;
                    }
                }
                pane.body = UiViewContent::Activity(activity);
            }
        }
    }
}

struct ProjectRefreshStart {
    ux: StudioController,
    id: u64,
    cadence: ProjectRefreshCadence,
}

struct ForegroundActionStart {
    ux: StudioController,
    id: u64,
    running_action: RunningAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RunningAction {
    label: String,
    node_id: String,
    cancelable: bool,
}

impl RunningAction {
    fn from_action(action: &UiAction) -> Self {
        Self {
            label: action.meta().label.clone(),
            node_id: action.node_id().to_string(),
            cancelable: foreground_action_timeout_ms(action).is_some(),
        }
    }

    fn summary(&self) -> String {
        format!("{} [{}]", self.label, self.node_id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectRefreshCadence {
    Simulator,
    Device,
}

async fn wait_for_next_project_refresh(mut model: Signal<StudioWebModel>) {
    let delay_ms = model.write().next_project_refresh_delay_ms();
    TimeoutFuture::new(delay_ms).await;
}

fn project_refresh_interval_ms(cadence: ProjectRefreshCadence) -> u32 {
    match cadence {
        ProjectRefreshCadence::Simulator => SIMULATOR_PROJECT_REFRESH_INTERVAL_MS,
        ProjectRefreshCadence::Device => DEVICE_PROJECT_REFRESH_INTERVAL_MS,
    }
}

fn passive_refresh_timeout_ms(cadence: ProjectRefreshCadence) -> u32 {
    match cadence {
        ProjectRefreshCadence::Simulator => SIMULATOR_PASSIVE_REFRESH_TIMEOUT_MS,
        ProjectRefreshCadence::Device => DEVICE_PASSIVE_REFRESH_TIMEOUT_MS,
    }
}

fn project_refresh_cadence_for_link_state(state: &LinkState) -> ProjectRefreshCadence {
    match state {
        LinkState::Connected { device } | LinkState::Managing { device, .. }
            if device.provider_id == LinkProviderKind::BrowserWorker =>
        {
            ProjectRefreshCadence::Simulator
        }
        _ => ProjectRefreshCadence::Device,
    }
}

async fn execute_action(mut model: Signal<StudioWebModel>, action: UiAction) {
    let incoming_action = RunningAction::from_action(&action);
    let preempts_refresh = action_preempts_passive_refresh(&action);
    let ForegroundActionStart {
        ux,
        id,
        running_action,
    } = loop {
        let acquire = {
            let mut state = model.write();
            if state.running {
                let running = state.running_action.clone();
                if action_preempts_foreground_action(&action)
                    && running.as_ref().is_some_and(|action| action.cancelable)
                {
                    let requested = state.request_foreground_action_cancel(action.clone());
                    let running_summary = running
                        .as_ref()
                        .map(RunningAction::summary)
                        .unwrap_or_else(|| "another action".to_string());
                    let message = if requested {
                        format!(
                            "Cancelling {running_summary} so {} can run.",
                            incoming_action.summary()
                        )
                    } else {
                        format!(
                            "{} is already waiting for {running_summary} to cancel.",
                            incoming_action.summary()
                        )
                    };
                    state.push_console_log(UiLogEntry::new(UiLogLevel::Warn, "studio", message));
                } else {
                    let running_summary = running
                        .as_ref()
                        .map(RunningAction::summary)
                        .unwrap_or_else(|| "another action".to_string());
                    state.push_console_log(UiLogEntry::new(
                        UiLogLevel::Warn,
                        "studio",
                        format!(
                            "Ignored {} because {running_summary} is still running.",
                            incoming_action.summary()
                        ),
                    ));
                }
                return;
            }
            if state.refreshing {
                if preempts_refresh && state.request_project_refresh_cancel() {
                    state.push_console_log(UiLogEntry::new(
                        UiLogLevel::Info,
                        "studio",
                        "Pausing project refresh for device action.",
                    ));
                }
                ActionAcquire::Wait
            } else if let Some(start) = state.begin_foreground_action(&action) {
                state.push_console_log(UiLogEntry::new(
                    UiLogLevel::Debug,
                    "studio",
                    format!("Starting action {}.", incoming_action.summary()),
                ));
                ActionAcquire::Ready(start)
            } else {
                ActionAcquire::MissingUx
            }
        };
        match acquire {
            ActionAcquire::Ready(start) => break start,
            ActionAcquire::Wait => TimeoutFuture::new(25).await,
            ActionAcquire::MissingUx => {
                model.write().push_console_log(UiLogEntry::new(
                    UiLogLevel::Error,
                    "studio",
                    "Studio UX is already busy.",
                ));
                return;
            }
        }
    };

    studio_url::update_for_action(&action);
    let accepting_updates = Rc::new(Cell::new(true));
    let mut update_model = model;
    let update_gate = Rc::clone(&accepting_updates);
    let updates = UxUpdateSink::new(move |update| {
        if update_gate.get() {
            update_model.write().apply_update(update);
        }
    });
    let timeout_ms = foreground_action_timeout_ms(&action);
    let recovers_server = foreground_timeout_recovers_server(&action);
    let disables_control_probes = foreground_timeout_disables_control_product_probes(&action);
    let outcome = execute_action_with_watchdog(ux, action, updates, model, id, timeout_ms).await;
    accepting_updates.set(false);
    match outcome {
        ForegroundActionOutcome::Completed { ux, result } => {
            let mut state = model.write();
            let _pending = state.finish_foreground_action(id, ux, false);
            match result {
                Ok(outcome) => {
                    for notice in outcome.notices {
                        state.push_console_log(log_from_notice(notice));
                    }
                }
                Err(error) => {
                    state.push_console_log(log_from_error(error));
                }
            }
        }
        ForegroundActionOutcome::Stopped { mut ux, stop } => {
            let message = match stop {
                ForegroundActionStop::Preempted => {
                    format!(
                        "Cancelled action {} for recovery.",
                        running_action.summary()
                    )
                }
                ForegroundActionStop::TimedOut => {
                    format!("Timed out action {}.", running_action.summary())
                }
            };
            ux.recover_from_foreground_action_timeout(message.clone(), recovers_server);
            let control_probes_disabled = disables_control_probes
                && ux.disable_control_product_probes(CONTROL_PRODUCT_PROBE_COMPATIBILITY_REASON);
            let mut state = model.write();
            let pending_action = state.finish_foreground_action(id, ux, true);
            state.delay_next_project_refresh(PASSIVE_REFRESH_FAILURE_BACKOFF_MS);
            state.push_console_log(UiLogEntry::new(UiLogLevel::Warn, "studio", message));
            if control_probes_disabled {
                state.push_console_log(UiLogEntry::new(
                    UiLogLevel::Warn,
                    "studio",
                    CONTROL_PRODUCT_PROBE_COMPATIBILITY_REASON,
                ));
            }
            drop(state);
            if let Some(pending_action) = pending_action {
                spawn(async move {
                    execute_action(model, pending_action).await;
                });
            }
        }
    }
}

fn action_preempts_passive_refresh(action: &UiAction) -> bool {
    if action.op_as::<ProjectOp>().is_some() {
        return false;
    }
    action
        .op_as::<ServerOp>()
        .is_some_and(|op| matches!(op, ServerOp::DisconnectServer))
        || action.op_as::<DeviceOp>().is_some_and(|op| {
            matches!(
                op,
                DeviceOp::OpenProvider { .. }
                    | DeviceOp::ConnectEndpoint { .. }
                    | DeviceOp::ConnectLightPlayer
                    | DeviceOp::DisconnectLightPlayer
                    | DeviceOp::ResetDevice
                    | DeviceOp::ProvisionFirmware
                    | DeviceOp::ResetToBlank
                    | DeviceOp::DisconnectDevice
                    | DeviceOp::RefreshConnections
            )
        })
}

fn action_preempts_foreground_action(action: &UiAction) -> bool {
    action_preempts_passive_refresh(action)
}

fn foreground_action_timeout_ms(action: &UiAction) -> Option<u32> {
    if action
        .op_as::<DeviceOp>()
        .is_some_and(|op| matches!(op, DeviceOp::ConnectLightPlayer))
    {
        return Some(CONNECT_LIGHTPLAYER_TIMEOUT_MS);
    }
    if action.op_as::<ProjectEditorOp>().is_some() {
        return Some(PROJECT_EDITOR_ACTION_TIMEOUT_MS);
    }
    action.op_as::<ProjectOp>().and_then(|op| match op {
        ProjectOp::ConnectRunningProject
        | ProjectOp::ConnectLoadedProject { .. }
        | ProjectOp::RefreshProject => Some(PROJECT_ACTION_TIMEOUT_MS),
        ProjectOp::LoadDemoProject => Some(PROJECT_LOAD_TIMEOUT_MS),
        ProjectOp::DisconnectProject => None,
    })
}

fn foreground_timeout_recovers_server(action: &UiAction) -> bool {
    foreground_action_timeout_ms(action).is_some()
}

fn foreground_timeout_disables_control_product_probes(action: &UiAction) -> bool {
    action
        .op_as::<DeviceOp>()
        .is_some_and(|op| matches!(op, DeviceOp::ConnectLightPlayer))
        || action.op_as::<ProjectOp>().is_some()
        || action.op_as::<ProjectEditorOp>().is_some()
}

enum ActionAcquire {
    Ready(ForegroundActionStart),
    Wait,
    MissingUx,
}

enum ForegroundActionOutcome {
    Completed {
        ux: StudioController,
        result: UiResult,
    },
    Stopped {
        ux: StudioController,
        stop: ForegroundActionStop,
    },
}

enum ForegroundActionRun {
    Completed(UiResult),
    Stopped(ForegroundActionStop),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ForegroundActionStop {
    Preempted,
    TimedOut,
}

async fn execute_action_with_watchdog(
    mut ux: StudioController,
    action: UiAction,
    updates: UxUpdateSink,
    model: Signal<StudioWebModel>,
    action_id: u64,
    timeout_ms: Option<u32>,
) -> ForegroundActionOutcome {
    let Some(timeout_ms) = timeout_ms else {
        let result = ux.dispatch_with_updates(action, updates).await;
        return ForegroundActionOutcome::Completed { ux, result };
    };

    let run = {
        let dispatch = ux.dispatch_with_updates(action, updates).fuse();
        let stop = wait_for_foreground_action_stop(model, action_id, timeout_ms).fuse();
        pin_mut!(dispatch, stop);

        match select(dispatch, stop).await {
            Either::Left((result, _stop)) => ForegroundActionRun::Completed(result),
            Either::Right((stop, dispatch)) => {
                drop(dispatch);
                ForegroundActionRun::Stopped(stop)
            }
        }
    };
    match run {
        ForegroundActionRun::Completed(result) => ForegroundActionOutcome::Completed { ux, result },
        ForegroundActionRun::Stopped(stop) => ForegroundActionOutcome::Stopped { ux, stop },
    }
}

async fn wait_for_foreground_action_stop(
    model: Signal<StudioWebModel>,
    action_id: u64,
    timeout_ms: u32,
) -> ForegroundActionStop {
    let mut elapsed_ms = 0;
    while elapsed_ms < timeout_ms {
        TimeoutFuture::new(ACTION_STOP_POLL_MS).await;
        elapsed_ms = elapsed_ms.saturating_add(ACTION_STOP_POLL_MS);
        if model.read().foreground_action_cancel_requested(action_id) {
            return ForegroundActionStop::Preempted;
        }
    }
    ForegroundActionStop::TimedOut
}

async fn execute_refresh_tick(mut model: Signal<StudioWebModel>) {
    let Some(ProjectRefreshStart {
        mut ux,
        id,
        cadence,
    }) = model.write().begin_project_refresh()
    else {
        return;
    };

    let outcome = {
        let refresh = ux.refresh_loaded_project_tick().fuse();
        let stop =
            wait_for_passive_refresh_stop(model, id, passive_refresh_timeout_ms(cadence)).fuse();
        pin_mut!(refresh, stop);

        match select(refresh, stop).await {
            Either::Left((result, _stop)) => PassiveRefreshOutcome::Completed(result),
            Either::Right((stop_reason, refresh)) => {
                drop(refresh);
                PassiveRefreshOutcome::Stopped(stop_reason)
            }
        }
    };

    match outcome {
        PassiveRefreshOutcome::Completed(result) => {
            let failed = passive_refresh_result_failed(&result);
            let disable_control_probes = passive_refresh_needs_control_probe_fallback(&result);
            if let Err(error) = &result {
                ux.mark_passive_project_refresh_failed(error.to_string());
            }
            let control_probes_disabled = disable_control_probes
                && ux.disable_control_product_probes(CONTROL_PRODUCT_PROBE_COMPATIBILITY_REASON);
            let mut state = model.write();
            state.finish_project_refresh(id, ux);
            if failed {
                state.delay_next_project_refresh(PASSIVE_REFRESH_FAILURE_BACKOFF_MS);
            } else {
                state.passive_refresh_timeout_logged = false;
            }
            if control_probes_disabled {
                state.push_console_log(UiLogEntry::new(
                    UiLogLevel::Warn,
                    "studio",
                    CONTROL_PRODUCT_PROBE_COMPATIBILITY_REASON,
                ));
            }
            if let Err(error) = result {
                state.push_console_log(log_from_error(error));
            }
        }
        PassiveRefreshOutcome::Stopped(stop_reason) => {
            let message = match stop_reason {
                PassiveRefreshStop::Preempted => "passive project refresh was preempted",
                PassiveRefreshStop::TimedOut => PASSIVE_REFRESH_TIMEOUT_MESSAGE,
            };
            ux.mark_passive_project_refresh_failed(message);
            let control_probes_disabled = matches!(stop_reason, PassiveRefreshStop::TimedOut)
                && ux.disable_control_product_probes(CONTROL_PRODUCT_PROBE_COMPATIBILITY_REASON);
            let mut state = model.write();
            state.finish_project_refresh(id, ux);
            if matches!(stop_reason, PassiveRefreshStop::TimedOut) {
                state.delay_next_project_refresh(PASSIVE_REFRESH_FAILURE_BACKOFF_MS);
                if !state.passive_refresh_timeout_logged {
                    state.push_console_log(UiLogEntry::new(UiLogLevel::Warn, "studio", message));
                    state.passive_refresh_timeout_logged = true;
                }
            }
            if control_probes_disabled {
                state.push_console_log(UiLogEntry::new(
                    UiLogLevel::Warn,
                    "studio",
                    CONTROL_PRODUCT_PROBE_COMPATIBILITY_REASON,
                ));
            }
        }
    }
}

enum PassiveRefreshOutcome {
    Completed(Result<Option<ProjectSyncRun>, UiError>),
    Stopped(PassiveRefreshStop),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PassiveRefreshStop {
    Preempted,
    TimedOut,
}

async fn wait_for_passive_refresh_stop(
    model: Signal<StudioWebModel>,
    refresh_id: u64,
    timeout_ms: u32,
) -> PassiveRefreshStop {
    let mut elapsed_ms = 0;
    while elapsed_ms < timeout_ms {
        TimeoutFuture::new(PASSIVE_REFRESH_CANCEL_POLL_MS).await;
        elapsed_ms = elapsed_ms.saturating_add(PASSIVE_REFRESH_CANCEL_POLL_MS);
        if model.read().project_refresh_cancel_requested(refresh_id) {
            return PassiveRefreshStop::Preempted;
        }
    }
    PassiveRefreshStop::TimedOut
}

fn passive_refresh_result_failed(result: &Result<Option<ProjectSyncRun>, UiError>) -> bool {
    match result {
        Ok(Some(sync)) => !sync.synced,
        Ok(None) => false,
        Err(_) => true,
    }
}

fn passive_refresh_needs_control_probe_fallback(
    result: &Result<Option<ProjectSyncRun>, UiError>,
) -> bool {
    match result {
        Err(error) => refresh_failure_text_suggests_probe_compatibility(&error.to_string()),
        Ok(Some(sync)) if !sync.synced => sync
            .logs
            .iter()
            .any(|log| refresh_failure_text_suggests_probe_compatibility(&log.message)),
        _ => false,
    }
}

fn refresh_failure_text_suggests_probe_compatibility(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("timed out")
        || message.contains("timeout")
        || message.contains("unknown variant")
        || message.contains("control_product")
}

fn log_from_notice(notice: UiNotice) -> UiLogEntry {
    UiLogEntry::new(
        log_level_from_notice(notice.level),
        "studio",
        notice.message,
    )
}

fn log_level_from_notice(level: UiNoticeLevel) -> UiLogLevel {
    match level {
        UiNoticeLevel::Info => UiLogLevel::Info,
        UiNoticeLevel::Warning => UiLogLevel::Warn,
        UiNoticeLevel::Error => UiLogLevel::Error,
    }
}

fn log_from_error(error: UiError) -> UiLogEntry {
    let level = if matches!(&error, UiError::Cancelled(_)) {
        UiLogLevel::Info
    } else {
        UiLogLevel::Error
    };
    UiLogEntry::new(level, "studio", error.to_string())
}

fn log_to_js_console(log: &UiLogEntry) {
    let message = format!("[{}] {}", log.source, log.message);
    match log.level {
        UiLogLevel::Debug => console_debug(&message),
        UiLogLevel::Info => console_info(&message),
        UiLogLevel::Warn => console_warn(&message),
        UiLogLevel::Error => console_error(&message),
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console, js_name = debug)]
    fn console_debug(message: &str);

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console, js_name = info)]
    fn console_info(message: &str);

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(message: &str);

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

#[cfg(not(target_arch = "wasm32"))]
fn console_debug(_message: &str) {}

#[cfg(not(target_arch = "wasm32"))]
fn console_info(_message: &str) {}

#[cfg(not(target_arch = "wasm32"))]
fn console_warn(_message: &str) {}

#[cfg(not(target_arch = "wasm32"))]
fn console_error(_message: &str) {}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{
        ConnectedDeviceSummary, DeviceController, LinkState, ProgressState, ProjectController,
        ServerController,
    };

    use super::*;

    #[test]
    fn browser_worker_link_uses_simulator_refresh_cadence() {
        let state = LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::BrowserWorker,
                "browser-worker",
                "session",
                "Simulator",
            ),
        };

        assert_eq!(
            project_refresh_cadence_for_link_state(&state),
            ProjectRefreshCadence::Simulator
        );
    }

    #[test]
    fn serial_link_keeps_device_refresh_cadence() {
        let state = LinkState::Connected {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::BrowserSerialEsp32,
                "serial",
                "session",
                "ESP32",
            ),
        };

        assert_eq!(
            project_refresh_cadence_for_link_state(&state),
            ProjectRefreshCadence::Device
        );
    }

    #[test]
    fn managing_browser_worker_keeps_simulator_refresh_cadence() {
        let state = LinkState::Managing {
            device: ConnectedDeviceSummary::new(
                LinkProviderKind::BrowserWorker,
                "browser-worker",
                "session",
                "Simulator",
            ),
            progress: ProgressState::new("Resetting simulator"),
        };

        assert_eq!(
            project_refresh_cadence_for_link_state(&state),
            ProjectRefreshCadence::Simulator
        );
    }

    #[test]
    fn device_recovery_actions_preempt_passive_refresh() {
        let action = UiAction::from_op(DeviceController::NODE_ID, DeviceOp::ResetDevice);

        assert!(action_preempts_passive_refresh(&action));
    }

    #[test]
    fn server_disconnect_preempts_passive_refresh() {
        let action = UiAction::from_op(ServerController::NODE_ID, ServerOp::DisconnectServer);

        assert!(action_preempts_passive_refresh(&action));
    }

    #[test]
    fn project_actions_do_not_preempt_passive_refresh() {
        let action = UiAction::from_op(ProjectController::NODE_ID, ProjectOp::RefreshProject);

        assert!(!action_preempts_passive_refresh(&action));
    }

    #[test]
    fn refresh_cancel_request_is_scoped_to_active_generation() {
        let mut model = StudioWebModel::new();
        let refresh = model.begin_project_refresh().expect("refresh starts");

        assert!(!model.project_refresh_cancel_requested(refresh.id));
        assert!(model.request_project_refresh_cancel());
        assert!(model.project_refresh_cancel_requested(refresh.id));
        assert!(!model.project_refresh_cancel_requested(refresh.id + 1));

        model.finish_project_refresh(refresh.id, refresh.ux);
        assert!(!model.project_refresh_cancel_requested(refresh.id));
    }

    #[test]
    fn connect_lightplayer_has_foreground_timeout() {
        let action = UiAction::from_op(DeviceController::NODE_ID, DeviceOp::ConnectLightPlayer);

        assert_eq!(
            foreground_action_timeout_ms(&action),
            Some(CONNECT_LIGHTPLAYER_TIMEOUT_MS)
        );
        assert!(foreground_timeout_recovers_server(&action));
        assert!(foreground_timeout_disables_control_product_probes(&action));
    }

    #[test]
    fn recovery_actions_can_preempt_foreground_action() {
        let action = UiAction::from_op(DeviceController::NODE_ID, DeviceOp::ResetDevice);

        assert!(action_preempts_foreground_action(&action));
        assert_eq!(foreground_action_timeout_ms(&action), None);
    }

    #[test]
    fn project_editor_actions_have_foreground_timeout() {
        let action = UiAction::from_op("studio|project|node:fixture", ProjectEditorOp::Focus);

        assert_eq!(
            foreground_action_timeout_ms(&action),
            Some(PROJECT_EDITOR_ACTION_TIMEOUT_MS)
        );
    }

    #[test]
    fn foreground_cancel_request_is_scoped_to_active_generation() {
        let mut model = StudioWebModel::new();
        let running_action =
            UiAction::from_op(DeviceController::NODE_ID, DeviceOp::ConnectLightPlayer);
        let pending_action = UiAction::from_op(DeviceController::NODE_ID, DeviceOp::ResetDevice);
        let start = model
            .begin_foreground_action(&running_action)
            .expect("action starts");

        assert!(!model.foreground_action_cancel_requested(start.id));
        assert!(model.request_foreground_action_cancel(pending_action.clone()));
        assert!(model.foreground_action_cancel_requested(start.id));
        assert!(!model.foreground_action_cancel_requested(start.id + 1));

        let pending = model.finish_foreground_action(start.id, start.ux, true);
        assert_eq!(pending, Some(pending_action));
        assert!(!model.foreground_action_cancel_requested(start.id));
    }

    #[test]
    fn refresh_backoff_is_consumed_once() {
        let mut model = StudioWebModel::new();
        model.delay_next_project_refresh(1_250);

        assert_eq!(
            model.next_project_refresh_delay_ms(),
            DEVICE_PROJECT_REFRESH_INTERVAL_MS + 1_250
        );
        assert_eq!(
            model.next_project_refresh_delay_ms(),
            DEVICE_PROJECT_REFRESH_INTERVAL_MS
        );
    }

    #[test]
    fn timeout_text_suggests_control_probe_fallback() {
        assert!(refresh_failure_text_suggests_probe_compatibility(
            "timed out waiting for browser serial protocol response"
        ));
        assert!(refresh_failure_text_suggests_probe_compatibility(
            "unknown variant `control_product`"
        ));
        assert!(!refresh_failure_text_suggests_probe_compatibility(
            "shape sync response did not include shapes"
        ));
    }
}
