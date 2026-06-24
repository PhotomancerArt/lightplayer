use crate::core::{ProgressBar, TerminalOutput};
use dioxus::prelude::*;
use lpa_studio_core::UiActivityView;
use lpa_studio_core::core::view::activity_view::UiActivityStepState;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ActivityView(activity: UiActivityView) -> Element {
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
                ProgressBar { progress }
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
            TerminalOutput {
                lines: terminal,
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
