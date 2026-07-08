//! Typed value dispatcher for config slot field components.
//!
//! Every scalar, vector, and matrix value kind resolves to an editor when
//! the slot is editable and addressed, and the specialized hints (`Xy`,
//! `Dimensions`, `Affine2d`) resolve to their rich editors (M4). Composite
//! bodies (`Array`, `Struct`, `Enum`) stay with the composite gesture
//! machinery (M3 P4); `Resource` and `Product` references are explicitly
//! read-only displays.

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectSlotAddress, UiAction, UiSlotEditorHint, UiSlotFieldState, UiSlotUnit, UiSlotValue,
    UiSlotValueKind,
};

use crate::app::node::slot_dimensions_field::dimensions_parts;
use crate::app::node::{
    Affine2dSlotField, BoolSlotField, DimensionsSlotField, DropdownSlotField, FloatSlotField,
    IntSlotField, MatrixSlotField, SliderSlotField, StringSlotField, UIntSlotField,
    VectorSlotField, XySlotField,
};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SlotValueEditor(
    value: UiSlotValue,
    state: UiSlotFieldState,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let unit = value.display_unit();

    match value.editor.clone() {
        UiSlotEditorHint::Dropdown(options) => rsx! {
            DropdownSlotField {
                value: slot_value_key(&value),
                options,
                state,
                kind: Some(value.kind.clone()),
                address,
                on_action,
            }
        },
        UiSlotEditorHint::Xy => match value.kind.clone() {
            UiSlotValueKind::Vec2(value) => rsx! {
                XySlotField { value, state, address, on_action }
            },
            _ => fallback_value(value, state),
        },
        UiSlotEditorHint::Dimensions => match &value.kind {
            kind if dimensions_parts(kind).is_some() => {
                let kind = kind.clone();
                rsx! {
                    DimensionsSlotField { kind, state, address, on_action }
                }
            }
            _ => auto_value(
                value,
                state,
                unit,
                address,
                on_action,
                NumberBounds::default(),
            ),
        },
        UiSlotEditorHint::Affine2d => match value.kind.clone() {
            kind @ UiSlotValueKind::Mat3x3(_) => rsx! {
                Affine2dSlotField { kind, state, address, on_action }
            },
            _ => auto_value(
                value,
                state,
                unit,
                address,
                on_action,
                NumberBounds::default(),
            ),
        },
        UiSlotEditorHint::Slider { min, max, step } => match value.kind.clone() {
            UiSlotValueKind::F32(value) => rsx! {
                SliderSlotField {
                    value,
                    min,
                    max,
                    step,
                    state,
                    unit,
                    address,
                    on_action,
                }
            },
            _ => auto_value(
                value,
                state,
                unit,
                address,
                on_action,
                NumberBounds::default(),
            ),
        },
        UiSlotEditorHint::Number { min, max, step } => auto_value(
            value,
            state,
            unit,
            address,
            on_action,
            NumberBounds { min, max, step },
        ),
        UiSlotEditorHint::Text | UiSlotEditorHint::Auto => auto_value(
            value,
            state,
            unit,
            address,
            on_action,
            NumberBounds::default(),
        ),
    }
}

/// Optional numeric constraints carried from a `Number` editor hint into
/// scalar number inputs as input attributes.
#[derive(Clone, Copy, Default, PartialEq)]
struct NumberBounds {
    min: Option<f32>,
    max: Option<f32>,
    step: Option<f32>,
}

fn auto_value(
    value: UiSlotValue,
    state: UiSlotFieldState,
    unit: Option<UiSlotUnit>,
    address: Option<ProjectSlotAddress>,
    on_action: Option<EventHandler<UiAction>>,
    bounds: NumberBounds,
) -> Element {
    match value.kind.clone() {
        UiSlotValueKind::String(value) => rsx! {
            StringSlotField { value, state, address, on_action }
        },
        UiSlotValueKind::I32(value) => rsx! {
            IntSlotField {
                value,
                state,
                unit,
                min: bounds.min,
                max: bounds.max,
                step: bounds.step,
                address,
                on_action,
            }
        },
        UiSlotValueKind::U32(value) => rsx! {
            UIntSlotField {
                value,
                state,
                unit,
                min: bounds.min,
                max: bounds.max,
                step: bounds.step,
                address,
                on_action,
            }
        },
        UiSlotValueKind::F32(value) => rsx! {
            FloatSlotField {
                value,
                state,
                unit,
                min: bounds.min,
                max: bounds.max,
                step: bounds.step,
                address,
                on_action,
            }
        },
        UiSlotValueKind::Bool(value) => rsx! {
            BoolSlotField { value, state, address, on_action }
        },
        kind @ (UiSlotValueKind::Vec2(_)
        | UiSlotValueKind::Vec3(_)
        | UiSlotValueKind::Vec4(_)
        | UiSlotValueKind::IVec2(_)
        | UiSlotValueKind::IVec3(_)
        | UiSlotValueKind::IVec4(_)
        | UiSlotValueKind::UVec2(_)
        | UiSlotValueKind::UVec3(_)
        | UiSlotValueKind::UVec4(_)
        | UiSlotValueKind::BVec2(_)
        | UiSlotValueKind::BVec3(_)
        | UiSlotValueKind::BVec4(_)) => rsx! {
            VectorSlotField { kind, state, address, on_action }
        },
        kind @ (UiSlotValueKind::Mat2x2(_)
        | UiSlotValueKind::Mat3x3(_)
        | UiSlotValueKind::Mat4x4(_)) => rsx! {
            MatrixSlotField { kind, state, address, on_action }
        },
        // Composite bodies get gesture editors in M3 P4, not value editors.
        UiSlotValueKind::Unset
        | UiSlotValueKind::Array(_)
        | UiSlotValueKind::Struct { .. }
        | UiSlotValueKind::Enum { .. } => fallback_value(value, state),
        // Explicitly read-only reference displays (not a fallback): Studio
        // never authors resource/product references through value editors.
        UiSlotValueKind::Resource(_) | UiSlotValueKind::Product(_) => rsx! {
            StringSlotField { value: value.display, state }
        },
    }
}

fn fallback_value(value: UiSlotValue, state: UiSlotFieldState) -> Element {
    rsx! {
        StringSlotField {
            value: value.display,
            state,
        }
    }
}

fn slot_value_key(value: &UiSlotValue) -> String {
    match &value.kind {
        UiSlotValueKind::String(value) => value.clone(),
        UiSlotValueKind::I32(value) => value.to_string(),
        UiSlotValueKind::U32(value) => value.to_string(),
        UiSlotValueKind::F32(value) => value.to_string(),
        UiSlotValueKind::Bool(value) => value.to_string(),
        UiSlotValueKind::Unset
        | UiSlotValueKind::Vec2(_)
        | UiSlotValueKind::Vec3(_)
        | UiSlotValueKind::Vec4(_)
        | UiSlotValueKind::IVec2(_)
        | UiSlotValueKind::IVec3(_)
        | UiSlotValueKind::IVec4(_)
        | UiSlotValueKind::UVec2(_)
        | UiSlotValueKind::UVec3(_)
        | UiSlotValueKind::UVec4(_)
        | UiSlotValueKind::BVec2(_)
        | UiSlotValueKind::BVec3(_)
        | UiSlotValueKind::BVec4(_)
        | UiSlotValueKind::Mat2x2(_)
        | UiSlotValueKind::Mat3x3(_)
        | UiSlotValueKind::Mat4x4(_)
        | UiSlotValueKind::Array(_)
        | UiSlotValueKind::Struct { .. }
        | UiSlotValueKind::Enum { .. }
        | UiSlotValueKind::Resource(_)
        | UiSlotValueKind::Product(_) => value.display.clone(),
    }
}
