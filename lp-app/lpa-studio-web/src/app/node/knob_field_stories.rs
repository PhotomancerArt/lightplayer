//! Stories for the panel knob (knob v2: SVG value arc + ticks).
//!
//! Coverage: default, an arc sweep across the range, the violet bound
//! state, unsaved (warning dot), live-transient (blue dot), and the corner
//! ⓘ pinned open — the SAME `SlotDetailButton` popover as the backing slot
//! row, with the violet binding section.

use dioxus::prelude::*;
use lpa_studio_core::{UiNodeDirtyState, UiSlotFieldState, UiSlotSourceState};
use lpa_studio_web_story_macros::story;

use crate::app::node::PanelControl;
use crate::app::node::face_story_fixtures::{bound_source, knob_control};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn KnobStoryCard(children: Element) -> Element {
    rsx! {
        div { class: "tw:inline-flex tw:items-start tw:gap-6 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6",
            {children}
        }
    }
}

#[story(description = "Default knob: accent value arc, ticks, gradient body, value readout.")]
fn default() -> Element {
    rsx! {
        KnobStoryCard {
            PanelControl {
                control: knob_control(
                    "speed",
                    1.6,
                    0.0,
                    4.0,
                    UiSlotFieldState::editable(),
                    UiSlotSourceState::Unset,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Arc coverage across the range: min, quarter, mid, three-quarter, and max pointer/arc positions."
)]
fn value_sweep() -> Element {
    rsx! {
        KnobStoryCard {
            for (index, value) in [0.0_f32, 1.0, 2.0, 3.0, 4.0].into_iter().enumerate() {
                PanelControl {
                    key: "{index}",
                    control: knob_control(
                        "speed",
                        value,
                        0.0,
                        4.0,
                        UiSlotFieldState::editable(),
                        UiSlotSourceState::Unset,
                    ),
                    on_action: move |_| {},
                }
            }
        }
    }
}

#[story(
    description = "Bound knob: violet arc, pointer, body ring, and name — the bus binding owns the value (never green)."
)]
fn bound() -> Element {
    rsx! {
        KnobStoryCard {
            PanelControl {
                control: knob_control(
                    "speed",
                    1.6,
                    0.0,
                    4.0,
                    UiSlotFieldState::editable(),
                    bound_source(),
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Unsaved edit: the warning dot beside the readout; Save will persist the new value."
)]
fn dirty() -> Element {
    rsx! {
        KnobStoryCard {
            PanelControl {
                control: knob_control(
                    "scale",
                    2.0,
                    0.0,
                    4.0,
                    UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty),
                    UiSlotSourceState::Unset,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Live transient edit: the blue dot — applied to the running project, never written by Save."
)]
fn live() -> Element {
    rsx! {
        KnobStoryCard {
            PanelControl {
                control: knob_control(
                    "scale",
                    2.0,
                    0.0,
                    4.0,
                    UiSlotFieldState::editable()
                        .with_dirty(UiNodeDirtyState::Dirty)
                        .with_live(true),
                    UiSlotSourceState::Unset,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Corner ⓘ pinned open on a bound knob: the identical slot-row detail popover, including the violet binding section."
)]
fn detail_open() -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-[380px] tw:items-start tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6 tw:pl-24",
            PanelControl {
                control: knob_control(
                    "speed",
                    1.6,
                    0.0,
                    4.0,
                    UiSlotFieldState::editable(),
                    bound_source(),
                ),
                detail_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}
