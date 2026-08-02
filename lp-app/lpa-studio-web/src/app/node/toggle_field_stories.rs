//! Stories for the panel pill toggle.
//!
//! Good-green is worn ONLY by the on state (green = good/valid — never
//! selection, never binding); bound toggles ring violet in both states.
//! Coverage: on (default), off, bound, unsaved (warning label), live (blue
//! label), and the detail popover pinned open via the label trigger.

use dioxus::prelude::*;
use lpa_studio_core::{UiNodeDirtyState, UiSlotFieldState, UiSlotSourceState};
use lpa_studio_web_story_macros::story;

use crate::app::node::PanelControl;
use crate::app::node::face_story_fixtures::{bound_source, toggle_control};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ToggleStoryCard(children: Element) -> Element {
    rsx! {
        div { class: "tw:inline-flex tw:items-start tw:gap-6 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6",
            {children}
        }
    }
}

#[story(
    description = "Default (on) toggle: good-green pill — the one place green appears on the panel."
)]
fn default() -> Element {
    rsx! {
        ToggleStoryCard {
            PanelControl {
                control: toggle_control(
                    "mirror",
                    true,
                    UiSlotFieldState::editable(),
                    UiSlotSourceState::Unset,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(description = "Off toggle: neutral pill, thumb at rest.")]
fn off() -> Element {
    rsx! {
        ToggleStoryCard {
            PanelControl {
                control: toggle_control(
                    "mirror",
                    false,
                    UiSlotFieldState::editable(),
                    UiSlotSourceState::Unset,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Bound toggle: violet ring on the pill (both states) and the violet name — a bus trigger owns it."
)]
fn bound() -> Element {
    rsx! {
        ToggleStoryCard {
            PanelControl {
                control: toggle_control(
                    "trigger",
                    false,
                    UiSlotFieldState::editable(),
                    bound_source(),
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(description = "Unsaved edit: the warning-colored label.")]
fn dirty() -> Element {
    rsx! {
        ToggleStoryCard {
            PanelControl {
                control: toggle_control(
                    "mirror",
                    true,
                    UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty),
                    UiSlotSourceState::Unset,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(description = "Live transient edit: the blue label.")]
fn live() -> Element {
    rsx! {
        ToggleStoryCard {
            PanelControl {
                control: toggle_control(
                    "mirror",
                    true,
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
    description = "Detail pinned open via the LABEL trigger on a bound toggle: the identical slot-row detail popover."
)]
fn detail_open() -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-[380px] tw:items-start tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6 tw:pl-24",
            PanelControl {
                control: toggle_control(
                    "trigger",
                    false,
                    UiSlotFieldState::editable(),
                    bound_source(),
                ),
                detail_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}
