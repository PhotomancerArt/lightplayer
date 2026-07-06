//! Component-grid field for vector slot values.
//!
//! One `VectorSlotField` covers every vector family (`Vec*`, `IVec*`,
//! `UVec*`, `BVec*`). When the slot is editable and addressed, numeric
//! components render as number inputs (`onchange` semantics — dispatch on
//! blur/enter, per roadmap D5) and boolean components as compact toggles;
//! otherwise the grid renders the read-only component spans. Editing one
//! component read-modify-writes the WHOLE `LpValue` and dispatches a single
//! `SetValue` (plan D3 — one address per leaf; the actor coalesces rapid
//! multi-component edits per address).

use dioxus::prelude::*;
use lpa_studio_core::{LpValue, ProjectSlotAddress, UiAction, UiSlotFieldState, UiSlotValueKind};

use crate::app::node::slot_edit_actions::slot_set_value_action;
use crate::app::node::slot_fields::{
    field_wiring, format_float, numeric_field_class, parse_f32_input, parse_i32_input,
    parse_u32_input,
};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn VectorSlotField(
    kind: UiSlotValueKind,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let len = vector_len(&kind);

    rsx! {
        span { class: vector_grid_class(len),
            for index in 0..len {
                VectorComponentCell {
                    kind: kind.clone(),
                    index,
                    state: state.clone(),
                    address: address.clone(),
                    on_action,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn VectorComponentCell(
    kind: UiSlotValueKind,
    index: usize,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let label = component_label(index);
    let display = vector_component_display(&kind, index);
    let invalid_title = state.invalid.clone().unwrap_or_default();

    let Some((address, handler)) = field_wiring(&state, &address, on_action) else {
        return rsx! {
            span { class: numeric_field_class(&state),
                VectorComponentLabel { label }
                span { class: "tw:font-mono", "{display}" }
            }
        };
    };

    if let Some(value) = vector_component_bool(&kind, index) {
        let toggle_kind = kind.clone();
        return rsx! {
            span { class: numeric_field_class(&state), title: "{invalid_title}",
                VectorComponentLabel { label }
                button {
                    class: "tw:cursor-pointer tw:appearance-none tw:border-0 tw:bg-transparent tw:p-0 tw:font-mono tw:text-inherit tw:hover:text-strong-foreground",
                    r#type: "button",
                    onclick: move |event| {
                        event.stop_propagation();
                        let flipped = if value { "false" } else { "true" };
                        if let Some(next) = vector_set_component(&toggle_kind, index, flipped) {
                            handler.call(slot_set_value_action(address.clone(), next));
                        }
                    },
                    "{value}"
                }
            }
        };
    }

    let step = vector_step(&kind);
    rsx! {
        span { class: numeric_field_class(&state), title: "{invalid_title}",
            VectorComponentLabel { label }
            input {
                class: "tw:w-full tw:min-w-0 tw:border-0 tw:bg-transparent tw:p-0 tw:text-right tw:font-mono tw:text-inherit tw:outline-none",
                r#type: "number",
                step,
                value: "{display}",
                onchange: move |event| {
                    if let Some(next) = vector_set_component(&kind, index, &event.value()) {
                        handler.call(slot_set_value_action(address.clone(), next));
                    }
                },
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn VectorComponentLabel(label: &'static str) -> Element {
    rsx! {
        small { class: "tw:text-[0.64rem] tw:font-bold tw:uppercase tw:text-subtle-foreground",
            "{label}"
        }
    }
}

fn component_label(index: usize) -> &'static str {
    ["x", "y", "z", "w"].get(index).copied().unwrap_or("?")
}

fn vector_grid_class(len: usize) -> &'static str {
    match len {
        2 => "tw:grid tw:min-w-0 tw:grid-cols-2 tw:gap-1",
        3 => "tw:grid tw:min-w-0 tw:grid-cols-3 tw:gap-1",
        _ => "tw:grid tw:min-w-0 tw:grid-cols-4 tw:gap-1",
    }
}

fn vector_len(kind: &UiSlotValueKind) -> usize {
    match kind {
        UiSlotValueKind::Vec2(_)
        | UiSlotValueKind::IVec2(_)
        | UiSlotValueKind::UVec2(_)
        | UiSlotValueKind::BVec2(_) => 2,
        UiSlotValueKind::Vec3(_)
        | UiSlotValueKind::IVec3(_)
        | UiSlotValueKind::UVec3(_)
        | UiSlotValueKind::BVec3(_) => 3,
        UiSlotValueKind::Vec4(_)
        | UiSlotValueKind::IVec4(_)
        | UiSlotValueKind::UVec4(_)
        | UiSlotValueKind::BVec4(_) => 4,
        _ => 0,
    }
}

fn vector_component_display(kind: &UiSlotValueKind, index: usize) -> String {
    let display = match kind {
        UiSlotValueKind::Vec2(values) => values.get(index).copied().map(format_float),
        UiSlotValueKind::Vec3(values) => values.get(index).copied().map(format_float),
        UiSlotValueKind::Vec4(values) => values.get(index).copied().map(format_float),
        UiSlotValueKind::IVec2(values) => values.get(index).map(ToString::to_string),
        UiSlotValueKind::IVec3(values) => values.get(index).map(ToString::to_string),
        UiSlotValueKind::IVec4(values) => values.get(index).map(ToString::to_string),
        UiSlotValueKind::UVec2(values) => values.get(index).map(ToString::to_string),
        UiSlotValueKind::UVec3(values) => values.get(index).map(ToString::to_string),
        UiSlotValueKind::UVec4(values) => values.get(index).map(ToString::to_string),
        UiSlotValueKind::BVec2(values) => values.get(index).map(ToString::to_string),
        UiSlotValueKind::BVec3(values) => values.get(index).map(ToString::to_string),
        UiSlotValueKind::BVec4(values) => values.get(index).map(ToString::to_string),
        _ => None,
    };
    display.unwrap_or_default()
}

fn vector_component_bool(kind: &UiSlotValueKind, index: usize) -> Option<bool> {
    match kind {
        UiSlotValueKind::BVec2(values) => values.get(index).copied(),
        UiSlotValueKind::BVec3(values) => values.get(index).copied(),
        UiSlotValueKind::BVec4(values) => values.get(index).copied(),
        _ => None,
    }
}

fn vector_step(kind: &UiSlotValueKind) -> &'static str {
    match kind {
        UiSlotValueKind::Vec2(_) | UiSlotValueKind::Vec3(_) | UiSlotValueKind::Vec4(_) => "any",
        _ => "1",
    }
}

/// Replace one component (parsed from `raw` in the vector's element family)
/// and return the composed WHOLE value for a single `SetValue` dispatch.
/// `None` means "do not dispatch" (non-vector kind, bad index, or no parse).
pub(crate) fn vector_set_component(
    kind: &UiSlotValueKind,
    index: usize,
    raw: &str,
) -> Option<LpValue> {
    Some(match kind {
        UiSlotValueKind::Vec2(values) => {
            LpValue::Vec2(with_component(values, index, parse_f32_input(raw)?)?)
        }
        UiSlotValueKind::Vec3(values) => {
            LpValue::Vec3(with_component(values, index, parse_f32_input(raw)?)?)
        }
        UiSlotValueKind::Vec4(values) => {
            LpValue::Vec4(with_component(values, index, parse_f32_input(raw)?)?)
        }
        UiSlotValueKind::IVec2(values) => {
            LpValue::IVec2(with_component(values, index, parse_i32_input(raw)?)?)
        }
        UiSlotValueKind::IVec3(values) => {
            LpValue::IVec3(with_component(values, index, parse_i32_input(raw)?)?)
        }
        UiSlotValueKind::IVec4(values) => {
            LpValue::IVec4(with_component(values, index, parse_i32_input(raw)?)?)
        }
        UiSlotValueKind::UVec2(values) => {
            LpValue::UVec2(with_component(values, index, parse_u32_input(raw)?)?)
        }
        UiSlotValueKind::UVec3(values) => {
            LpValue::UVec3(with_component(values, index, parse_u32_input(raw)?)?)
        }
        UiSlotValueKind::UVec4(values) => {
            LpValue::UVec4(with_component(values, index, parse_u32_input(raw)?)?)
        }
        UiSlotValueKind::BVec2(values) => {
            LpValue::BVec2(with_component(values, index, raw.trim().parse().ok()?)?)
        }
        UiSlotValueKind::BVec3(values) => {
            LpValue::BVec3(with_component(values, index, raw.trim().parse().ok()?)?)
        }
        UiSlotValueKind::BVec4(values) => {
            LpValue::BVec4(with_component(values, index, raw.trim().parse().ok()?)?)
        }
        _ => return None,
    })
}

fn with_component<T: Copy, const N: usize>(
    values: &[T; N],
    index: usize,
    value: T,
) -> Option<[T; N]> {
    if index >= N {
        return None;
    }
    let mut next = *values;
    next[index] = value;
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::vector_set_component;
    use lpa_studio_core::{LpValue, UiSlotValueKind};

    #[test]
    fn composes_whole_float_vector_from_one_component() {
        let kind = UiSlotValueKind::Vec3([1.0, 0.42, 0.2]);

        let value = vector_set_component(&kind, 1, "0.9");

        assert_eq!(value, Some(LpValue::Vec3([1.0, 0.9, 0.2])));
    }

    #[test]
    fn composes_signed_vector_with_type_range_clamp() {
        let kind = UiSlotValueKind::IVec2([-1, 2]);

        let value = vector_set_component(&kind, 0, "99999999999");

        assert_eq!(value, Some(LpValue::IVec2([i32::MAX, 2])));
    }

    #[test]
    fn composes_unsigned_vector_clamping_negatives_to_zero() {
        let kind = UiSlotValueKind::UVec4([1, 2, 3, 4]);

        let value = vector_set_component(&kind, 3, "-7");

        assert_eq!(value, Some(LpValue::UVec4([1, 2, 3, 0])));
    }

    #[test]
    fn composes_bool_vector_from_toggle_value() {
        let kind = UiSlotValueKind::BVec3([true, false, true]);

        let value = vector_set_component(&kind, 2, "false");

        assert_eq!(value, Some(LpValue::BVec3([true, false, false])));
    }

    #[test]
    fn rejects_unparseable_and_out_of_range_components() {
        let kind = UiSlotValueKind::Vec2([0.42, 0.58]);

        assert_eq!(vector_set_component(&kind, 0, "abc"), None);
        assert_eq!(vector_set_component(&kind, 0, ""), None);
        assert_eq!(vector_set_component(&kind, 2, "1.0"), None);
        assert_eq!(vector_set_component(&kind, 0, "inf"), None);
    }

    #[test]
    fn rejects_non_vector_kinds() {
        assert_eq!(
            vector_set_component(&UiSlotValueKind::F32(1.0), 0, "2.0"),
            None
        );
    }
}
