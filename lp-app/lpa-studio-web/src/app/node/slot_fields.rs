//! Basic field renderers for the first config slot editor slice.
//!
//! Bool, slider, and dropdown fields are editable (M2): when a slot address
//! and the `on_action` conduit are present, input dispatches
//! `SlotEditOp::SetValue` with `oninput` semantics. Fields render the DTO
//! value only — the edit buffer and overlay mirror already shadow the synced
//! value, so no field keeps local value state.

use dioxus::prelude::*;
use lpa_studio_core::{
    LpValue, ProjectSlotAddress, UiAction, UiSlotFieldState, UiSlotOption, UiSlotUnit,
    UiSlotValueKind,
};

use crate::app::node::SlotUnitSuffix;
use crate::app::node::slot_edit_actions::slot_set_value_action;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StringSlotField(value: String, state: UiSlotFieldState) -> Element {
    rsx! {
        span { class: field_class(&state), "{value}" }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn IntSlotField(
    value: i32,
    state: UiSlotFieldState,
    #[props(default = None)] unit: Option<UiSlotUnit>,
) -> Element {
    rsx! {
        span { class: numeric_field_class(&state),
            span { class: "tw:font-mono", "{value}" }
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
) -> Element {
    rsx! {
        span { class: numeric_field_class(&state),
            span { class: "tw:font-mono", "{value}" }
            SlotUnitSuffix { unit, reserve: true }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn FloatSlotField(
    value: f32,
    state: UiSlotFieldState,
    #[props(default = None)] unit: Option<UiSlotUnit>,
) -> Element {
    rsx! {
        span { class: numeric_field_class(&state),
            span { class: "tw:font-mono", "{format_float(value)}" }
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
pub fn Vec2SlotField(value: [f32; 2], state: UiSlotFieldState) -> Element {
    rsx! {
        span { class: "tw:grid tw:min-w-0 tw:grid-cols-2 tw:gap-1",
            VectorComponentField { label: "x", value: value[0], state: state.clone() }
            VectorComponentField { label: "y", value: value[1], state }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn Vec3SlotField(value: [f32; 3], state: UiSlotFieldState) -> Element {
    rsx! {
        span { class: "tw:grid tw:min-w-0 tw:grid-cols-3 tw:gap-1",
            VectorComponentField { label: "x", value: value[0], state: state.clone() }
            VectorComponentField { label: "y", value: value[1], state: state.clone() }
            VectorComponentField { label: "z", value: value[2], state }
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
            Vec2SlotField {
                value,
                state,
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn VectorComponentField(label: &'static str, value: f32, state: UiSlotFieldState) -> Element {
    rsx! {
        span { class: numeric_field_class(&state),
            small { class: "tw:text-[0.64rem] tw:font-bold tw:uppercase tw:text-subtle-foreground", "{label}" }
            span { class: "tw:font-mono", "{format_float(value)}" }
        }
    }
}

/// The `(address, handler)` pair a field needs to dispatch edits, present
/// only when the slot is editable, addressed, and the conduit is wired.
fn field_wiring(
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

fn field_class(state: &UiSlotFieldState) -> &'static str {
    if state.invalid.is_some() {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-status-error-foreground"
    } else if state.editable {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-muted-foreground"
    } else {
        "tw:inline-flex tw:min-h-7 tw:min-w-0 tw:items-center tw:justify-between tw:gap-2 tw:rounded-xs tw:border tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1 tw:text-sm tw:font-medium tw:text-subtle-foreground"
    }
}

fn numeric_field_class(state: &UiSlotFieldState) -> &'static str {
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

fn format_float(value: f32) -> String {
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
