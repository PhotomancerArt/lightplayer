//! Recursive record editor for config slot fields.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiSlotMapKeyKind, UiSlotRecord};

use crate::app::node::ConfigSlotRow;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SlotRecordEditor(
    record: UiSlotRecord,
    #[props(default = 0)] depth: usize,
    #[props(default = false)] separated: bool,
    /// Set when these rows are map entries of a map with this key domain:
    /// each row gets the per-entry remove affordance (`RemoveValue` at the
    /// entry path) and the click-to-edit key label (`MoveEntry`).
    #[props(default = None)]
    entry_key_kind: Option<UiSlotMapKeyKind>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let class = if separated {
        "tw:grid tw:min-w-0 tw:overflow-hidden tw:border-t tw:border-border-muted tw:divide-y tw:divide-border-muted"
    } else {
        "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted"
    };

    rsx! {
        div { class,
            for (index, slot) in record.fields.into_iter().enumerate() {
                ConfigSlotRow {
                    key: "{slot.key}",
                    slot,
                    depth,
                    index,
                    entry_key_kind,
                    on_action,
                }
            }
        }
    }
}
