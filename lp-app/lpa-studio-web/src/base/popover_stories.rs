//! Base popover stories.
//!
//! This file is intentionally small because it is the canonical example of the
//! path-inferred story contract: `base/popover_stories.rs#edge_placement`
//! becomes `base/popover/edge-placement`.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use crate::base::{IconPopoverButton, PopoverPlacement, StudioIconName};

#[story]
fn edge_placement() -> Element {
    rsx! {
        section { class: "ux-popover-story",
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

#[story(description = "An open popover positioned near its trigger.")]
fn open_popover() -> Element {
    rsx! {
        section { class: "tw:min-h-80 tw:pt-16",
            div { class: "tw:flex tw:justify-end tw:pr-24",
                PopoverStoryButton {
                    label: "Open",
                    placement: PopoverPlacement::BottomEnd,
                    initially_open: true,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PopoverStoryButton(
    label: &'static str,
    placement: PopoverPlacement,
    #[props(default = false)] initially_open: bool,
) -> Element {
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
            initially_open,
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
