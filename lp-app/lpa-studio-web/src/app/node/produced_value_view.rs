//! Leaf presentation for a produced value stat.

use dioxus::prelude::*;
use lpa_studio_core::UiProducedValue;

use crate::app::node::{DirtyMark, ProducedBindingMark};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedValueView(value: UiProducedValue) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-h-20 tw:min-w-0 tw:content-between tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-2",
            dd { class: "tw:m-0 tw:flex tw:min-w-0 tw:items-baseline tw:gap-1 tw:leading-none",
                strong { class: "tw:min-w-0 tw:text-xl tw:font-bold tw:text-strong-foreground tw:break-words", "{value.value}" }
                if let Some(detail) = value.detail.as_ref() {
                    small { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground tw:break-words", "{detail}" }
                }
            }
            dt { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:text-xs tw:font-bold tw:leading-tight tw:text-subtle-foreground",
                span { class: "tw:min-w-0 tw:break-words", "{value.label}" }
                ProducedBindingMark {
                    label: value.label.clone(),
                    bindings: value.binding.bindings.clone(),
                }
                DirtyMark { dirty: value.dirty }
            }
        }
    }
}
