use dioxus::prelude::*;
use lpa_studio_ux::{StudioUx, StudioView, UxAction};

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
    let on_action = move |action: UxAction| {
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
}

async fn execute_action(mut model: Signal<StudioWebModel>, action: UxAction) {
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

    let result = ux.dispatch(action).await;
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
