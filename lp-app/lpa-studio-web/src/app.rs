use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_studio_ux::{StudioUx, StudioView, UiAction, UiBody, UxUpdate, UxUpdateSink};

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
    let error = model.read().error.clone();
    let notices = model.read().notices.clone();
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
            error,
            notices,
            on_action,
        }
    }
}

struct StudioWebModel {
    ux: Option<StudioUx>,
    view: StudioView,
    running: bool,
    error: Option<String>,
    notices: Vec<String>,
}

impl StudioWebModel {
    fn new() -> Self {
        let ux = StudioUx::new();
        let view = ux.view();
        Self {
            ux: Some(ux),
            view,
            running: false,
            error: None,
            notices: Vec::new(),
        }
    }

    fn refresh_from_ux(&mut self) {
        if let Some(ux) = &self.ux {
            self.view = ux.view();
        }
    }

    fn apply_update(&mut self, update: UxUpdate) {
        match update {
            UxUpdate::View(view) => {
                self.view = view;
            }
            UxUpdate::Activity {
                node_id,
                status,
                activity,
            } => {
                if let Some(pane) = self
                    .view
                    .panes
                    .iter_mut()
                    .find(|pane| pane.node_id == node_id)
                {
                    pane.status = status;
                    pane.body = UiBody::Activity(activity);
                }
            }
            UxUpdate::Log(log) => {
                self.view.logs.push(log);
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
        state.error = None;
        state.ux.take()
    }) else {
        model.write().error = Some("Studio UX is already busy.".to_string());
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
    match result {
        Ok(outcome) => {
            state.notices = outcome
                .notices
                .into_iter()
                .map(|notice| notice.message)
                .collect();
        }
        Err(error) => {
            state.error = Some(error.to_string());
        }
    }
    state.ux = Some(ux);
    state.refresh_from_ux();
    state.running = false;
}
