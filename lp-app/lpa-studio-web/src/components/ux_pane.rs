use dioxus::prelude::*;
use lpa_studio_ux::{UxAction, UxBody, UxPaneView};

use crate::components::ActionStrip;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn UxPane(
    view: UxPaneView,
    primary: bool,
    running: bool,
    on_action: EventHandler<UxAction>,
) -> Element {
    let UxPaneView {
        title,
        status,
        body,
        actions,
        ..
    } = view;
    let status_label = status.label;
    let panel_class = if primary {
        "ux-panel ux-panel-primary"
    } else {
        "ux-panel"
    };

    rsx! {
        section { class: "{panel_class}",
            div { class: "ux-panel-heading",
                p { "{title}" }
                h2 { "{status_label}" }
            }
            UxPaneBody { body }
            if !actions.is_empty() {
                ActionStrip {
                    actions,
                    running,
                    on_action,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UxPaneBody(body: UxBody) -> Element {
    match body {
        UxBody::Empty => rsx! {},
        UxBody::Text(text) => rsx! {
            p { class: "ux-panel-copy", "{text}" }
        },
        UxBody::Progress(progress) => {
            let label = progress.label;
            let detail = progress.detail;
            rsx! {
                p { class: "ux-panel-copy", "{label}" }
                if let Some(detail) = detail.as_ref() {
                    p { class: "ux-panel-copy ux-panel-detail", "{detail}" }
                }
            }
        }
        UxBody::Issue(issue) => {
            let message = issue.message;
            let detail = issue.detail;
            rsx! {
                p { class: "ux-panel-copy ux-panel-issue", "{message}" }
                if let Some(detail) = detail.as_ref() {
                    p { class: "ux-panel-copy ux-panel-detail", "{detail}" }
                }
            }
        }
        UxBody::Metrics(metrics) => rsx! {
            dl { class: "ux-metrics",
                for metric in metrics {
                    div {
                        dt { "{metric.label}" }
                        dd { "{metric.value}" }
                    }
                }
            }
        },
    }
}
