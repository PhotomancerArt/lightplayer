use std::cell::Cell;
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_studio_ux::{
    DeviceUx, LinkUx, ProjectUx, ServerUx, StudioUx, StudioView, UiAction, UiBody, UiStepState,
    UxUpdate, UxUpdateSink,
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
                self.view.logs.push(log);
            }
        }
    }
}

fn device_activity_section_id(node_id: &str, title: &str) -> Option<&'static str> {
    if node_id == ServerUx::NODE_ID {
        return Some("connect-lightplayer");
    }
    if node_id == ProjectUx::NODE_ID {
        return Some("open-project");
    }
    if node_id == LinkUx::NODE_ID {
        if title.contains("Provision") {
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
