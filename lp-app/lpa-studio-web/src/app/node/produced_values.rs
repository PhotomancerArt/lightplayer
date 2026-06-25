use dioxus::prelude::*;
use lpa_studio_core::UiProducedValue;

use crate::app::node::{DirtyMark, ProducedBindingMark};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedValues(values: Vec<UiProducedValue>) -> Element {
    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:gap-2",
            h4 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:leading-none tw:text-heading", "Produced values" }
            dl { class: "tw:m-0 tw:grid tw:grid-cols-[repeat(auto-fit,minmax(140px,1fr))] tw:gap-2",
                for value in values {
                    div { class: "tw:min-w-0 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-2",
                        dt { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:text-[0.68rem] tw:font-bold tw:uppercase tw:leading-tight tw:text-subtle-foreground",
                            ProducedBindingMark {
                                label: value.label.clone(),
                                bindings: value.binding.bindings.clone(),
                            }
                            span { class: "tw:min-w-0 tw:break-words", "{value.label}" }
                            DirtyMark { dirty: value.dirty }
                        }
                        dd { class: "tw:m-0 tw:mt-1 tw:text-sm tw:font-bold tw:leading-tight tw:text-strong-foreground tw:break-words", "{value.value}" }
                        if let Some(detail) = value.detail.as_ref() {
                            small { class: "tw:mt-1 tw:block tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
                        }
                    }
                }
            }
        }
    }
}
