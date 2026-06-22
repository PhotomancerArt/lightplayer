use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_studio_ux::{
    DeviceUx, LinkUx, ProjectUx, ServerUx, StudioUx, StudioView, UiAction, UiBody, UiStepState,
    UiTerminalLine, UxError, UxLogEntry, UxLogLevel, UxNotice, UxNoticeLevel, UxUpdate,
    UxUpdateSink,
};

use crate::components::StudioShell;

const STYLE: &str = include_str!("style.css");

#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn App() -> Element {
    #[cfg(feature = "stories")]
    if crate::stories::story_book::should_show_story_book() {
        return rsx! {
            style { "{STYLE}" }
            crate::stories::story_book::StoryBook {}
        };
    }

    let model = use_signal(StudioWebModel::new);
    let view = model.read().view.clone();
    let running = model.read().running;
    let on_action = move |action: UiAction| {
        spawn(async move {
            execute_action(model, action).await;
        });
    };

    rsx! {
        style { "{STYLE}" }
        StudioShell {
            view,
            running,
            on_action,
        }
    }
}

struct StudioWebModel {
    ux: Option<StudioUx>,
    view: StudioView,
    running: bool,
    console_logs: Vec<UxLogEntry>,
}

impl StudioWebModel {
    fn new() -> Self {
        let ux = StudioUx::new();
        let view = ux.view();
        Self {
            ux: Some(ux),
            view,
            running: false,
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
                node_id,
                status,
                activity,
            } => {
                if let Some(pane) = self.view.panes.iter_mut().find(|pane| {
                    pane.node_id == node_id
                        || (pane.node_id.as_str() == DeviceUx::NODE_ID
                            && matches!(
                                node_id.as_str(),
                                LinkUx::NODE_ID | ServerUx::NODE_ID | ProjectUx::NODE_ID
                            ))
                }) {
                    let section_id = device_activity_section_id(node_id.as_str(), &activity.title);
                    pane.status = status;
                    if pane.node_id.as_str() == DeviceUx::NODE_ID {
                        if let (Some(section_id), UiBody::Stack(stack)) =
                            (section_id, &mut pane.body)
                        {
                            if let Some(section) = stack
                                .sections
                                .iter_mut()
                                .find(|section| section.id == section_id)
                            {
                                section.state = UiStepState::Active;
                                section.body = UiBody::Activity(activity);
                                section.actions.clear();
                                return;
                            }
                        }
                    }
                    pane.body = UiBody::Activity(activity);
                }
            }
            UxUpdate::Log(log) => {
                append_device_terminal_log(&mut self.view, &log);
                self.view.logs.push(log);
            }
        }
    }

    fn push_console_log(&mut self, log: UxLogEntry) {
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
}

fn append_device_terminal_log(view: &mut StudioView, log: &UxLogEntry) {
    if !is_device_log_source(&log.source) {
        return;
    }
    let Some(device_pane) = view
        .panes
        .iter_mut()
        .find(|pane| pane.node_id.as_str() == DeviceUx::NODE_ID)
    else {
        return;
    };
    let UiBody::Stack(stack) = &mut device_pane.body else {
        return;
    };
    stack.terminal.push(UiTerminalLine::new(format!(
        "[{}] {}",
        log.source, log.message
    )));
    if stack.terminal.len() > 240 {
        let remove_count = stack.terminal.len() - 240;
        stack.terminal.drain(0..remove_count);
    }
}

fn is_device_log_source(source: &str) -> bool {
    matches!(
        source,
        "lpa-link" | "browser-serial" | "fw-esp32" | "fw-browser" | "lp-server"
    )
}

fn device_activity_section_id(node_id: &str, title: &str) -> Option<&'static str> {
    if node_id == ServerUx::NODE_ID {
        return Some("connect-lightplayer");
    }
    if node_id == ProjectUx::NODE_ID {
        return Some("open-project");
    }
    if node_id == LinkUx::NODE_ID {
        if title.contains("Provision") || title.contains("Flash") {
            return Some("connect-lightplayer");
        }
        return Some("connect-device");
    }
    if node_id != DeviceUx::NODE_ID {
        return None;
    }
    if title.contains("LightPlayer")
        || title.contains("server")
        || title.contains("firmware")
        || title.contains("Firmware")
    {
        Some("connect-lightplayer")
    } else if title.contains("project") || title.contains("Project") {
        Some("open-project")
    } else {
        Some("connect-device")
    }
}

async fn execute_action(mut model: Signal<StudioWebModel>, action: UiAction) {
    let Some(mut ux) = ({
        let mut state = model.write();
        if state.running {
            return;
        }
        state.running = true;
        state.ux.take()
    }) else {
        model.write().push_console_log(UxLogEntry::new(
            UxLogLevel::Error,
            "studio",
            "Studio UX is already busy.",
        ));
        return;
    };

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

fn log_from_notice(notice: UxNotice) -> UxLogEntry {
    UxLogEntry::new(
        log_level_from_notice(notice.level),
        "studio",
        notice.message,
    )
}

fn log_level_from_notice(level: UxNoticeLevel) -> UxLogLevel {
    match level {
        UxNoticeLevel::Info => UxLogLevel::Info,
        UxNoticeLevel::Warning => UxLogLevel::Warn,
        UxNoticeLevel::Error => UxLogLevel::Error,
    }
}

fn log_from_error(error: UxError) -> UxLogEntry {
    let level = if matches!(&error, UxError::Cancelled(_)) {
        UxLogLevel::Info
    } else {
        UxLogLevel::Error
    };
    UxLogEntry::new(level, "studio", error.to_string())
}
