use crate::base::{StudioIcon, StudioIconName};
use crate::core::{ActionStrip, TerminalOutput, ViewContent};
use dioxus::prelude::*;
use lpa_studio_core::core::view::steps_view::UiStepState;
use lpa_studio_core::{UiAction, UiStepsView};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StepsView(stack: UiStepsView, running: bool, on_action: EventHandler<UiAction>) -> Element {
    let terminal = stack.terminal;
    let sections = stack.sections.into_iter().collect::<Vec<_>>();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-4",
            ol { class: "tw:m-0 tw:grid tw:list-none tw:gap-0 tw:p-0",
                for section in sections {
                    li { class: "{stack_section_class(section.state)}",
                        div { class: "{stack_marker_class(section.state)}", aria_label: "{step_state_label(section.state)}",
                            if let Some(icon) = stack_marker_icon(section.state) {
                                StudioIcon {
                                    name: icon,
                                    size: 14,
                                }
                            }
                        }
                        h3 { class: "tw:m-0 tw:self-center tw:text-base tw:font-bold tw:leading-tight tw:text-strong-foreground tw:break-words", "{section.title}" }
                        div { class: "tw:col-span-2 tw:min-w-0",
                            ViewContent {
                                body: section.body,
                                running,
                                on_action,
                            }
                        }
                        if !section.actions.is_empty() {
                            div { class: "tw:col-span-2 tw:min-w-0",
                                ActionStrip {
                                    actions: section.actions,
                                    running,
                                    on_action,
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

fn stack_section_class(state: UiStepState) -> &'static str {
    match state {
        UiStepState::Pending => {
            "tw:grid tw:grid-cols-[32px_minmax(0,1fr)] tw:gap-x-3 tw:gap-y-2 tw:border-t tw:border-border-muted tw:bg-transparent tw:py-3 tw:text-subtle-foreground tw:first:border-t-0"
        }
        UiStepState::Active => {
            "tw:grid tw:grid-cols-[32px_minmax(0,1fr)] tw:gap-x-3 tw:gap-y-2 tw:border-t tw:border-border-muted tw:bg-transparent tw:py-3 tw:text-status-working-foreground tw:first:border-t-0"
        }
        UiStepState::Complete => {
            "tw:grid tw:grid-cols-[32px_minmax(0,1fr)] tw:gap-x-3 tw:gap-y-2 tw:border-t tw:border-border-muted tw:bg-transparent tw:py-3 tw:text-status-good-foreground tw:first:border-t-0"
        }
        UiStepState::NeedsAttention => {
            "tw:grid tw:grid-cols-[32px_minmax(0,1fr)] tw:gap-x-3 tw:gap-y-2 tw:border-t tw:border-border-muted tw:bg-transparent tw:py-3 tw:text-status-error-foreground tw:first:border-t-0"
        }
    }
}

fn stack_marker_class(state: UiStepState) -> &'static str {
    match state {
        UiStepState::Pending => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:self-center tw:rounded-full tw:border tw:border-current tw:bg-transparent tw:text-subtle-foreground"
        }
        UiStepState::Active => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:self-center tw:rounded-full tw:border tw:border-current tw:bg-step-active tw:text-status-working-foreground"
        }
        UiStepState::Complete => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:self-center tw:rounded-full tw:border tw:border-current tw:bg-status-good-bg tw:text-status-good-foreground"
        }
        UiStepState::NeedsAttention => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:self-center tw:rounded-full tw:border tw:border-current tw:bg-status-error-bg tw:text-status-error-foreground"
        }
    }
}

fn stack_marker_icon(state: UiStepState) -> Option<StudioIconName> {
    match state {
        UiStepState::Pending => None,
        UiStepState::Active => Some(StudioIconName::StepActive),
        UiStepState::Complete => Some(StudioIconName::StepComplete),
        UiStepState::NeedsAttention => Some(StudioIconName::StepAttention),
    }
}

fn step_state_label(state: UiStepState) -> &'static str {
    match state {
        UiStepState::Pending => "pending",
        UiStepState::Active => "active",
        UiStepState::Complete => "complete",
        UiStepState::NeedsAttention => "needs attention",
    }
}
