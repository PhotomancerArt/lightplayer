use dioxus::prelude::*;
use lpa_studio_ux::{
    UiAction, UiActivity, UiActivityStepState, UiBody, UiPaneView, UiProgress, UiStackView,
    UiStepState,
};

use crate::components::ActionStrip;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn UxPane(
    view: UiPaneView,
    primary: bool,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let UiPaneView {
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
            UxPaneBody {
                body,
                running,
                on_action,
            }
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
fn UxPaneBody(body: UiBody, running: bool, on_action: EventHandler<UiAction>) -> Element {
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
            UxActivityBody { activity }
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
            dl { class: "ux-metrics",
                for metric in metrics {
                    div {
                        dt { "{metric.label}" }
                        dd { "{metric.value}" }
                    }
                }
            }
        },
        UiBody::Stack(stack) => rsx! {
            UxStackBody {
                stack: *stack,
                running,
                on_action,
            }
        },
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UxStackBody(stack: UiStackView, running: bool, on_action: EventHandler<UiAction>) -> Element {
    let terminal = stack.terminal;

    rsx! {
        div { class: "ux-stack",
            ol { class: "ux-stack-sections",
                for section in stack.sections {
                    li { class: "{stack_section_class(section.state)}",
                        div { class: "ux-stack-section-heading",
                            span { class: "ux-stack-section-marker", "{section.state.text_marker()}" }
                            h3 { "{section.title}" }
                        }
                        div { class: "ux-stack-section-body",
                            UxPaneBody {
                                body: section.body,
                                running,
                                on_action,
                            }
                        }
                        if !section.actions.is_empty() {
                            ActionStrip {
                                actions: section.actions,
                                running,
                                on_action,
                            }
                        }
                    }
                }
            }
            if !terminal.is_empty() {
                ol { class: "ux-terminal ux-stack-terminal",
                    for line in terminal.iter() {
                        li { "{line.text}" }
                    }
                }
            }
        }
    }
}

fn stack_section_class(state: UiStepState) -> &'static str {
    match state {
        UiStepState::Pending => "ux-stack-section ux-stack-section-pending",
        UiStepState::Active => "ux-stack-section ux-stack-section-active",
        UiStepState::Complete => "ux-stack-section ux-stack-section-complete",
        UiStepState::NeedsAttention => "ux-stack-section ux-stack-section-attention",
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UxActivityBody(activity: UiActivity) -> Element {
    let title = activity.title;
    let detail = activity.detail;
    let progress = activity.progress;
    let steps = activity.steps;
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
            if !steps.is_empty() {
                ol { class: "ux-activity-steps",
                    for step in steps {
                        li { class: "{activity_step_class(step.state)}",
                            span { class: "ux-activity-step-marker", "{activity_step_marker(step.state)}" }
                            div { class: "ux-activity-step-copy",
                                span { "{step.label}" }
                                if let Some(detail) = step.detail.as_ref() {
                                    small { "{detail}" }
                                }
                            }
                        }
                    }
                }
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

fn activity_step_class(state: UiActivityStepState) -> &'static str {
    match state {
        UiActivityStepState::Pending => "ux-activity-step ux-activity-step-pending",
        UiActivityStepState::Active => "ux-activity-step ux-activity-step-active",
        UiActivityStepState::Complete => "ux-activity-step ux-activity-step-complete",
        UiActivityStepState::Failed => "ux-activity-step ux-activity-step-failed",
    }
}

fn activity_step_marker(state: UiActivityStepState) -> &'static str {
    state.text_marker()
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn UxProgressBar(progress: UiProgress) -> Element {
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
