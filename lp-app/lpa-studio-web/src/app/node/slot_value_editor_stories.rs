//! Stories for slot value editor field variants.

use dioxus::prelude::*;
use lpa_studio_core::{UiSlotEditorHint, UiSlotFieldState, UiSlotValue};
use lpa_studio_web_story_macros::story;

use crate::app::node::node_story_fixtures::slot_value_variants_fixture;
use crate::app::node::SlotValueEditor;

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
            value: UiSlotValue::f32(0.35).with_detail("s"),
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

#[story(description = "A minimal XY slot field for Vec2 values.")]
pub(crate) fn xy_field() -> Element {
    rsx! {
        SlotValueEditor {
            value: UiSlotValue::vec2([0.42, 0.58]).with_editor(UiSlotEditorHint::Xy),
            state: UiSlotFieldState::editable(),
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
