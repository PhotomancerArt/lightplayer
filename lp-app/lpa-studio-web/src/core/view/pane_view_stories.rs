use dioxus::prelude::*;
use lpa_studio_core::{ControllerId, UiPaneView, UiStatus, UiViewContent};
use lpa_studio_web_story_macros::story;

use crate::core::PaneView;
use crate::core::story_fixtures::{story_actions, story_issue, story_pane};

#[story]
pub(crate) fn workflow_pane() -> Element {
    rsx! {
        PaneView {
            view: story_pane(),
            primary: true,
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn attention_pane() -> Element {
    let pane = UiPaneView::new(
        ControllerId::new("story|core|attention-pane"),
        "Project",
        UiStatus::error("Sync issue"),
        UiViewContent::Issue(story_issue()),
        story_actions(),
    );

    rsx! {
        PaneView {
            view: pane,
            primary: true,
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn quiet_pane() -> Element {
    let pane = UiPaneView::new(
        ControllerId::new("story|core|quiet-pane"),
        "Server",
        UiStatus::neutral("Offline"),
        UiViewContent::text("Open a link endpoint to attach the server protocol."),
        Vec::new(),
    );

    rsx! {
        PaneView {
            view: pane,
            primary: false,
            running: false,
            on_action: move |_| {},
        }
    }
}
