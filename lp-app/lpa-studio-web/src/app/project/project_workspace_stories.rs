//! Stories for loaded-project workspace states.

use dioxus::prelude::*;
use lpa_studio_core::UxLogLevel;
use lpa_studio_web_story_macros::story;

use crate::app::story_fixtures::{
    device_project_empty_view, device_project_selection_view, project_ready_state,
    project_ready_view, project_sync_failed_view, project_syncing_view, project_view, shell_story,
    studio_log,
};
use crate::core::AppPane;

#[story]
pub(crate) fn project_pane() -> Element {
    let view = project_view(project_ready_state(), true);
    rsx! {
        AppPane {
            view,
            primary: false,
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn device_project_empty() -> Element {
    let view = device_project_empty_view();
    rsx! {
        AppPane {
            view,
            primary: true,
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn device_project_selection() -> Element {
    let view = device_project_selection_view();
    rsx! {
        AppPane {
            view,
            primary: true,
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn project_ready() -> Element {
    shell_story(
        project_ready_view(),
        false,
        vec![studio_log(UxLogLevel::Info, "Demo project loaded")],
    )
}

#[story]
pub(crate) fn project_syncing() -> Element {
    shell_story(
        project_syncing_view(),
        true,
        vec![studio_log(UxLogLevel::Info, "Reading project shapes")],
    )
}

#[story]
pub(crate) fn project_sync_failed() -> Element {
    shell_story(
        project_sync_failed_view(),
        false,
        vec![studio_log(
            UxLogLevel::Error,
            "project sync failed: protocol timeout",
        )],
    )
}
