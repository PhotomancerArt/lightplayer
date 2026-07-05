//! Stories for config slot row states.

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectNodeAddress, ProjectSlotAddress, ProjectSlotRoot, SlotPath, UiBindingEndpoint,
    UiConfigSlot, UiNodeDirtyState, UiSlotEditorHint, UiSlotFieldState, UiSlotOptionality,
    UiSlotSourceState, UiSlotUnit, UiSlotValue,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::ConfigSlotRow;
use crate::app::node::node_story_fixtures::config_row_states_fixture;

fn story_slot_address(path: &str) -> ProjectSlotAddress {
    ProjectSlotAddress::new(
        ProjectNodeAddress::parse("/demo.project/clock.clock").expect("valid story node address"),
        ProjectSlotRoot::def(),
        SlotPath::parse(path).expect("valid story slot path"),
    )
}

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
            slot: UiConfigSlot::value(
                "time",
                "Time",
                UiSlotValue::f32(3.333).with_unit(UiSlotUnit::seconds()),
            )
            .with_source(UiSlotSourceState::Bound(UiBindingEndpoint::new(
                "bus#time.seconds",
            ))),
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
                slot: UiConfigSlot::value(
                    "fade_after",
                    "Fade after",
                    UiSlotValue::f32(0.35).with_unit(UiSlotUnit::seconds()),
                )
                .with_source(UiSlotSourceState::Bound(UiBindingEndpoint::new(
                    "bus#time.seconds",
                ))),
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

#[story(
    label = "Live Chrome",
    description = "Touched transient controls: the live (blue) row tint and detail icon only — no badge; Reset lives in the detail popup."
)]
pub(crate) fn live_chrome() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            ConfigSlotRow {
                slot: UiConfigSlot::value("controls.running", "Running", UiSlotValue::bool(true))
                    .with_address(story_slot_address("controls.running"))
                    .with_state(
                        UiSlotFieldState::editable()
                            .with_dirty(UiNodeDirtyState::Dirty)
                            .with_live(true),
                    ),
                depth: 0,
                index: 0,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "controls.rate",
                    "Rate",
                    UiSlotValue::f32(2.0).with_editor(UiSlotEditorHint::Slider {
                        min: 0.0,
                        max: 4.0,
                        step: Some(0.05),
                    }),
                )
                    .with_address(story_slot_address("controls.rate"))
                    .with_state(
                        UiSlotFieldState::editable()
                            .with_dirty(UiNodeDirtyState::Dirty)
                            .with_live(true),
                    ),
                depth: 0,
                index: 1,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    label = "Live Detail Popup",
    description = "The detail popup for a touched live control: edit state sections plus the Reset affordance."
)]
pub(crate) fn live_detail_popup() -> Element {
    rsx! {
        div { class: "tw:min-h-72",
            ConfigSlotRow {
                slot: UiConfigSlot::value("controls.running", "Running", UiSlotValue::bool(false))
                    .with_address(story_slot_address("controls.running"))
                    .with_state(
                        UiSlotFieldState::editable()
                            .with_dirty(UiNodeDirtyState::Dirty)
                            .with_live(true),
                    ),
                depth: 0,
                index: 0,
                initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    label = "Unsaved Detail Popup",
    description = "The detail popup for an unsaved persisted edit: edited section plus the Revert affordance."
)]
pub(crate) fn unsaved_detail_popup() -> Element {
    rsx! {
        div { class: "tw:min-h-72",
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "color_order",
                    "Color order",
                    UiSlotValue::string("grb").with_editor(UiSlotEditorHint::dropdown([
                        ("rgb", "RGB"),
                        ("grb", "GRB"),
                        ("bgr", "BGR"),
                    ])),
                )
                    .with_address(story_slot_address("color_order"))
                    .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
                depth: 0,
                index: 0,
                initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    label = "Unsaved Chrome",
    description = "A touched persisted slot: amber unsaved badge and tint; Revert lives in the detail popup."
)]
pub(crate) fn unsaved_chrome() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::value(
                "color_order",
                "Color order",
                UiSlotValue::string("grb").with_editor(UiSlotEditorHint::dropdown([
                    ("rgb", "RGB"),
                    ("grb", "GRB"),
                    ("bgr", "BGR"),
                ])),
            )
                .with_address(story_slot_address("color_order"))
                .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
            depth: 0,
            index: 0,
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Editable Clean Controls",
    description = "Untouched editable toggle, slider, and dropdown fields (no edit chrome)."
)]
pub(crate) fn editable_clean_controls() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            ConfigSlotRow {
                slot: UiConfigSlot::value("controls.running", "Running", UiSlotValue::bool(true))
                    .with_address(story_slot_address("controls.running"))
                    .with_state(UiSlotFieldState::editable().with_live(true)),
                depth: 0,
                index: 0,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "controls.rate",
                    "Rate",
                    UiSlotValue::f32(1.0).with_editor(UiSlotEditorHint::Slider {
                        min: 0.0,
                        max: 4.0,
                        step: Some(0.05),
                    }),
                )
                    .with_address(story_slot_address("controls.rate"))
                    .with_state(UiSlotFieldState::editable().with_live(true)),
                depth: 0,
                index: 1,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "color_order",
                    "Color order",
                    UiSlotValue::string("grb").with_editor(UiSlotEditorHint::dropdown([
                        ("rgb", "RGB"),
                        ("grb", "GRB"),
                        ("bgr", "BGR"),
                    ])),
                )
                    .with_address(story_slot_address("color_order"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 2,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    label = "Rejected Edit",
    description = "A rejected edit: error chrome preserves the value with the rejection reason."
)]
pub(crate) fn rejected_edit() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::value(
                "controls.rate",
                "Rate",
                UiSlotValue::f32(9.0).with_editor(UiSlotEditorHint::Slider {
                    min: 0.0,
                    max: 4.0,
                    step: Some(0.05),
                }),
            )
                .with_address(story_slot_address("controls.rate"))
                .with_state(
                    UiSlotFieldState::editable()
                        .with_dirty(UiNodeDirtyState::Error)
                        .with_invalid("target slot is not writable")
                        .with_live(true),
                ),
            depth: 0,
            index: 0,
            on_action: move |_| {},
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
