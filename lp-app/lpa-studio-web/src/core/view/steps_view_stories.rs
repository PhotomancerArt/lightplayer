use dioxus::prelude::*;
use lpa_studio_core::core::view::steps_view::{UiStepState, UiStepView};
use lpa_studio_core::{UiProgress, UiStepsView, UiViewContent};
use lpa_studio_web_story_macros::story;

use crate::core::StepsView;
use crate::core::story_fixtures::{
    confirmation_action, story_actions, story_issue, story_metrics, story_steps,
    story_terminal_lines,
};

#[story]
pub(crate) fn workflow() -> Element {
    rsx! {
        StepsView {
            stack: story_steps(),
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn nested_content() -> Element {
    let steps = UiStepsView::new(vec![
        UiStepView::new("text", "Text body", UiStepState::Complete)
            .with_body(UiViewContent::text("The simulator provider is selected.")),
        UiStepView::new("progress", "Progress body", UiStepState::Active).with_body(
            UiViewContent::Progress(
                UiProgress::determinate("Reading project", 68)
                    .with_detail("Fetching node shape metadata."),
            ),
        ),
        UiStepView::new("metrics", "Metrics body", UiStepState::Complete)
            .with_body(UiViewContent::Metrics(story_metrics())),
        UiStepView::new("issue", "Issue body", UiStepState::NeedsAttention)
            .with_body(UiViewContent::Issue(story_issue()))
            .with_actions(story_actions()),
    ])
    .with_terminal(story_terminal_lines());

    rsx! {
        StepsView {
            stack: steps,
            running: false,
            on_action: move |_| {},
        }
    }
}

#[story]
pub(crate) fn running_actions() -> Element {
    let steps = UiStepsView::new(vec![
        UiStepView::new("active", "Flashing firmware", UiStepState::Active)
            .with_body(UiViewContent::text("Studio is writing the firmware image."))
            .with_actions(vec![confirmation_action()]),
    ]);

    rsx! {
        StepsView {
            stack: steps,
            running: true,
            on_action: move |_| {},
        }
    }
}
