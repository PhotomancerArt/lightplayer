use dioxus::prelude::*;
use lpa_studio_core::core::view::activity_view::{UiActivityStep, UiActivityStepState};
use lpa_studio_core::{UiActivityView, UiProgress};
use lpa_studio_web_story_macros::story;

use crate::core::ActivityView;
use crate::core::story_fixtures::{story_activity, story_terminal_lines};

#[story]
pub(crate) fn flashing() -> Element {
    rsx! {
        ActivityView {
            activity: story_activity(),
        }
    }
}

#[story]
pub(crate) fn failed_step() -> Element {
    let activity = UiActivityView::new("Provision firmware")
        .with_detail("Studio stopped after the device rejected the write command.")
        .with_progress(UiProgress::indeterminate("Waiting for retry"))
        .with_steps(vec![
            UiActivityStep::new("connect", "Connect bootloader")
                .with_state(UiActivityStepState::Complete),
            UiActivityStep::new("erase", "Erase flash").with_state(UiActivityStepState::Complete),
            UiActivityStep::new("write", "Write firmware")
                .with_state(UiActivityStepState::Failed)
                .with_detail("The browser serial write failed."),
        ])
        .with_terminal(story_terminal_lines());

    rsx! {
        ActivityView {
            activity,
        }
    }
}
