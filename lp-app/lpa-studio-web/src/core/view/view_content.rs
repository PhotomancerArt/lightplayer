use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiViewContent};

use crate::app::ProjectPane;
use crate::core::{ActivityView, IssueView, MetricGrid, ProgressBar, StepsView};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ViewContent(
    body: UiViewContent,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    match body {
        UiViewContent::Empty => rsx! {},
        UiViewContent::Text(text) => rsx! {
            p { class: "tw:m-0 tw:text-sm tw:leading-normal tw:text-muted-foreground", "{text}" }
        },
        UiViewContent::Progress(progress) => rsx! {
            ProgressBar { progress }
        },
        UiViewContent::Activity(activity) => rsx! {
            ActivityView { activity }
        },
        UiViewContent::Issue(issue) => rsx! {
            IssueView { issue }
        },
        UiViewContent::Metrics(metrics) => rsx! {
            MetricGrid { metrics }
        },
        UiViewContent::Stack(stack) => rsx! {
            StepsView {
                stack: *stack,
                running,
                on_action,
            }
        },
        // `PaneView` intercepts the project editor to attach the pane's
        // status and actions to the ProjectPane header; this arm only
        // serves direct `ViewContent` consumers and renders with defaults.
        UiViewContent::ProjectEditor(editor) => rsx! {
            ProjectPane {
                view: *editor,
                running,
                on_action,
            }
        },
    }
}
