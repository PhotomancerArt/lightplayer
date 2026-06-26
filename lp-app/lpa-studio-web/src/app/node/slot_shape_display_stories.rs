//! Stories for slot shape presentation.

use dioxus::prelude::*;
use lpa_studio_core::{UiSlotShape, UiSlotShapeField};
use lpa_studio_web_story_macros::story;

use crate::app::node::{SlotShapeDisplay, SlotShapeDisplayMode};

#[story(description = "Compact and friendly renderings for primitive slot shapes.")]
pub(crate) fn gallery() -> Element {
    let shapes = vec![
        UiSlotShape::Int32,
        UiSlotShape::UInt32,
        UiSlotShape::Float32,
        UiSlotShape::Bool,
        UiSlotShape::Text,
        UiSlotShape::Vec2,
        UiSlotShape::Vec3,
    ];

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:max-w-[520px] tw:gap-2",
            for shape in shapes {
                div { class: "tw:grid tw:min-w-0 tw:grid-cols-[minmax(80px,0.35fr)_minmax(0,1fr)] tw:items-baseline tw:gap-3 tw:border-b tw:border-border-muted tw:pb-1.5",
                    SlotShapeDisplay {
                        shape: shape.clone(),
                        mode: SlotShapeDisplayMode::Compact,
                    }
                    SlotShapeDisplay {
                        shape,
                        mode: SlotShapeDisplayMode::CompactFriendly,
                    }
                }
            }
        }
    }
}

#[story(description = "A record shape with compact recursive field types.")]
pub(crate) fn record_shape() -> Element {
    rsx! {
        SlotShapeDisplay {
            shape: transform_shape(),
            mode: SlotShapeDisplayMode::CompactFriendly,
        }
    }
}

#[story(description = "A verbose record shape with nested fields.")]
pub(crate) fn verbose_record() -> Element {
    rsx! {
        SlotShapeDisplay {
            shape: transform_shape(),
            mode: SlotShapeDisplayMode::Verbose,
        }
    }
}

fn transform_shape() -> UiSlotShape {
    UiSlotShape::Record(vec![
        UiSlotShapeField::new("Origin", UiSlotShape::Vec2),
        UiSlotShapeField::new("Scale", UiSlotShape::Vec2),
        UiSlotShapeField::new(
            "Envelope",
            UiSlotShape::Record(vec![
                UiSlotShapeField::new("Fade after", UiSlotShape::Float32),
                UiSlotShapeField::new("Trigger", UiSlotShape::Bool),
            ]),
        ),
    ])
}
