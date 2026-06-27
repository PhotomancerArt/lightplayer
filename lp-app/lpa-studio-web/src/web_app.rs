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
    DeviceOp, LinkProviderKind, LinkState, ProjectOp, ProjectSyncRun, ServerOp, StudioController,
    UiAction, UiActivityView, UiError, UiLogEntry, UiLogLevel, UiNotice, UiNoticeLevel, UiStatus,
    UiStudioView, UiViewContent, UxActivityTarget, UxUpdate, UxUpdateSink,
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
                self.view.logs.push(log);
            }
        }
    }

    fn push_console_log(&mut self, log: UiLogEntry) {
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
    let preempts_refresh = action_preempts_passive_refresh(&action);
    let mut ux = loop {
        let acquire = {
            let mut state = model.write();
            if state.running {
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
            } else if let Some(ux) = state.ux.take() {
                state.running = true;
                ActionAcquire::Ready(ux)
            } else {
                ActionAcquire::MissingUx
            }
        };
        match acquire {
            ActionAcquire::Ready(ux) => break ux,
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
    let result = ux.dispatch_with_updates(action, updates).await;
    accepting_updates.set(false);
    let mut state = model.write();
    state.ux = Some(ux);
    state.refresh_from_ux();
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
    state.running = false;
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

enum ActionAcquire {
    Ready(StudioController),
    Wait,
    MissingUx,
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
