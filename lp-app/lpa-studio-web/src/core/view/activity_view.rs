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
        div { class: "tw:grid tw:min-w-0 tw:gap-3",
            p { class: "tw:m-0 tw:text-sm tw:font-bold tw:leading-normal tw:text-strong-foreground", "{title}" }
            if let Some(detail) = detail.as_ref() {
                p { class: "tw:m-0 tw:text-sm tw:leading-normal tw:text-subtle-foreground", "{detail}" }
            }
            if let Some(progress) = progress {
                ProgressBar { progress }
            }
            if !steps.is_empty() {
                ol { class: "tw:m-0 tw:grid tw:list-none tw:gap-2 tw:p-0",
                    for step in steps {
                        li { class: "{activity_step_class(step.state)}",
                            span { class: "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-current tw:bg-step-marker tw:text-xs tw:font-bold tw:leading-none", "{activity_step_marker(step.state)}" }
                            div { class: "tw:grid tw:min-w-0 tw:gap-1",
                                span { "{step.label}" }
                                if let Some(detail) = step.detail.as_ref() {
                                    small { class: "tw:text-xs tw:text-subtle-foreground", "{detail}" }
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
        UiActivityStepState::Pending => {
            "tw:grid tw:grid-cols-[28px_minmax(0,1fr)] tw:gap-3 tw:text-sm tw:text-subtle-foreground"
        }
        UiActivityStepState::Active => {
            "tw:grid tw:grid-cols-[28px_minmax(0,1fr)] tw:gap-3 tw:text-sm tw:font-bold tw:text-status-working-foreground"
        }
        UiActivityStepState::Complete => {
            "tw:grid tw:grid-cols-[28px_minmax(0,1fr)] tw:gap-3 tw:text-sm tw:text-status-good-foreground"
        }
        UiActivityStepState::Failed => {
            "tw:grid tw:grid-cols-[28px_minmax(0,1fr)] tw:gap-3 tw:text-sm tw:text-status-error-foreground"
        }
    }
}

fn activity_step_marker(state: UiActivityStepState) -> &'static str {
    state.text_marker()
}
