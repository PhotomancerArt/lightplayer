//! Shared presentation for Studio slot units.

use dioxus::prelude::*;
use lpa_studio_core::UiSlotUnit;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "short unit mode is exercised by story builds and compact callers"
)]
pub(crate) enum SlotUnitDisplayMode {
    Short,
    Long,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SlotUnitDisplay(unit: UiSlotUnit, mode: SlotUnitDisplayMode) -> Element {
    let label = match mode {
        SlotUnitDisplayMode::Short => unit.short,
        SlotUnitDisplayMode::Long => unit.long,
    };

    rsx! {
        span { class: "tw:text-subtle-foreground tw:break-words", "{label}" }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SlotUnitSuffix(
    unit: Option<UiSlotUnit>,
    #[props(default = false)] reserve: bool,
) -> Element {
    let class = unit_suffix_class(unit.is_some(), reserve);
    let label = unit
        .map(|unit| unit.short)
        .unwrap_or_else(|| "xx".to_string());

    rsx! {
        span { class, "{label}" }
    }
}

fn unit_suffix_class(visible: bool, reserve: bool) -> &'static str {
    match (visible, reserve) {
        (true, true) => {
            "tw:inline-flex tw:min-w-[2ch] tw:justify-start tw:text-xs tw:font-bold tw:text-subtle-foreground"
        }
        (true, false) => {
            "tw:inline-flex tw:justify-start tw:text-xs tw:font-bold tw:text-subtle-foreground"
        }
        (false, true) => {
            "tw:invisible tw:inline-flex tw:min-w-[2ch] tw:justify-start tw:text-xs tw:font-bold"
        }
        (false, false) => "tw:hidden",
    }
}
