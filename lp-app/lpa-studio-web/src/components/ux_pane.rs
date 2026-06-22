use dioxus::prelude::*;
use lpa_studio_ux::{UxAction, UxActivity, UxBody, UxPaneView, UxProgress};

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
        UxBody::Activity(activity) => rsx! {
            UxActivityBody { activity }
        },
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

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UxActivityBody(activity: UxActivity) -> Element {
    let title = activity.title;
    let detail = activity.detail;
    let progress = activity.progress;
    let terminal = activity.terminal;

    rsx! {
        div { class: "ux-activity",
            p { class: "ux-panel-copy ux-activity-title", "{title}" }
            if let Some(detail) = detail.as_ref() {
                p { class: "ux-panel-copy ux-panel-detail", "{detail}" }
            }
            if let Some(progress) = progress {
                UxProgressBar { progress }
            }
            if !terminal.is_empty() {
                ol { class: "ux-terminal",
                    for line in terminal.iter().rev().take(12).rev() {
                        li { "{line.text}" }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UxProgressBar(progress: UxProgress) -> Element {
    let label = progress.label;
    let detail = progress.detail;
    let percent = progress.percent;
    let timeout_ms = progress.timeout_ms.unwrap_or(0);
    let bar_class = if percent.is_some() {
        "ux-progress-fill ux-progress-fill-determinate"
    } else if progress.timeout_ms.is_some() {
        "ux-progress-fill ux-progress-fill-timeout"
    } else {
        "ux-progress-fill ux-progress-fill-indeterminate"
    };
    let fill_style = match (percent, progress.timeout_ms) {
        (Some(percent), _) => format!("width: {}%;", percent.min(100)),
        (None, Some(_)) => "width: 100%;".to_string(),
        (None, None) => String::new(),
    };
    let timeout_style = if timeout_ms > 0 {
        format!("animation-duration: {timeout_ms}ms;")
    } else {
        String::new()
    };

    rsx! {
        div { class: "ux-progress",
            div { class: "ux-progress-meta",
                span { "{label}" }
                if let Some(percent) = percent {
                    strong { "{percent.min(100)}%" }
                }
            }
            div { class: "ux-progress-track",
                div { class: "{bar_class}", style: "{fill_style}{timeout_style}" }
            }
            if let Some(detail) = detail.as_ref() {
                p { class: "ux-progress-detail", "{detail}" }
            }
        }
    }
}
