use dioxus::prelude::*;
use lpa_studio_core::UiIssue;
use lpa_studio_web_story_macros::story;

use crate::core::IssueView;

#[story]
pub(crate) fn message_only() -> Element {
    rsx! {
        IssueView {
            issue: UiIssue::new("No LightPlayer firmware detected."),
        }
    }
}

#[story]
pub(crate) fn with_detail() -> Element {
    rsx! {
        IssueView {
            issue: UiIssue::new("Firmware flashing failed")
                .with_detail("Check the cable, boot mode, and browser serial permission."),
        }
    }
}
