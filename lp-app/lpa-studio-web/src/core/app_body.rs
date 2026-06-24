use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiViewContent};

use crate::app::ProjectSidebar;
use crate::core::{AppActivity, AppStack, MetricGrid};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AppBody(body: UiViewContent, running: bool, on_action: EventHandler<UiAction>) -> Element {
    match body {
        UiViewContent::Empty => rsx! {},
        UiViewContent::Text(text) => rsx! {
            p { class: "ux-panel-copy", "{text}" }
        },
        UiViewContent::Progress(progress) => {
            let label = progress.label;
            let detail = progress.detail;
            rsx! {
                p { class: "ux-panel-copy", "{label}" }
                if let Some(detail) = detail.as_ref() {
                    p { class: "ux-panel-copy ux-panel-detail", "{detail}" }
                }
            }
        }
        UiViewContent::Activity(activity) => rsx! {
            AppActivity { activity }
        },
        UiViewContent::Issue(issue) => {
            let message = issue.message;
            let detail = issue.detail;
            rsx! {
                p { class: "ux-panel-copy ux-panel-issue", "{message}" }
                if let Some(detail) = detail.as_ref() {
                    p { class: "ux-panel-copy ux-panel-detail", "{detail}" }
                }
            }
        }
        UiViewContent::Metrics(metrics) => rsx! {
            MetricGrid { metrics }
        },
        UiViewContent::Stack(stack) => rsx! {
            AppStack {
                stack: *stack,
                running,
                on_action,
            }
        },
        UiViewContent::ProjectEditor(editor) => rsx! {
            ProjectSidebar {
                view: *editor,
                running,
                on_action,
            }
        },
    }
}
