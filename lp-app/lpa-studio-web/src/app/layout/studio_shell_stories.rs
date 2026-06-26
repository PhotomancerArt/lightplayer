//! Stories for whole-Studio shell states.

use dioxus::prelude::*;
use lpa_studio_core::UiLogLevel;
use lpa_studio_web_story_macros::story;

use crate::app::story_fixtures::{
    editor_shell_story, endpoint_view, error_view, idle_view, shell_story, simulator_ready_view,
    starting_view, studio_log,
};

#[story]
pub(crate) fn editor_shell() -> Element {
    editor_shell_story()
}

#[story]
pub(crate) fn simulator_idle() -> Element {
    shell_story(idle_view(), false, Vec::new())
}

#[story]
pub(crate) fn simulator_endpoint() -> Element {
    shell_story(endpoint_view(), false, Vec::new())
}

#[story]
pub(crate) fn simulator_starting() -> Element {
    shell_story(starting_view(), true, Vec::new())
}

#[story]
pub(crate) fn simulator_ready() -> Element {
    shell_story(
        simulator_ready_view(),
        false,
        vec![
            studio_log(UiLogLevel::Info, "Simulator is running"),
            studio_log(UiLogLevel::Info, "Demo project loaded"),
        ],
    )
}

#[story]
pub(crate) fn action_error() -> Element {
    shell_story(
        error_view(),
        false,
        vec![studio_log(
            UiLogLevel::Error,
            "browser worker boot timed out",
        )],
    )
}
