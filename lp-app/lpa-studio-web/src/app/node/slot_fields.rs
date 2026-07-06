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
use wasm_bindgen::JsCast;

use crate::app::node::slot_edit_actions::slot_set_value_action;
use crate::app::node::{SlotRawInputPopover, SlotUnitSuffix, VectorSlotField};

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
/// Wired sliders also carry the raw-input detail popup — exact numeric entry
/// against the same slot path (`onchange` semantics), the second view onto
/// the one path-keyed buffer entry.
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
    /// Open the raw-input popup on first render (stories).
    #[props(default = false)]
    raw_initially_open: bool,
) -> Element {
    let wired = field_wiring(&state, &address, on_action);
    let raw_input = wired.clone();
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
            SlotUnitSuffix { unit: unit.clone(), reserve: true }
            if let Some((address, handler)) = raw_input {
                SlotRawInputPopover { initially_open: raw_initially_open,
                    FloatSlotField {
                        value,
                        state: state.clone(),
                        unit,
                        address: Some(address),
                        on_action: Some(handler),
                    }
                }
            }
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

/// XY pad for `Vec2` slots with an `Xy` editor hint. When the slot is
/// editable and addressed, dragging the pad dispatches `SlotEditOp::SetValue`
/// with `oninput` semantics — a continuous flood composing the WHOLE `Vec2`,
/// coalesced per address by the actor — and the raw-input detail popup offers
/// exact x/y entry against the same slot path (`onchange`). The component
/// readouts beside the pad stay read-only and stacked (x above y) at a fixed
/// tabular width, so nothing shifts while a drag floods new values; exact
/// (unrounded) entry and display live in the popup.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn XySlotField(
    value: [f32; 2],
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// Open the raw-input popup on first render (stories).
    #[props(default = false)]
    raw_initially_open: bool,
) -> Element {
    let x = value[0].clamp(0.0, 1.0) * 100.0;
    let y = (1.0 - value[1].clamp(0.0, 1.0)) * 100.0;
    let point_style = format!("left: {x:.1}%; top: {y:.1}%;");
    let pad_class = xy_pad_class(&state);
    let wired = field_wiring(&state, &address, on_action);
    let raw_input = wired.clone();
    let down_wiring = wired.clone();
    let move_wiring = wired.clone();
    let mut dragging = use_signal(|| false);

    rsx! {
        span { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
            span {
                class: pad_class,
                // Keep touch drags on the pad instead of scrolling the pane.
                style: if wired.is_some() { "touch-action: none; cursor: crosshair;" } else { "" },
                onpointerdown: move |event| {
                    let Some((address, handler)) = down_wiring.clone() else {
                        return;
                    };
                    capture_pad_pointer(&event);
                    dragging.set(true);
                    let point = event.data().element_coordinates();
                    handler.call(slot_set_value_action(address, xy_pad_value(point.x, point.y)));
                },
                onpointermove: move |event| {
                    if !dragging() {
                        return;
                    }
                    if event.data().held_buttons().is_empty() {
                        // Missed release (no pointer capture): stop the drag.
                        dragging.set(false);
                        return;
                    }
                    let Some((address, handler)) = move_wiring.clone() else {
                        return;
                    };
                    let point = event.data().element_coordinates();
                    handler.call(slot_set_value_action(address, xy_pad_value(point.x, point.y)));
                },
                onpointerup: move |_| dragging.set(false),
                onpointercancel: move |_| dragging.set(false),
                span { class: "tw:pointer-events-none tw:absolute tw:left-1/2 tw:top-0 tw:h-full tw:w-px tw:bg-border-muted" }
                span { class: "tw:pointer-events-none tw:absolute tw:left-0 tw:top-1/2 tw:h-px tw:w-full tw:bg-border-muted" }
                span { class: "tw:pointer-events-none tw:absolute tw:h-2 tw:w-2 tw:-translate-x-1/2 tw:-translate-y-1/2 tw:rounded-full tw:border tw:border-accent-border tw:bg-accent", style: "{point_style}" }
            }
            span { class: "tw:flex tw:min-w-0 tw:flex-col tw:justify-center tw:gap-1",
                XyPadReadout { label: "x", value: value[0], state: state.clone() }
                XyPadReadout { label: "y", value: value[1], state: state.clone() }
            }
            if let Some((address, handler)) = raw_input {
                SlotRawInputPopover { initially_open: raw_initially_open,
                    VectorSlotField {
                        kind: UiSlotValueKind::Vec2(value),
                        state: state.clone(),
                        address: Some(address),
                        on_action: Some(handler),
                    }
                }
            }
        }
    }
}

/// One read-only component readout row beside the XY pad. The value is
/// display-capped to three decimals at a fixed tabular/monospace width (sized
/// for the sign in `-0.000`), so the row's width never changes while a drag
/// floods new values. Exact values are never rounded on dispatch; the
/// raw-input popup shows and edits them unrounded.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn XyPadReadout(label: &'static str, value: f32, state: UiSlotFieldState) -> Element {
    rsx! {
        span { class: numeric_field_class(&state),
            small { class: "tw:text-[0.64rem] tw:font-bold tw:uppercase tw:text-subtle-foreground",
                "{label}"
            }
            span { class: "tw:w-[6ch] tw:text-right tw:font-mono tw:tabular-nums",
                "{format_xy_readout(value)}"
            }
        }
    }
}

/// Fixed-width display format for the XY pad readouts: always exactly three
/// decimals, so every value in the pad's domain renders at the same width
/// (plus one leading char for the sign). Display-only — dispatched values
/// are never rounded.
pub(crate) fn format_xy_readout(value: f32) -> String {
    format!("{value:.3}")
}

/// CSS pixel size of the square XY pad (`tw:h-14 tw:w-14` = 3.5rem at the
/// 16px root), used to normalize pad-relative pointer offsets into the
/// pad's 0..=1 value domain.
const XY_PAD_SIZE_PX: f64 = 56.0;

/// Compose the WHOLE `Vec2` value for a pad-relative pointer position:
/// x maps left→right onto 0..=1, y maps bottom→top (screen y is inverted),
/// both clamped so drags past the pad edge pin to the domain boundary.
pub(crate) fn xy_pad_value(x: f64, y: f64) -> LpValue {
    let fx = (x / XY_PAD_SIZE_PX).clamp(0.0, 1.0) as f32;
    let fy = (1.0 - y / XY_PAD_SIZE_PX).clamp(0.0, 1.0) as f32;
    LpValue::Vec2([fx, fy])
}

/// Route subsequent pointer events to the pad for the duration of the drag
/// (pointer capture), so a drag can leave the small pad and keep updating
/// with edge-clamped values. No-op outside a real browser event.
fn capture_pad_pointer(event: &Event<PointerData>) {
    use dioxus::web::WebEventExt;

    if let Some(web_event) = event.data().try_as_web_event()
        && let Some(target) = web_event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
    {
        let _ = target.set_pointer_capture(web_event.pointer_id());
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

pub(crate) fn dropdown_field_class(state: &UiSlotFieldState) -> &'static str {
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
    use super::{
        format_xy_readout, parse_f32_input, parse_i32_input, parse_u32_input, xy_pad_value,
    };
    use lpa_studio_core::LpValue;

    #[test]
    fn xy_readout_display_is_fixed_three_decimals() {
        // Display-only cap: always exactly three decimals so the readout
        // column never changes width during a drag.
        assert_eq!(format_xy_readout(0.42), "0.420");
        assert_eq!(format_xy_readout(0.0), "0.000");
        assert_eq!(format_xy_readout(1.0), "1.000");
        assert_eq!(format_xy_readout(-0.5), "-0.500");
        assert_eq!(format_xy_readout(0.123_456), "0.123");
    }

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

    #[test]
    fn xy_pad_composes_whole_vec2_with_inverted_y() {
        // Pad center → (0.5, 0.5); top-left corner → (0, 1) in value space.
        assert_eq!(xy_pad_value(28.0, 28.0), LpValue::Vec2([0.5, 0.5]));
        assert_eq!(xy_pad_value(0.0, 0.0), LpValue::Vec2([0.0, 1.0]));
        assert_eq!(xy_pad_value(56.0, 56.0), LpValue::Vec2([1.0, 0.0]));
    }

    #[test]
    fn xy_pad_clamps_out_of_pad_drags_to_the_domain_edge() {
        // Captured-pointer drags past the pad edge pin to 0..=1.
        assert_eq!(xy_pad_value(-20.0, 80.0), LpValue::Vec2([0.0, 0.0]));
        assert_eq!(xy_pad_value(90.0, -14.0), LpValue::Vec2([1.0, 1.0]));
    }
}
