//! Lamp type + supply budget field for `Power`-hinted struct slot values.
//!
//! `PowerSlotField` renders a `FixturePower`-shaped struct value (`lamp_type`
//! string and `budget_ma` u32 fields) as one paired control — a lamp-type
//! picker and a milliamp budget input — instead of a generic struct display.
//! When the slot is editable and addressed, editing either component
//! read-modify-writes the WHOLE struct `LpValue` and dispatches a single
//! `SetValue`, mirroring `DimensionsSlotField`.
//!
//! A budget of zero means unlimited (the model's documented opt-out), so the
//! budget input accepts it and the read-only form says "unlimited" instead of
//! pretending "0 mA" is a budget.

use dioxus::prelude::*;
use lpa_studio_core::{
    LampType, LpValue, ProjectSlotAddress, UiAction, UiSlotFieldState, UiSlotValueKind,
};

use crate::app::node::slot_edit_actions::slot_set_value_action;
use crate::app::node::slot_fields::{
    dropdown_field_class, field_wiring, numeric_field_class, parse_u32_input,
};

/// The lamp/budget component pair carried by a power struct value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PowerParts {
    pub lamp_type: LampType,
    pub budget_ma: u32,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PowerSlotField(
    kind: UiSlotValueKind,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let Some(parts) = power_parts(&kind) else {
        return rsx! {};
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();

    let Some((address, handler)) = field_wiring(&state, &address, on_action) else {
        return rsx! {
            span { class: numeric_field_class(&state), title: "{invalid_title}",
                span { "{parts.lamp_type.display_name()}" }
                span { class: "tw:text-subtle-foreground", "\u{b7}" }
                if parts.budget_ma == 0 {
                    span { "unlimited" }
                } else {
                    span { class: "tw:font-mono", "{parts.budget_ma}" }
                    span { class: "tw:text-subtle-foreground", "mA" }
                }
            }
        };
    };

    let lamp_kind = kind.clone();
    let lamp_address = address.clone();
    rsx! {
        span { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1",
            select {
                class: dropdown_field_class(&state),
                title: "Lamp part family — selects the power model",
                value: "{parts.lamp_type.as_str()}",
                oninput: move |event| {
                    if let Some(next) = power_set_lamp(&lamp_kind, &event.value()) {
                        handler.call(slot_set_value_action(lamp_address.clone(), next));
                    }
                },
                for lamp in LampType::ALL {
                    option {
                        value: "{lamp.as_str()}",
                        selected: *lamp == parts.lamp_type,
                        "{lamp.display_name()}"
                    }
                }
            }
            span { class: numeric_field_class(&state), title: "{invalid_title}",
                input {
                    class: "tw:w-14 tw:min-w-0 tw:border-0 tw:bg-transparent tw:p-0 tw:text-right tw:font-mono tw:text-inherit tw:outline-none",
                    r#type: "number",
                    min: "0",
                    step: "50",
                    value: "{parts.budget_ma}",
                    aria_label: "Supply budget (mA)",
                    title: "Supply budget in mA \u{2014} 0 means unlimited",
                    onchange: move |event| {
                        if let Some(next) = power_set_budget(&kind, &event.value()) {
                            handler.call(slot_set_value_action(address.clone(), next));
                        }
                    },
                }
                span { class: "tw:flex-none tw:text-subtle-foreground", "mA" }
            }
        }
    }
}

/// Extract the lamp/budget pair from a power struct value. `None` when the
/// kind is not a `FixturePower`-shaped struct (a parseable `lamp_type` string
/// and a `budget_ma` u32) — the caller falls back to the generic display.
pub(crate) fn power_parts(kind: &UiSlotValueKind) -> Option<PowerParts> {
    let UiSlotValueKind::Struct { fields, .. } = kind else {
        return None;
    };
    if fields.len() != 2 {
        return None;
    }
    let lamp_type =
        fields.iter().find_map(
            |(field, value)| match (&value.kind, field == FIELD_LAMP_TYPE) {
                (UiSlotValueKind::String(value), true) => LampType::parse(value),
                _ => None,
            },
        )?;
    let budget_ma =
        fields.iter().find_map(
            |(field, value)| match (&value.kind, field == FIELD_BUDGET_MA) {
                (UiSlotValueKind::U32(value), true) => Some(*value),
                _ => None,
            },
        )?;
    Some(PowerParts {
        lamp_type,
        budget_ma,
    })
}

const FIELD_LAMP_TYPE: &str = "lamp_type";
const FIELD_BUDGET_MA: &str = "budget_ma";

/// Replace the lamp type (parsed from a picker key) and return the composed
/// WHOLE struct value for a single `SetValue` dispatch. `None` means "do not
/// dispatch" (not a power struct or an unknown lamp key).
pub(crate) fn power_set_lamp(kind: &UiSlotValueKind, key: &str) -> Option<LpValue> {
    let parts = power_parts(kind)?;
    let lamp_type = LampType::parse(key)?;
    compose_power(kind, lamp_type, parts.budget_ma)
}

/// Replace the budget (parsed as u32 from `raw`, zero allowed — it means
/// unlimited) and return the composed WHOLE struct value for a single
/// `SetValue` dispatch. `None` means "do not dispatch".
pub(crate) fn power_set_budget(kind: &UiSlotValueKind, raw: &str) -> Option<LpValue> {
    let parts = power_parts(kind)?;
    let budget_ma = parse_u32_input(raw)?;
    compose_power(kind, parts.lamp_type, budget_ma)
}

/// Compose the whole struct value, preserving the struct name and field
/// order carried by the current value.
fn compose_power(kind: &UiSlotValueKind, lamp_type: LampType, budget_ma: u32) -> Option<LpValue> {
    let UiSlotValueKind::Struct { name, fields } = kind else {
        return None;
    };
    Some(LpValue::Struct {
        name: name.clone(),
        fields: fields
            .iter()
            .map(|(field, _)| {
                let value = if field == FIELD_LAMP_TYPE {
                    LpValue::String(lamp_type.as_str().to_string())
                } else {
                    LpValue::U32(budget_ma)
                };
                (field.clone(), value)
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::{PowerParts, power_parts, power_set_budget, power_set_lamp};
    use lpa_studio_core::{LampType, LpValue, UiSlotValue, UiSlotValueKind};

    fn power_kind(lamp: &str, budget_ma: u32) -> UiSlotValueKind {
        UiSlotValueKind::Struct {
            name: Some("FixturePower".to_string()),
            fields: vec![
                ("lamp_type".to_string(), UiSlotValue::string(lamp)),
                ("budget_ma".to_string(), UiSlotValue::u32(budget_ma)),
            ],
        }
    }

    fn power_lp_value(lamp: &str, budget_ma: u32) -> LpValue {
        LpValue::Struct {
            name: Some("FixturePower".to_string()),
            fields: vec![
                ("lamp_type".to_string(), LpValue::String(lamp.to_string())),
                ("budget_ma".to_string(), LpValue::U32(budget_ma)),
            ],
        }
    }

    #[test]
    fn extracts_lamp_and_budget_from_power_struct() {
        assert_eq!(
            power_parts(&power_kind("ws2811_12v", 2500)),
            Some(PowerParts {
                lamp_type: LampType::Ws281112v,
                budget_ma: 2500,
            })
        );
    }

    #[test]
    fn rejects_non_power_kinds_and_unknown_lamps() {
        assert_eq!(power_parts(&UiSlotValueKind::U32(1000)), None);
        assert_eq!(power_parts(&power_kind("nonsense", 1000)), None);
        assert_eq!(
            power_parts(&UiSlotValueKind::Struct {
                name: None,
                fields: vec![("lamp_type".to_string(), UiSlotValue::string("ws2812b_5v"))],
            }),
            None
        );
    }

    #[test]
    fn composes_whole_struct_on_lamp_change() {
        assert_eq!(
            power_set_lamp(&power_kind("ws2812b_5v", 1000), "ws2815_12v"),
            Some(power_lp_value("ws2815_12v", 1000))
        );
        assert_eq!(
            power_set_lamp(&power_kind("ws2812b_5v", 1000), "nonsense"),
            None,
            "an unknown lamp key must not dispatch"
        );
    }

    #[test]
    fn composes_whole_struct_on_budget_change_and_zero_is_allowed() {
        assert_eq!(
            power_set_budget(&power_kind("ws2812b_5v", 1000), "2500"),
            Some(power_lp_value("ws2812b_5v", 2500))
        );
        // Zero is the documented unlimited opt-out, not a rejected input.
        assert_eq!(
            power_set_budget(&power_kind("ws2812b_5v", 1000), "0"),
            Some(power_lp_value("ws2812b_5v", 0))
        );
        assert_eq!(
            power_set_budget(&power_kind("ws2812b_5v", 1000), "abc"),
            None
        );
    }
}
