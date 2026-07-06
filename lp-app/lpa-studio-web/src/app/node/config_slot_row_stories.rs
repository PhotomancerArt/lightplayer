//! Stories for config slot row states.

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectNodeAddress, ProjectSlotAddress, ProjectSlotRoot, SlotPath, UiBindingEndpoint,
    UiConfigSlot, UiNodeDirtyState, UiSlotComposite, UiSlotEditorHint, UiSlotEnumComposite,
    UiSlotFieldState, UiSlotMapComposite, UiSlotMapKeyKind, UiSlotOptionality, UiSlotSourceState,
    UiSlotUnit, UiSlotValue,
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
    description = "Touched transient controls: the live (blue) row tint, the detail icon, and the inline Reset icon on rows with an own edit entry — no text chips."
)]
pub(crate) fn live_chrome() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            ConfigSlotRow {
                slot: UiConfigSlot::value("controls.running", "Running", UiSlotValue::bool(true))
                    .with_address(story_slot_address("controls.running"))
                    .with_edit_entry_address(story_slot_address("controls.running"))
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
                    .with_edit_entry_address(story_slot_address("controls.rate"))
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
                    .with_edit_entry_address(story_slot_address("controls.running"))
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
                    .with_edit_entry_address(story_slot_address("color_order"))
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
    description = "A touched persisted slot: amber tint plus the inline Revert icon — no text chip; the detail popup keeps its Revert footer."
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
                .with_edit_entry_address(story_slot_address("color_order"))
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
    label = "Editable Scalar Inputs",
    description = "Untouched editable text and number inputs: string, bounded int, uint, and plain float."
)]
pub(crate) fn editable_scalar_inputs() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "label",
                    "Label",
                    UiSlotValue::string("warm wash").with_editor(UiSlotEditorHint::Text),
                )
                    .with_address(story_slot_address("label"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 0,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "ring_count",
                    "Ring count",
                    UiSlotValue::i32(-4).with_editor(UiSlotEditorHint::Number {
                        min: Some(-8.0),
                        max: Some(8.0),
                        step: Some(1.0),
                    }),
                )
                    .with_address(story_slot_address("ring_count"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 1,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value("pixel_count", "Pixel count", UiSlotValue::u32(144))
                    .with_address(story_slot_address("pixel_count"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 2,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "fade_after",
                    "Fade after",
                    UiSlotValue::f32(0.35).with_unit(UiSlotUnit::seconds()),
                )
                    .with_address(story_slot_address("fade_after"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 3,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    label = "Editable Vector Grids",
    description = "Editable component grids composing whole vector values: Vec3, Vec4, IVec2, UVec2, and BVec3."
)]
pub(crate) fn editable_vector_grids() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            ConfigSlotRow {
                slot: UiConfigSlot::value("tint", "Tint", UiSlotValue::vec3([1.0, 0.42, 0.2]))
                    .with_address(story_slot_address("tint"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 0,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "color",
                    "Color",
                    UiSlotValue::vec4([1.0, 0.42, 0.2, 1.0]),
                )
                    .with_address(story_slot_address("color"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 1,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value("offset", "Offset", UiSlotValue::ivec2([-3, 7]))
                    .with_address(story_slot_address("offset"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 2,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value("extent", "Extent", UiSlotValue::uvec2([32, 48]))
                    .with_address(story_slot_address("extent"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 3,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "mirror",
                    "Mirror",
                    UiSlotValue::bvec3([true, false, true]),
                )
                    .with_address(story_slot_address("mirror"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 4,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    label = "Editable Matrix Cells",
    description = "Per-cell matrix grids composing whole matrix values: editable Mat3x3 and read-only Mat2x2."
)]
pub(crate) fn editable_matrix_cells() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "transform",
                    "Transform",
                    UiSlotValue::mat3x3([
                        [1.0, 0.0, 0.5],
                        [0.0, 1.0, 0.25],
                        [0.0, 0.0, 1.0],
                    ]),
                )
                    .with_address(story_slot_address("transform"))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 0,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "basis",
                    "Basis",
                    UiSlotValue::mat2x2([[0.0, -1.0], [1.0, 0.0]]),
                )
                    .with_state(UiSlotFieldState::readonly()),
                depth: 0,
                index: 1,
            }
        }
    }
}

#[story(
    label = "Scalar Input States",
    description = "Dirty and invalid chrome on the new text/number inputs and vector grids."
)]
pub(crate) fn scalar_input_states() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "label",
                    "Label",
                    UiSlotValue::string("cool wash").with_editor(UiSlotEditorHint::Text),
                )
                    .with_address(story_slot_address("label"))
                    .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
                depth: 0,
                index: 0,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value("ring_count", "Ring count", UiSlotValue::i32(-12))
                    .with_address(story_slot_address("ring_count"))
                    .with_state(
                        UiSlotFieldState::editable().with_invalid("value must be at least -8"),
                    ),
                depth: 0,
                index: 1,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value("tint", "Tint", UiSlotValue::vec3([1.0, 0.9, 0.2]))
                    .with_address(story_slot_address("tint"))
                    .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
                depth: 0,
                index: 2,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value(
                    "transform",
                    "Transform",
                    UiSlotValue::mat2x2([[1.0, 0.0], [0.0, 0.0]]),
                )
                    .with_address(story_slot_address("transform"))
                    .with_state(
                        UiSlotFieldState::editable().with_invalid("matrix must be invertible"),
                    ),
                depth: 0,
                index: 3,
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

fn u32_map_slot(entries: &[(u32, u32)], suggested_key: &str) -> UiConfigSlot {
    UiConfigSlot::record(
        "ring_lamp_counts",
        "Ring lamp counts",
        entries
            .iter()
            .map(|(key, count)| {
                UiConfigSlot::value(
                    format!("ring_lamp_counts[{key}]"),
                    key.to_string(),
                    UiSlotValue::u32(*count),
                )
                .with_address(story_slot_address(&format!("ring_lamp_counts[{key}]")))
                .with_state(UiSlotFieldState::editable())
            })
            .collect(),
    )
    .with_address(story_slot_address("ring_lamp_counts"))
    .with_composite(UiSlotComposite::Map(UiSlotMapComposite {
        key_kind: UiSlotMapKeyKind::U32,
        suggested_key: suggested_key.to_string(),
    }))
    .with_state(UiSlotFieldState::editable())
}

fn string_map_slot(entries: &[(&str, f32)]) -> UiConfigSlot {
    UiConfigSlot::record(
        "presets",
        "Presets",
        entries
            .iter()
            .map(|(key, value)| {
                UiConfigSlot::value(
                    format!("presets[\"{key}\"]"),
                    key.to_string(),
                    UiSlotValue::f32(*value),
                )
                .with_address(story_slot_address(&format!("presets[\"{key}\"]")))
                .with_state(UiSlotFieldState::editable())
            })
            .collect(),
    )
    .with_address(story_slot_address("presets"))
    .with_composite(UiSlotComposite::Map(UiSlotMapComposite {
        key_kind: UiSlotMapKeyKind::String,
        suggested_key: String::new(),
    }))
    .with_state(UiSlotFieldState::editable())
}

#[story(
    label = "Map Add Immediate",
    description = "A numeric map's closed add affordances: + adds immediately at the first free index (gap-filling: effective keys 0 and 2 suggest 1), # opens the optional key override."
)]
pub(crate) fn map_add_immediate() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: u32_map_slot(&[(0, 16), (2, 24)], "1"),
            depth: 0,
            index: 0,
            initially_expanded: Some(true),
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Map Add Entry Open",
    description = "An expanded numeric map row with the optional key-override input open, prefilled with the first free index; entry rows carry remove affordances."
)]
pub(crate) fn map_add_entry_open() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: u32_map_slot(&[(0, 16), (1, 24)], "2"),
            depth: 0,
            index: 0,
            initially_expanded: Some(true),
            initially_adding: true,
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Map Add String Key",
    description = "A string-keyed map keeps the key input as the primary add flow (string keys cannot be guessed): + opens the empty input."
)]
pub(crate) fn map_add_string_key() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: string_map_slot(&[("warm", 0.5)]),
            depth: 0,
            index: 0,
            initially_expanded: Some(true),
            initially_adding: true,
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Map Added Entry Dirty",
    description = "A freshly added map entry: the entry row shows dirty and the parent map rides the prefix join with the unsaved badge."
)]
pub(crate) fn map_added_entry_dirty() -> Element {
    let mut slot = u32_map_slot(&[(0, 16), (1, 24)], "2");
    slot.state = UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty);
    if let lpa_studio_core::UiConfigSlotBody::Record(record) = &mut slot.body {
        record.fields[1].state = UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty);
    }
    rsx! {
        ConfigSlotRow {
            slot,
            depth: 0,
            index: 0,
            initially_expanded: Some(true),
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Map Removed Entry Parent Dirty",
    description = "A removed map entry has no surviving row; the parent map row shows the structural edit as dirty via the prefix join."
)]
pub(crate) fn map_removed_entry_parent_dirty() -> Element {
    let mut slot = u32_map_slot(&[(0, 16)], "1");
    slot.state = UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty);
    rsx! {
        ConfigSlotRow {
            slot,
            depth: 0,
            index: 0,
            initially_expanded: Some(true),
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Map Entry Key Edit Open",
    description = "A map entry row with its click-to-edit key input open, prefilled with the current key; committing a changed key dispatches MoveEntry on the parent map."
)]
pub(crate) fn map_entry_key_edit_open() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::value("ring_lamp_counts[1]", "1", UiSlotValue::u32(24))
                .with_address(story_slot_address("ring_lamp_counts[1]"))
                .with_state(UiSlotFieldState::editable()),
            depth: 1,
            index: 0,
            entry_key_kind: Some(UiSlotMapKeyKind::U32),
            initially_key_editing: true,
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Map Move Rejected Occupied",
    description = "An occupied-target key move: the rejection parks Failed at the map address, so the map row wears the error chrome with the server's key-already-in-use reason."
)]
pub(crate) fn map_move_rejected_occupied() -> Element {
    let mut slot = u32_map_slot(&[(0, 16), (1, 24)], "2");
    slot.state = UiSlotFieldState::editable()
        .with_dirty(UiNodeDirtyState::Error)
        .with_invalid("map entry ring_lamp_counts[1] already exists in the effective definition");
    rsx! {
        ConfigSlotRow {
            slot,
            depth: 0,
            index: 0,
            initially_expanded: Some(true),
            on_action: move |_| {},
        }
    }
}

#[story(
    label = "Option Toggle On And Off",
    description = "Wired some/none toggles at the trailing (right-anchored) edge of the value area — a fixed-width track with no state text, so toggling never reflows the row: on dispatches EnsurePresent at the interior some path, off dispatches RemoveValue at the option path; the last row's non-toggleable switch renders muted and inert."
)]
pub(crate) fn option_toggle_on_off() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            ConfigSlotRow {
                slot: UiConfigSlot::value("brightness", "Brightness", UiSlotValue::u32(255))
                    .with_address(story_slot_address("brightness"))
                    .with_optionality(UiSlotOptionality::included(true))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 0,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::empty("gamma_correction", "Gamma correction")
                    .with_address(story_slot_address("gamma_correction"))
                    .with_optionality(UiSlotOptionality::excluded(true))
                    .with_source(UiSlotSourceState::Unset)
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 1,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: UiConfigSlot::value("white_point", "White point", UiSlotValue::u32(128))
                    .with_address(story_slot_address("white_point"))
                    .with_optionality(UiSlotOptionality::included(false))
                    .with_state(UiSlotFieldState::editable()),
                depth: 0,
                index: 2,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    label = "Enum Variant Switched",
    description = "An enum row after a variant switch: the variant dropdown lists the declared idents verbatim, the row shows the pending structural edit, and the active variant's payload fields render directly below — no variant-level row (the payload addresses keep the variant segment)."
)]
pub(crate) fn enum_variant_switched() -> Element {
    rsx! {
        ConfigSlotRow {
            slot: UiConfigSlot::record(
                "mapping",
                "Mapping",
                vec![
                    UiConfigSlot::record("mapping.PathPoints.paths", "Paths", vec![])
                        .with_address(story_slot_address("mapping.PathPoints.paths"))
                        .with_composite(UiSlotComposite::Map(UiSlotMapComposite {
                            key_kind: UiSlotMapKeyKind::U32,
                            suggested_key: "0".to_string(),
                        }))
                        .with_state(UiSlotFieldState::editable()),
                    UiConfigSlot::value(
                        "mapping.PathPoints.sample_diameter",
                        "Sample diameter",
                        UiSlotValue::f32(1.5),
                    )
                    .with_address(story_slot_address("mapping.PathPoints.sample_diameter"))
                    .with_state(UiSlotFieldState::editable()),
                ],
            )
            .with_address(story_slot_address("mapping"))
            .with_composite(UiSlotComposite::Enum(UiSlotEnumComposite {
                active: "PathPoints".to_string(),
                variants: vec![
                    "Unset".to_string(),
                    "PathPoints".to_string(),
                    "SvgPath".to_string(),
                ],
            }))
            .with_state(UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)),
            depth: 0,
            index: 0,
            initially_expanded: Some(true),
            on_action: move |_| {},
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
