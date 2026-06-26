//! Typed value dispatcher for config slot field components.

use dioxus::prelude::*;
use lpa_studio_core::{UiSlotEditorHint, UiSlotFieldState, UiSlotValue, UiSlotValueKind};

use crate::app::node::{
    BoolSlotField, DropdownSlotField, FloatSlotField, IntSlotField, StringSlotField, UIntSlotField,
    Vec2SlotField, Vec3SlotField, XySlotField,
};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SlotValueEditor(value: UiSlotValue, state: UiSlotFieldState) -> Element {
    let unit = value.display_unit();

    match value.editor.clone() {
        UiSlotEditorHint::Dropdown(options) => rsx! {
            DropdownSlotField {
                value: slot_value_key(&value),
                options,
                state,
            }
        },
        UiSlotEditorHint::Xy => match value.kind.clone() {
            UiSlotValueKind::Vec2(value) => rsx! {
                XySlotField { value, state }
            },
            _ => fallback_value(value, state),
        },
        UiSlotEditorHint::Text
        | UiSlotEditorHint::Number { .. }
        | UiSlotEditorHint::Slider { .. }
        | UiSlotEditorHint::Auto => match value.kind.clone() {
            UiSlotValueKind::String(value) => rsx! {
                StringSlotField { value, state }
            },
            UiSlotValueKind::I32(value) => rsx! {
                IntSlotField { value, state, unit }
            },
            UiSlotValueKind::U32(value) => rsx! {
                UIntSlotField { value, state, unit }
            },
            UiSlotValueKind::F32(value) => rsx! {
                FloatSlotField { value, state, unit }
            },
            UiSlotValueKind::Bool(value) => rsx! {
                BoolSlotField { value, state }
            },
            UiSlotValueKind::Vec2(value) => rsx! {
                Vec2SlotField { value, state }
            },
            UiSlotValueKind::Vec3(value) => rsx! {
                Vec3SlotField { value, state }
            },
            UiSlotValueKind::Unset
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
            | UiSlotValueKind::Product(_) => fallback_value(value, state),
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
