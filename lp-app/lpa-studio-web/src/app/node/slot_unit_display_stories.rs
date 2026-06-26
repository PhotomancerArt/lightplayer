//! Stories for slot unit presentation.

use dioxus::prelude::*;
use lpa_studio_core::UiSlotUnit;
use lpa_studio_web_story_macros::story;

use crate::app::node::{SlotUnitDisplay, SlotUnitDisplayMode, SlotUnitSuffix};

#[story(description = "Short and long renderings for known slot units.")]
pub(crate) fn gallery() -> Element {
    let units = vec![
        UiSlotUnit::seconds(),
        UiSlotUnit::milliseconds(),
        UiSlotUnit::hertz(),
        UiSlotUnit::radians(),
        UiSlotUnit::percent(),
    ];

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:max-w-[360px] tw:gap-2",
            for unit in units {
                div { class: "tw:grid tw:min-w-0 tw:grid-cols-[80px_minmax(0,1fr)] tw:items-baseline tw:gap-3 tw:border-b tw:border-border-muted tw:pb-1.5",
                    SlotUnitDisplay {
                        unit: unit.clone(),
                        mode: SlotUnitDisplayMode::Short,
                    }
                    SlotUnitDisplay {
                        unit,
                        mode: SlotUnitDisplayMode::Long,
                    }
                }
            }
        }
    }
}

#[story(description = "Reserved unit suffix spacing used inside numeric fields.")]
pub(crate) fn suffix_spacing() -> Element {
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-center tw:gap-2",
            span { class: "tw:inline-flex tw:min-h-7 tw:items-baseline tw:justify-end tw:gap-1 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-muted-foreground",
                span { class: "tw:font-mono", "3.33" }
                SlotUnitSuffix {
                    unit: Some(UiSlotUnit::seconds()),
                    reserve: true,
                }
            }
            span { class: "tw:inline-flex tw:min-h-7 tw:items-baseline tw:justify-end tw:gap-1 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-muted-foreground",
                span { class: "tw:font-mono", "128" }
                SlotUnitSuffix {
                    unit: None,
                    reserve: true,
                }
            }
        }
    }
}
