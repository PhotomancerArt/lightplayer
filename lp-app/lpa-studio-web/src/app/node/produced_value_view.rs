//! Leaf presentation for a produced value stat.

use dioxus::prelude::*;
use lpa_studio_core::{UiProducedValue, UiSlotUnit};

use crate::app::node::{SlotDetailButton, SlotUnitSuffix};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedValueView(
    value: UiProducedValue,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let aspects = value.visible_aspects();
    let unit = value.display_unit();
    let detail = produced_value_detail(value.detail.as_ref(), unit.as_ref());

    rsx! {
        div { class: "tw:grid tw:min-h-20 tw:min-w-0 tw:content-between tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-2",
            dd { class: "tw:m-0 tw:flex tw:min-w-0 tw:items-baseline tw:gap-1 tw:leading-none",
                strong { class: "tw:min-w-0 tw:text-xl tw:font-bold tw:text-strong-foreground tw:break-words", "{value.value}" }
                SlotUnitSuffix { unit, reserve: false }
                if let Some(detail) = detail {
                    small { class: "tw:text-xs tw:font-bold tw:text-subtle-foreground tw:break-words", "{detail}" }
                }
            }
            dt { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-1.5 tw:text-xs tw:font-bold tw:leading-tight tw:text-subtle-foreground",
                span { class: "tw:min-w-0 tw:break-words", "{value.label}" }
                SlotDetailButton {
                    label: value.label.clone(),
                    aspects,
                    initially_open,
                }
            }
        }
    }
}

fn produced_value_detail(detail: Option<&String>, unit: Option<&UiSlotUnit>) -> Option<String> {
    let detail = detail?;
    let is_unit = unit.is_some_and(|unit| detail == &unit.short || detail == &unit.long)
        || UiSlotUnit::from_known_label(detail).is_some();
    (!is_unit).then(|| detail.clone())
}
