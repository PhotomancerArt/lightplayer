//! Base popover stories.
//!
//! This file is intentionally small because it is the canonical example of the
//! path-inferred story contract: `base/popover_story.rs#edge_placement` becomes
//! `base/popover/edge-placement`.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::ui_base::{IconPopoverButton, PopoverPlacement, StudioIconName};

#[story(
    label = "Popover placement",
    description = "Icon popovers anchored near viewport and container edges."
)]
fn edge_placement() -> Element {
    rsx! {
        section { class: "ux-popover-story",
            header { class: "ux-popover-story-heading",
                h2 { "Popover placement" }
                p { "Open each trigger to check viewport clamping, edge anchoring, and click-away behavior." }
            }
            div { class: "ux-popover-story-grid",
                div { class: "ux-popover-story-cell ux-popover-story-cell-start",
                    PopoverStoryButton {
                        label: "Start edge",
                        placement: PopoverPlacement::BottomStart,
                    }
                }
                div { class: "ux-popover-story-cell ux-popover-story-cell-center",
                    PopoverStoryButton {
                        label: "Center",
                        placement: PopoverPlacement::BottomEnd,
                    }
                }
                div { class: "ux-popover-story-cell ux-popover-story-cell-end",
                    PopoverStoryButton {
                        label: "End edge",
                        placement: PopoverPlacement::BottomEnd,
                    }
                }
                div { class: "ux-popover-story-cell ux-popover-story-cell-lower-end",
                    PopoverStoryButton {
                        label: "Lower edge",
                        placement: PopoverPlacement::BottomEnd,
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PopoverStoryButton(label: &'static str, placement: PopoverPlacement) -> Element {
    rsx! {
        IconPopoverButton {
            class: "ux-node-ui-popup-trigger".to_string(),
            open_class: "ux-node-ui-popup-trigger ux-node-ui-popup-trigger-open".to_string(),
            icon: StudioIconName::BoundValue,
            icon_size: 13,
            label: format!("{label} details"),
            title: format!("{label} details"),
            popup_class: "ux-node-ui-popup ux-popover-story-panel".to_string(),
            placement,
            div { class: "ux-node-ui-popup-kicker", "popover" }
            strong { "{label}" }
            p { "This panel is positioned with fixed coordinates and clamped to the viewport." }
            div { class: "ux-node-ui-binding-section ux-node-ui-bus-binding-section",
                div { class: "ux-node-ui-binding-heading", "example binding" }
                div { class: "ux-node-ui-bus-binding-row",
                    span { "bus#" }
                    code { "visual.out" }
                    button { r#type: "button", "del" }
                }
            }
        }
    }
}
