//! Stories for config slot row states.

use dioxus::prelude::*;
use lpa_studio_core::{
    UiBindingEndpoint, UiConfigSlot, UiNodeDirtyState, UiSlotFieldState, UiSlotOptionality,
    UiSlotSourceState, UiSlotValue,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::ConfigSlotRow;
use crate::app::node::node_story_fixtures::config_row_states_fixture;

#[story(
    label = "All States",
    description = "Representative config rows for direct, bound, edited, invalid, and record slots."
)]
pub(crate) fn all_states() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            for (index, slot) in config_row_states_fixture().into_iter().enumerate() {
                ConfigSlotRow { slot, depth: 0, index }
            }
        }
    }
}

#[story(description = "A directly authored value row.")]
pub(crate) fn direct_value() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::value("brightness", "Brightness", UiSlotValue::f32(0.72)),
            depth: 0,
            index: 0,
        }
    }
}

#[story(description = "A row whose visible value comes from a binding endpoint.")]
pub(crate) fn bound_value() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::value("time", "Time", UiSlotValue::f32(3.333)).with_source(
                UiSlotSourceState::Bound(UiBindingEndpoint::new("bus#time.seconds")),
            ),
            depth: 0,
            index: 0,
        }
    }
}

#[story(description = "An open slot info popup showing the compact aspect rows.")]
pub(crate) fn info_popup() -> Element {
    rsx! {
        div { class: "tw:min-h-56",
            ConfigSlotRow {
                slot: UiConfigSlot::value("fade_after", "Fade after", UiSlotValue::f32(0.35))
                    .with_source(UiSlotSourceState::Bound(UiBindingEndpoint::new("bus#time.seconds"))),
                depth: 0,
                index: 0,
                initially_open: true,
            }
        }
    }
}

#[story(description = "A row with a local edited-state affordance.")]
pub(crate) fn edited_value() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::value("shader", "Shader", UiSlotValue::string("idle.glsl"))
                .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
            depth: 0,
            index: 0,
        }
    }
}

#[story(description = "A row with a validation issue.")]
pub(crate) fn invalid_value() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::value("fade_after", "Fade after", UiSlotValue::f32(-1.0))
                .with_state(UiSlotFieldState::editable().with_invalid("value must be non-negative")),
            depth: 0,
            index: 0,
        }
    }
}

#[story(description = "A row preserving an edited value after a failed write.")]
pub(crate) fn write_failed() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::value("shader", "Shader", UiSlotValue::string("blast.glsl"))
                .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Error)),
            depth: 0,
            index: 0,
        }
    }
}

#[story(description = "A row with no direct value or binding.")]
pub(crate) fn unset_value() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::empty("optional_trigger", "Optional trigger")
                .with_optionality(UiSlotOptionality::excluded(true))
                .with_source(UiSlotSourceState::Unset),
            depth: 0,
            index: 0,
        }
    }
}

#[story(
    description = "An included optional scalar renders as a normal value with an enable toggle."
)]
pub(crate) fn optional_scalar_included() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::value("brightness", "Brightness", UiSlotValue::u32(255))
                .with_optionality(UiSlotOptionality::included(true)),
            depth: 0,
            index: 0,
        }
    }
}

#[story(
    description = "An excluded optional scalar renders as an unset value with the enable toggle off."
)]
pub(crate) fn optional_scalar_excluded() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::empty("brightness", "Brightness")
                .with_optionality(UiSlotOptionality::excluded(true))
                .with_source(UiSlotSourceState::Unset),
            depth: 0,
            index: 0,
        }
    }
}

#[story(description = "An included optional record expands into its real child fields.")]
pub(crate) fn optional_record_included() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::record(
                "output_options",
                "Output options",
                vec![
                    UiConfigSlot::value("dither", "Dither", UiSlotValue::bool(true)),
                    UiConfigSlot::value("interpolate", "Interpolate", UiSlotValue::bool(false)),
                ],
            )
            .with_optionality(UiSlotOptionality::included(true)),
            depth: 0,
            index: 0,
        }
    }
}

#[story(description = "A collapsed record row.")]
pub(crate) fn record_row() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::record(
                "transform",
                "Transform",
                vec![
                    UiConfigSlot::value("origin", "Origin", UiSlotValue::vec2([0.42, 0.58])),
                    UiConfigSlot::value("scale", "Scale", UiSlotValue::vec2([1.0, 1.0])),
                ],
            ),
            depth: 0,
            index: 0,
        }
    }
}
