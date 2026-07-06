//! Stories for slot value editor field variants.

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectNodeAddress, ProjectSlotAddress, ProjectSlotRoot, SlotPath, UiSlotEditorHint,
    UiSlotFieldState, UiSlotUnit, UiSlotValue,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::slot_value_variants_fixture;
use crate::app::node::{SliderSlotField, SlotValueEditor, XySlotField};

fn story_slot_address(path: &str) -> ProjectSlotAddress {
    ProjectSlotAddress::new(
        ProjectNodeAddress::parse("/demo.project/pixels.fixture").expect("valid story address"),
        ProjectSlotRoot::def(),
        SlotPath::parse(path).expect("valid story slot path"),
    )
}

/// A `Dim2u`-shaped struct value wearing the `Dimensions` editor hint.
fn dim2u_value(width: u32, height: u32) -> UiSlotValue {
    UiSlotValue::struct_value(
        Some("Dim2u".to_string()),
        vec![
            ("width".to_string(), UiSlotValue::u32(width)),
            ("height".to_string(), UiSlotValue::u32(height)),
        ],
    )
    .with_editor(UiSlotEditorHint::Dimensions)
}

#[story(description = "Slot value editor dispatch across the M1 value types.")]
pub(crate) fn gallery() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:max-w-[420px] tw:gap-2",
            for value in slot_value_variants_fixture() {
                SlotValueEditor {
                    value,
                    state: UiSlotFieldState::editable(),
                }
            }
        }
    }
}

#[story(description = "String slot field.")]
pub(crate) fn string_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::string("./idle.glsl").with_editor(UiSlotEditorHint::Text),
            state: UiSlotFieldState::editable(),
        }
    }
}

#[story(description = "Signed integer slot field.")]
pub(crate) fn int_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::i32(-4),
            state: UiSlotFieldState::editable(),
        }
    }
}

#[story(description = "Unsigned integer slot field.")]
pub(crate) fn uint_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::u32(128),
            state: UiSlotFieldState::editable(),
        }
    }
}

#[story(description = "Floating point slot field.")]
pub(crate) fn float_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::f32(0.35).with_unit(UiSlotUnit::seconds()),
            state: UiSlotFieldState::editable(),
        }
    }
}

#[story(description = "Floating point slot field with a slider editor hint.")]
pub(crate) fn slider_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::f32(0.72).with_editor(UiSlotEditorHint::slider(0.0, 1.0)),
            state: UiSlotFieldState::editable(),
        }
    }
}

#[story(description = "Boolean slot field.")]
pub(crate) fn bool_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::bool(true),
            state: UiSlotFieldState::editable(),
        }
    }
}

#[story(description = "Two-component vector slot field.")]
pub(crate) fn vec2_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::vec2([0.42, 0.58]),
            state: UiSlotFieldState::editable(),
        }
    }
}

#[story(description = "Three-component vector slot field.")]
pub(crate) fn vec3_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::vec3([1.0, 0.42, 0.2]),
            state: UiSlotFieldState::editable(),
        }
    }
}

#[story(description = "Dropdown slot field for enum-like values.")]
pub(crate) fn dropdown_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::string("blast").with_editor(UiSlotEditorHint::dropdown([
                ("idle", "Idle"),
                ("blast", "Blast"),
                ("strobe", "Strobe"),
            ])),
            state: UiSlotFieldState::editable(),
        }
    }
}

#[story(
    description = "The editable XY pad for Vec2 values: drag-to-edit pad, read-only component readouts, and the raw-input popup affordance."
)]
pub(crate) fn xy_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::vec2([0.42, 0.58]).with_editor(UiSlotEditorHint::Xy),
            state: UiSlotFieldState::editable(),
            address: story_slot_address("origin"),
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Dimensions Field",
    description = "The compact width × height editor for Dimensions-hinted Dim2u struct values, composing the whole struct on change."
)]
pub(crate) fn dimensions_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: dim2u_value(32, 18),
            state: UiSlotFieldState::editable(),
            address: story_slot_address("render_size"),
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Affine2d Field",
    description = "The labeled six-parameter grid (a b tx / c d ty) for Affine2d-hinted Mat3x3 values, writing the whole matrix with the inactive row fixed."
)]
pub(crate) fn affine2d_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::mat3x3([[1.0, 0.25, 12.0], [-0.5, 2.0, -8.0], [0.0, 0.0, 1.0]])
                .with_editor(UiSlotEditorHint::Affine2d),
            state: UiSlotFieldState::editable(),
            address: story_slot_address("transform"),
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Slider Raw Input Popup",
    description = "The slider's raw-input detail popup open: exact numeric entry (onchange) against the same slot path as the slider's oninput drags — two views onto one path-keyed buffer entry."
)]
pub(crate) fn slider_raw_input_popup() -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-44 tw:max-w-[420px] tw:justify-end",
            SliderSlotField {
                value: 0.72,
                min: 0.0,
                max: 1.0,
                state: UiSlotFieldState::editable(),
                address: Some(story_slot_address("brightness")),
                on_action: move |_| {},
                raw_initially_open: true,
            }
        }
    }
}

#[story(
    label = "Xy Raw Input Popup",
    description = "The XY pad's raw-input detail popup open: exact x/y entry (onchange) against the same slot path as the pad's oninput drags."
)]
pub(crate) fn xy_raw_input_popup() -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-52 tw:max-w-[420px] tw:justify-end",
            XySlotField {
                value: [0.42, 0.58],
                state: UiSlotFieldState::editable(),
                address: Some(story_slot_address("origin")),
                on_action: move |_| {},
                raw_initially_open: true,
            }
        }
    }
}

#[story(description = "Invalid value field state.")]
pub(crate) fn invalid_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::f32(-1.0),
            state: UiSlotFieldState::editable().with_invalid("value must be non-negative"),
        }
    }
}
