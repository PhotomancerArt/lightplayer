//! Recursive record editor for config slot fields.

use dioxus::prelude::*;
use lpa_studio_core::UiSlotRecord;

use crate::app::node::ConfigSlotRow;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SlotRecordEditor(record: UiSlotRecord, #[props(default = 0)] depth: usize) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:border-border-muted",
            for (index, slot) in record.fields.into_iter().enumerate() {
                ConfigSlotRow {
                    key: "{slot.key}",
                    slot,
                    depth,
                    index,
                }
            }
        }
    }
}
