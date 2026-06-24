use std::cell::Cell;
use std::rc::Rc;

use crate::app::StudioShell;
use dioxus::prelude::*;
use lpa_studio_core::view::steps_view::UiStepState;
use lpa_studio_core::{
    NoticeLevel, StudioController, UiAction, UiActivity, UiError, UiLogEntry, UiLogLevel, UiNotice,
    UiStatus, UiStudioView, UiViewContent, UxActivityTarget, UxUpdate, UxUpdateSink,
};

const STYLE: &str = include_str!("style.css");

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

    fn apply_activity_update(
        &mut self,
        target: UxActivityTarget,
        status: UiStatus,
        activity: UiActivity,
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

async fn execute_action(mut model: Signal<StudioWebModel>, action: UiAction) {
    let Some(mut ux) = ({
        let mut state = model.write();
        if state.running {
            return;
        }
        state.running = true;
        state.ux.take()
    }) else {
        model.write().push_console_log(UiLogEntry::new(
            UiLogLevel::Error,
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

fn log_from_notice(notice: UiNotice) -> UiLogEntry {
    UiLogEntry::new(
        log_level_from_notice(notice.level),
        "studio",
        notice.message,
    )
}

fn log_level_from_notice(level: NoticeLevel) -> UiLogLevel {
    match level {
        NoticeLevel::Info => UiLogLevel::Info,
        NoticeLevel::Warning => UiLogLevel::Warn,
        NoticeLevel::Error => UiLogLevel::Error,
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
