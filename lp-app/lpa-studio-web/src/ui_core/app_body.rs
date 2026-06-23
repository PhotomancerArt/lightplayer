use dioxus::prelude::*;
use lpa_studio_ux::{UiAction, UiBody};

use crate::ui_core::{AppActivity, AppStack, MetricGrid};
use crate::ui_studio::ProjectSidebar;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AppBody(body: UiBody, running: bool, on_action: EventHandler<UiAction>) -> Element {
    match body {
        UiBody::Empty => rsx! {},
        UiBody::Text(text) => rsx! {
            p { class: "ux-panel-copy", "{text}" }
        },
        UiBody::Progress(progress) => {
            let label = progress.label;
            let detail = progress.detail;
            rsx! {
                p { class: "ux-panel-copy", "{label}" }
                if let Some(detail) = detail.as_ref() {
                    p { class: "ux-panel-copy ux-panel-detail", "{detail}" }
                }
            }
        }
        UiBody::Activity(activity) => rsx! {
            AppActivity { activity }
        },
        UiBody::Issue(issue) => {
            let message = issue.message;
            let detail = issue.detail;
            rsx! {
                p { class: "ux-panel-copy ux-panel-issue", "{message}" }
                if let Some(detail) = detail.as_ref() {
                    p { class: "ux-panel-copy ux-panel-detail", "{detail}" }
                }
            }
        }
        UiBody::Metrics(metrics) => rsx! {
            MetricGrid { metrics }
        },
        UiBody::Stack(stack) => rsx! {
            AppStack {
                stack: *stack,
                running,
                on_action,
            }
        },
        UiBody::ProjectEditor(editor) => rsx! {
            ProjectSidebar {
                view: *editor,
                running,
                on_action,
            }
        },
    }
}
