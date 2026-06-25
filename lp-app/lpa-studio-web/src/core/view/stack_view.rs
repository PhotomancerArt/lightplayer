use crate::core::{ActionStrip, TerminalOutput, ViewContent};
use dioxus::prelude::*;
use lpa_studio_core::core::view::steps_view::UiStepState;
use lpa_studio_core::{UiAction, UiStepsView};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StepsView(stack: UiStepsView, running: bool, on_action: EventHandler<UiAction>) -> Element {
    let terminal = stack.terminal;
    let sections = stack
        .sections
        .into_iter()
        .enumerate()
        .map(|(index, section)| (index + 1, section))
        .collect::<Vec<_>>();

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-4",
            ol { class: "tw:m-0 tw:grid tw:list-none tw:gap-0 tw:p-0",
                for (step_number, section) in sections {
                    li { class: "{stack_section_class(section.state)}",
                        div { class: "tw:inline-flex tw:mt-px tw:h-6 tw:w-6 tw:items-center tw:justify-center tw:rounded-full tw:border tw:border-current tw:bg-step-marker tw:text-xs tw:font-bold tw:leading-none", "{step_number}" }
                        div { class: "tw:grid tw:min-w-0 tw:gap-2",
                            h3 { class: "tw:m-0 tw:text-base tw:font-bold tw:leading-tight tw:text-strong-foreground tw:break-words", "{section.title}" }
                            div { class: "tw:min-w-0",
                                ViewContent {
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
            "tw:grid tw:grid-cols-[28px_minmax(0,1fr)] tw:gap-3 tw:border-t tw:border-border-muted tw:bg-transparent tw:py-3 tw:text-subtle-foreground tw:first:border-t-0"
        }
        UiStepState::Active => {
            "tw:grid tw:grid-cols-[28px_minmax(0,1fr)] tw:gap-3 tw:border-t tw:border-border-muted tw:bg-transparent tw:py-3 tw:text-status-working-foreground tw:first:border-t-0"
        }
        UiStepState::Complete => {
            "tw:grid tw:grid-cols-[28px_minmax(0,1fr)] tw:gap-3 tw:border-t tw:border-border-muted tw:bg-transparent tw:py-3 tw:text-status-good-foreground tw:first:border-t-0"
        }
        UiStepState::NeedsAttention => {
            "tw:grid tw:grid-cols-[28px_minmax(0,1fr)] tw:gap-3 tw:border-t tw:border-border-muted tw:bg-transparent tw:py-3 tw:text-status-error-foreground tw:first:border-t-0"
        }
    }
}
