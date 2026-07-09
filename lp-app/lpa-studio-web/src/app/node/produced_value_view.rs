//! Leaf presentation for a produced value stat.

use dioxus::prelude::*;
use lpa_studio_core::{UiProducedValue, UiSlotUnit};

use crate::app::node::{
    BindingChip, BindingChipDirection, SlotPane, SlotPaneTreatment, SlotUnitDisplay,
    SlotUnitDisplayMode,
};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedValueView(
    value: UiProducedValue,
    #[props(default = false)] initially_open: bool,
) -> Element {
    let aspects = value.visible_aspects();
    let unit = value.display_unit();
    let display_value = produced_value_display(&value.value, unit.as_ref());
    let reading_class = produced_value_reading_class(unit.is_some());
    let bus_target = value.binding.bindings.bus_target.clone();
    let treatment = if bus_target.is_some() {
        SlotPaneTreatment::Bound
    } else {
        SlotPaneTreatment::Neutral
    };

    rsx! {
        SlotPane {
            title: value.label.clone(),
            aspects,
            initially_open,
            treatment,
            span { class: "{reading_class}",
                strong { class: "ux-produced-value-number", "{display_value}" }
                if let Some(unit) = unit {
                    span { class: "ux-produced-value-unit",
                        SlotUnitDisplay {
                            unit,
                            mode: SlotUnitDisplayMode::Short,
                        }
                    }
                }
            }
            if let Some(endpoint) = bus_target {
                BindingChip {
                    endpoint,
                    direction: BindingChipDirection::Publishes,
                }
            }
        }
    }
}

fn produced_value_display(value: &str, unit: Option<&UiSlotUnit>) -> String {
    let trimmed = value.trim();
    let Some(decimals) = produced_value_decimal_places(trimmed, unit) else {
        return value.to_string();
    };
    let Ok(number) = trimmed.parse::<f64>() else {
        return value.to_string();
    };
    if number.is_finite() {
        format!("{number:.decimals$}")
    } else {
        value.to_string()
    }
}

fn produced_value_decimal_places(value: &str, unit: Option<&UiSlotUnit>) -> Option<usize> {
    if unit.is_some_and(|unit| unit.short == "s" || unit.short == "ms") {
        return Some(3);
    }
    (value.contains('.') || value.contains('e') || value.contains('E')).then_some(3)
}

fn produced_value_reading_class(has_unit: bool) -> &'static str {
    if has_unit {
        "ux-produced-value-reading ux-produced-value-reading-with-unit"
    } else {
        "ux-produced-value-reading"
    }
}
