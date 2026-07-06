//! Basic field renderers for scalar config slot values.
//!
//! All scalar fields are editable (M2/M3): when a slot address and the
//! `on_action` conduit are present, input dispatches `SlotEditOp::SetValue`.
//! Sliders (and other rich controls) dispatch with `oninput` semantics;
//! text/number inputs dispatch with `onchange` semantics (blur/enter, not
//! per keystroke — roadmap D5). Fields render the DTO value only — the edit
//! buffer and overlay mirror already shadow the synced value, so no field
//! keeps local value state.

use dioxus::prelude::*;
use lpa_studio_core::{
    LpValue, ProjectSlotAddress, UiAction, UiSlotFieldState, UiSlotOption, UiSlotUnit,
    UiSlotValueKind,
};

use crate::app::node::slot_edit_actions::slot_set_value_action;
use crate::app::node::{SlotUnitSuffix, VectorSlotField};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StringSlotField(
    value: String,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let Some((address, handler)) = field_wiring(&state, &address, on_action) else {
        return rsx! {
            span { class: field_class(&state), "{value}" }
        };
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();

    rsx! {
        span { class: field_class(&state), title: "{invalid_title}",
            input {
                class: "tw:w-full tw:min-w-0 tw:border-0 tw:bg-transparent tw:p-0 tw:text-inherit tw:outline-none",
                r#type: "text",
                value: "{value}",
                onchange: move |event| {
                    handler.call(slot_set_value_action(address.clone(), LpValue::String(event.value())));
                },
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn IntSlotField(
    value: i32,
    state: UiSlotFieldState,
    #[props(default = None)] unit: Option<UiSlotUnit>,
    #[props(default = None)] min: Option<f32>,
    #[props(default = None)] max: Option<f32>,
    #[props(default = None)] step: Option<f32>,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let Some((address, handler)) = field_wiring(&state, &address, on_action) else {
        return rsx! {
            span { class: numeric_field_class(&state),
                span { class: "tw:font-mono", "{value}" }
                SlotUnitSuffix { unit, reserve: true }
            }
        };
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();
    let step = step.map_or_else(|| "1".to_string(), |step| step.to_string());

    rsx! {
        span { class: numeric_field_class(&state), title: "{invalid_title}",
            input {
                class: scalar_number_input_class(),
                r#type: "number",
                min: min.map(|min| min.to_string()),
                max: max.map(|max| max.to_string()),
                step: "{step}",
                value: "{value}",
                onchange: move |event| {
                    if let Some(next) = parse_i32_input(&event.value()) {
                        handler.call(slot_set_value_action(address.clone(), LpValue::I32(next)));
                    }
                },
            }
            SlotUnitSuffix { unit, reserve: true }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn UIntSlotField(
    value: u32,
    state: UiSlotFieldState,
    #[props(default = None)] unit: Option<UiSlotUnit>,
    #[props(default = None)] min: Option<f32>,
    #[props(default = None)] max: Option<f32>,
    #[props(default = None)] step: Option<f32>,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let Some((address, handler)) = field_wiring(&state, &address, on_action) else {
        return rsx! {
            span { class: numeric_field_class(&state),
                span { class: "tw:font-mono", "{value}" }
                SlotUnitSuffix { unit, reserve: true }
            }
        };
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();
    let step = step.map_or_else(|| "1".to_string(), |step| step.to_string());

    rsx! {
        span { class: numeric_field_class(&state), title: "{invalid_title}",
            input {
                class: scalar_number_input_class(),
                r#type: "number",
                min: min.map_or_else(|| "0".to_string(), |min| min.to_string()),
                max: max.map(|max| max.to_string()),
                step: "{step}",
                value: "{value}",
                onchange: move |event| {
                    if let Some(next) = parse_u32_input(&event.value()) {
                        handler.call(slot_set_value_action(address.clone(), LpValue::U32(next)));
                    }
                },
            }
            SlotUnitSuffix { unit, reserve: true }
        }
    }
}

/// Plain numeric field for `F32` slots without a `Slider` hint.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FloatSlotField(
    value: f32,
    state: UiSlotFieldState,
    #[props(default = None)] unit: Option<UiSlotUnit>,
    #[props(default = None)] min: Option<f32>,
    #[props(default = None)] max: Option<f32>,
    #[props(default = None)] step: Option<f32>,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let Some((address, handler)) = field_wiring(&state, &address, on_action) else {
        return rsx! {
            span { class: numeric_field_class(&state),
                span { class: "tw:font-mono", "{format_float(value)}" }
                SlotUnitSuffix { unit, reserve: true }
            }
        };
    };
    let invalid_title = state.invalid.clone().unwrap_or_default();
    let step = step.map_or_else(|| "any".to_string(), |step| step.to_string());

    rsx! {
        span { class: numeric_field_class(&state), title: "{invalid_title}",
            input {
                class: scalar_number_input_class(),
                r#type: "number",
                min: min.map(|min| min.to_string()),
                max: max.map(|max| max.to_string()),
                step: "{step}",
                value: "{value}",
                onchange: move |event| {
                    if let Some(next) = parse_f32_input(&event.value()) {
                        handler.call(slot_set_value_action(address.clone(), LpValue::F32(next)));
                    }
                },
            }
            SlotUnitSuffix { unit, reserve: true }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn BoolSlotField(
    value: bool,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let true_class = bool_option_class(&state, value);
    let false_class = bool_option_class(&state, !value);
    let wired = field_wiring(&state, &address, on_action);
    let invalid_title = state.invalid.clone().unwrap_or_default();

    rsx! {
        span {
            class: "tw:inline-grid tw:grid-cols-2 tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:text-xs tw:font-bold",
            title: "{invalid_title}",
            if let Some((address, handler)) = wired {
                BoolOptionButton {
                    label: "true",
                    class: true_class,
                    address: address.clone(),
                    target: true,
                    handler,
                }
                BoolOptionButton {
                    label: "false",
                    class: false_class,
                    address,
                    target: false,
                    handler,
                }
            } else {
                span { class: true_class, "true" }
                span { class: false_class, "false" }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn BoolOptionButton(
    label: &'static str,
    class: &'static str,
    address: ProjectSlotAddress,
    target: bool,
    handler: EventHandler<UiAction>,
) -> Element {
    rsx! {
        button {
            class: "{class} tw:cursor-pointer tw:appearance-none tw:border-0 tw:hover:text-strong-foreground",
            r#type: "button",
            onclick: move |event| {
                event.stop_propagation();
                handler.call(slot_set_value_action(address.clone(), LpValue::Bool(target)));
            },
            "{label}"
        }
    }
}

/// Slider field for `F32` slots with a `Slider` editor hint.
///
/// Dispatches `SlotEditOp::SetValue` on `oninput`, so the running project
/// re-paces while the user drags; the actor coalesces the flood per address.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SliderSlotField(
    value: f32,
    min: f32,
    max: f32,
    #[props(default = None)] step: Option<f32>,
    state: UiSlotFieldState,
    #[props(default = None)] unit: Option<UiSlotUnit>,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let wired = field_wiring(&state, &address, on_action);
    let disabled = wired.is_none();
    let step = step.map_or_else(|| "any".to_string(), |step| step.to_string());
    let invalid_title = state.invalid.clone().unwrap_or_default();

    rsx! {
        span {
            class: numeric_field_class(&state),
            title: "{invalid_title}",
            input {
                class: "ux-slot-slider",
                r#type: "range",
                min: "{min}",
                max: "{max}",
                step: "{step}",
                value: "{value}",
                disabled,
                oninput: move |event| {
                    if let (Some((address, handler)), Ok(next)) =
                        (wired.clone(), event.value().parse::<f32>())
                    {
                        handler.call(slot_set_value_action(address, LpValue::F32(next)));
                    }
                },
            }
            span { class: "tw:min-w-10 tw:text-right tw:font-mono", "{format_float(value)}" }
            SlotUnitSuffix { unit, reserve: true }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn DropdownSlotField(
    value: String,
    options: Vec<UiSlotOption>,
    state: UiSlotFieldState,
    /// Value family of the backing slot, used to type the dispatched value.
    #[props(default = None)]
    kind: Option<UiSlotValueKind>,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let wired = field_wiring(&state, &address, on_action);
    let invalid_title = state.invalid.clone().unwrap_or_default();

    if let Some((address, handler)) = wired {
        let current = value.clone();
        let dispatch_kind = kind.clone();
        return rsx! {
            select {
                class: dropdown_field_class(&state),
                title: "{invalid_title}",
                value: "{value}",
                oninput: move |event| {
                    let key = event.value();
                    if let Some(next) = dropdown_lp_value(dispatch_kind.as_ref(), &key) {
                        handler.call(slot_set_value_action(address.clone(), next));
                    }
                },
                for option in options.clone() {
                    option {
                        value: "{option.value}",
                        selected: option.value == current,
                        "{option.label}"
                    }
                }
            }
        };
    }

    let label = options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.as_str())
        .unwrap_or(value.as_str());

    rsx! {
        span { class: field_class(&state), title: "{invalid_title}",
            span { class: "tw:min-w-0 tw:truncate", "{label}" }
            span { class: "tw:text-subtle-foreground", "v" }
        }
    }
}

/// Display-only XY pad for `Vec2` slots with an `Xy` editor hint (rich XY
/// editing is roadmap M4); the component readouts render read-only.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn XySlotField(value: [f32; 2], state: UiSlotFieldState) -> Element {
    let x = value[0].clamp(0.0, 1.0) * 100.0;
    let y = (1.0 - value[1].clamp(0.0, 1.0)) * 100.0;
    let point_style = format!("left: {x:.1}%; top: {y:.1}%;");
    let pad_class = xy_pad_class(&state);

    rsx! {
        span { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
            span { class: pad_class,
                span { class: "tw:absolute tw:left-1/2 tw:top-0 tw:h-full tw:w-px tw:bg-border-muted" }
                span { class: "tw:absolute tw:left-0 tw:top-1/2 tw:h-px tw:w-full tw:bg-border-muted" }
                span { class: "tw:absolute tw:h-2 tw:w-2 tw:-translate-x-1/2 tw:-translate-y-1/2 tw:rounded-full tw:border tw:border-accent-border tw:bg-accent", style: "{point_style}" }
            }
            VectorSlotField {
                kind: UiSlotValueKind::Vec2(value),
                state,
            }
        }
    }
}

/// The `(address, handler)` pair a field needs to dispatch edits, present
/// only when the slot is editable, addressed, and the conduit is wired.
pub(crate) fn field_wiring(
    state: &UiSlotFieldState,
    address: &Option<ProjectSlotAddress>,
    on_action: Option<EventHandler<UiAction>>,
) -> Option<(ProjectSlotAddress, EventHandler<UiAction>)> {
    if !state.editable {
        return None;
    }
    Some((address.clone()?, on_action?))
}

/// Type a dropdown option key into the slot's value family. Dropdowns over
/// families without a scalar key stay read-only elsewhere; `None` here means
/// "do not dispatch".
fn dropdown_lp_value(kind: Option<&UiSlotValueKind>, key: &str) -> Option<LpValue> {
    match kind? {
        UiSlotValueKind::String(_) => Some(LpValue::String(key.to_string())),
        UiSlotValueKind::I32(_) => key.parse().ok().map(LpValue::I32),
        UiSlotValueKind::U32(_) => key.parse().ok().map(LpValue::U32),
        UiSlotValueKind::F32(_) => key.parse().ok().map(LpValue::F32),
        UiSlotValueKind::Bool(_) => key.parse().ok().map(LpValue::Bool),
        _ => None,
    }
}

fn dropdown_field_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:max-w-full tw:cursor-pointer tw:items-center tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-status-error-foreground"
    } else {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:max-w-full tw:cursor-pointer tw:items-center tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-muted-foreground"
    }
}

/// Parse a signed integer input, clamping to the `i32` range. `None` means
/// "do not dispatch".
pub(crate) fn parse_i32_input(raw: &str) -> Option<i32> {
    let parsed = raw.trim().parse::<i64>().ok()?;
    Some(parsed.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
}

/// Parse an unsigned integer input, clamping to the `u32` range. `None`
/// means "do not dispatch".
pub(crate) fn parse_u32_input(raw: &str) -> Option<u32> {
    let parsed = raw.trim().parse::<i64>().ok()?;
    Some(parsed.clamp(0, i64::from(u32::MAX)) as u32)
}

/// Parse a finite float input. `None` means "do not dispatch".
pub(crate) fn parse_f32_input(raw: &str) -> Option<f32> {
    raw.trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

fn scalar_number_input_class() -> &'static str {
    "tw:w-16 tw:min-w-0 tw:border-0 tw:bg-transparent tw:p-0 tw:text-right tw:font-mono tw:text-inherit tw:outline-none"
}

pub(crate) fn field_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-status-error-foreground"
    } else if state.editable {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-muted-foreground"
    } else {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-subtle-foreground"
    }
}

pub(crate) fn numeric_field_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-baseline tw:justify-end tw:gap-1 tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-status-error-foreground"
    } else if state.editable {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-baseline tw:justify-end tw:gap-1 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-muted-foreground"
    } else {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-baseline tw:justify-end tw:gap-1 tw:rounded-xs tw:border tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-subtle-foreground"
    }
}

fn bool_option_class(state: &UiSlotFieldState, active: bool) -> &'static str {
    match (state.invalid.is_some(), active) {
        (true, true) => "tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-status-error-foreground",
        (true, false) => "tw:bg-page tw:px-2 tw:py-1 tw:text-subtle-foreground",
        (false, true) => "tw:bg-accent-bg tw:px-2 tw:py-1 tw:text-accent",
        (false, false) => "tw:bg-page tw:px-2 tw:py-1 tw:text-subtle-foreground",
    }
}

fn xy_pad_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:relative tw:h-14 tw:w-14 tw:flex-none tw:overflow-hidden tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg"
    } else {
        "tw:relative tw:h-14 tw:w-14 tw:flex-none tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page"
    }
}

pub(crate) fn format_float(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        let formatted = format!("{value:.3}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_f32_input, parse_i32_input, parse_u32_input};

    #[test]
    fn parses_and_clamps_signed_integer_input() {
        assert_eq!(parse_i32_input("-4"), Some(-4));
        assert_eq!(parse_i32_input(" 12 "), Some(12));
        assert_eq!(parse_i32_input("99999999999"), Some(i32::MAX));
        assert_eq!(parse_i32_input("-99999999999"), Some(i32::MIN));
        assert_eq!(parse_i32_input("abc"), None);
        assert_eq!(parse_i32_input(""), None);
    }

    #[test]
    fn parses_and_clamps_unsigned_integer_input() {
        assert_eq!(parse_u32_input("128"), Some(128));
        assert_eq!(parse_u32_input("-7"), Some(0));
        assert_eq!(parse_u32_input("99999999999"), Some(u32::MAX));
        assert_eq!(parse_u32_input("1.5"), None);
        assert_eq!(parse_u32_input(""), None);
    }

    #[test]
    fn parses_finite_float_input_only() {
        assert_eq!(parse_f32_input("0.35"), Some(0.35));
        assert_eq!(parse_f32_input(" -2 "), Some(-2.0));
        assert_eq!(parse_f32_input("inf"), None);
        assert_eq!(parse_f32_input("NaN"), None);
        assert_eq!(parse_f32_input("abc"), None);
        assert_eq!(parse_f32_input(""), None);
    }
}
