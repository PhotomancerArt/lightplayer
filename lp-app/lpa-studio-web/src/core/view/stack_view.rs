use crate::core::{ActionStrip, ViewContent};
use dioxus::prelude::*;
use lpa_studio_core::core::view::steps_view::UiStepState;
use lpa_studio_core::{UiAction, UiStepsView};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StepsView(stack: UiStepsView, running: bool, on_action: EventHandler<UiAction>) -> Element {
    let sections = stack
        .sections
        .into_iter()
        .enumerate()
        .map(|(index, section)| (index + 1, section))
        .collect::<Vec<_>>();

    rsx! {
        div { class: "ux-stack",
            ol { class: "ux-stack-sections",
                for (step_number, section) in sections {
                    li { class: "{stack_section_class(section.state)}",
                        div { class: "ux-stack-section-marker", "{step_number}" }
                        div { class: "ux-stack-section-content",
                            h3 { "{section.title}" }
                            div { class: "ux-stack-section-body",
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
