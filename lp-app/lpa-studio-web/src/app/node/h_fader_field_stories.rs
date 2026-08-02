//! Stories for the horizontal fader (the fixture face's dominant
//! brightness control; build A — ticks above, block grip — settled at the
//! P2c re-gate).
//!
//! Coverage: default, the violet bound state, unsaved (warning label),
//! live-transient (blue label), and the detail popover pinned open — the
//! LABEL is the trigger, and the control's outline merges with the aspect
//! card (P2c item 3).

use dioxus::prelude::*;
use lpa_studio_core::{UiNodeDirtyState, UiSlotFieldState, UiSlotSourceState};
use lpa_studio_web_story_macros::story;

use crate::app::node::PanelControl;
use crate::app::node::face_story_fixtures::{bound_source, fader_control};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FaderStoryCard(#[props(default = false)] tall: bool, children: Element) -> Element {
    let class = if tall {
        "tw:w-96 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6 tw:pb-[340px]"
    } else {
        "tw:w-96 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6"
    };
    rsx! {
        div { class: "{class}", {children} }
    }
}

#[story(description = "Default fader: accent fill sized to the value, chunky grip.")]
fn default() -> Element {
    rsx! {
        FaderStoryCard {
            PanelControl {
                control: fader_control(
                    184.0,
                    UiSlotFieldState::editable(),
                    UiSlotSourceState::Unset,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Bound fader: violet fill, rail border, grip ring, and name — the binding owns the value."
)]
fn bound() -> Element {
    rsx! {
        FaderStoryCard {
            PanelControl {
                control: fader_control(184.0, UiSlotFieldState::editable(), bound_source()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(description = "Unsaved edit: the warning-colored label.")]
fn dirty() -> Element {
    rsx! {
        FaderStoryCard {
            PanelControl {
                control: fader_control(
                    212.0,
                    UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty),
                    UiSlotSourceState::Unset,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Live transient edit: the blue label — brightness rides the running project, not Save."
)]
fn live() -> Element {
    rsx! {
        FaderStoryCard {
            PanelControl {
                control: fader_control(
                    212.0,
                    UiSlotFieldState::editable()
                        .with_dirty(UiNodeDirtyState::Dirty)
                        .with_debug(true),
                    UiSlotSourceState::Unset,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Detail pinned open via the LABEL trigger on a bound fader: the control's outline merges with the identical slot-row aspect card below into one contiguous shape (P2c item 3)."
)]
fn detail_open() -> Element {
    rsx! {
        FaderStoryCard { tall: true,
            PanelControl {
                control: fader_control(184.0, UiSlotFieldState::editable(), bound_source()),
                detail_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}
