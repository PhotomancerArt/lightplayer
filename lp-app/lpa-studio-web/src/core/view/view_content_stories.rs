use dioxus::prelude::*;
use lpa_studio_core::{UiProgress, UiViewContent};
use lpa_studio_web_story_macros::story;

use crate::core::ViewContent;
use crate::core::story_fixtures::{story_activity, story_issue, story_metrics, story_steps};

#[story]
pub(crate) fn body_variants() -> Element {
    rsx! {
        div { class: "ux-story-stack",
            ViewContent {
                body: UiViewContent::text("Choose how Studio should connect."),
                running: false,
                on_action: move |_| {},
            }
            ViewContent {
                body: UiViewContent::Progress(
                    UiProgress::determinate("Reading project", 68)
                        .with_detail("Fetching node shape metadata."),
                ),
                running: false,
                on_action: move |_| {},
            }
            ViewContent {
                body: UiViewContent::Issue(story_issue()),
                running: false,
                on_action: move |_| {},
            }
            ViewContent {
                body: UiViewContent::Metrics(story_metrics()),
                running: false,
                on_action: move |_| {},
            }
        }
    }
}

#[story]
pub(crate) fn composed_variants() -> Element {
    rsx! {
        div { class: "ux-story-stack",
            ViewContent {
                body: UiViewContent::Activity(story_activity()),
                running: false,
                on_action: move |_| {},
            }
            ViewContent {
                body: UiViewContent::Stack(Box::new(story_steps())),
                running: false,
                on_action: move |_| {},
            }
        }
    }
}
