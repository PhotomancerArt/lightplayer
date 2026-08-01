//! Stories for the panel knob (knob v2: SVG value arc + ticks).
//!
//! Coverage: default, an arc sweep across the range, the violet bound
//! state, the label state-color ladder (the label button is the detail
//! trigger AND the edit-state signal — the old dot is gone), and the
//! detail popover pinned open via the label — the SAME `SlotDetailButton`
//! popover as the backing slot row, with the violet binding section.

use dioxus::prelude::*;
use lpa_studio_core::{UiNodeDirtyState, UiSlotFieldState, UiSlotSourceState};
use lpa_studio_web_story_macros::story;

use crate::app::node::PanelControl;
use crate::app::node::face_story_fixtures::{bound_source, knob_control, knob_control_stepped};

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
    description = "Bound knob with a live bus reading: the violet arc and pointer follow the LIVE value while the readout leads with it (violet) and keeps the authored default secondary — the default stays the edit target."
)]
fn bound_live() -> Element {
    let mut control = knob_control(
        "speed",
        1.6,
        0.0,
        4.0,
        UiSlotFieldState::editable(),
        bound_source(),
    );
    control.live_value = Some("3.4".to_string());
    rsx! {
        KnobStoryCard {
            PanelControl { control, on_action: move |_| {} }
        }
    }
}

#[story(description = "Unsaved edit: the warning-colored label; Save will persist the new value.")]
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
    description = "Live transient edit: the blue label — applied to the running project, never written by Save."
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
    description = "The label state-color ladder on one row: quiet (clean), violet (bound), warning (unsaved), blue (live transient), red (write failed), and warning-over-violet-widget (edited while bound) — the label is both the detail trigger and the edit-state signal; the old readout dot is gone."
)]
fn label_states() -> Element {
    let clean = UiSlotFieldState::editable();
    let dirty = UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty);
    let live = UiSlotFieldState::editable()
        .with_dirty(UiNodeDirtyState::Dirty)
        .with_live(true);
    let failed = UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Error);
    rsx! {
        KnobStoryCard {
            PanelControl {
                control: knob_control("clean", 1.0, 0.0, 4.0, clean.clone(), UiSlotSourceState::Unset),
                on_action: move |_| {},
            }
            PanelControl {
                control: knob_control("bound", 1.6, 0.0, 4.0, clean, bound_source()),
                on_action: move |_| {},
            }
            PanelControl {
                control: knob_control("unsaved", 2.0, 0.0, 4.0, dirty.clone(), UiSlotSourceState::Unset),
                on_action: move |_| {},
            }
            PanelControl {
                control: knob_control("live", 2.4, 0.0, 4.0, live, UiSlotSourceState::Unset),
                on_action: move |_| {},
            }
            PanelControl {
                control: knob_control("failed", 3.0, 0.0, 4.0, failed, UiSlotSourceState::Unset),
                on_action: move |_| {},
            }
            PanelControl {
                control: knob_control("both", 2.8, 0.0, 4.0, dirty, bound_source()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Whole-number knob: a `count` uniform over 1..=4 (an i32/u32 shape, or an authored step of 1). The sweep is CUT INTO BLOCKS — one per increment, squared off, lighting whole — so a discrete control never reads as a continuous one. Drag it and blocks light one at a time instead of an arc growing smoothly."
)]
fn stepped() -> Element {
    rsx! {
        KnobStoryCard {
            for (index, value) in [1.0_f32, 2.0, 3.0, 4.0].into_iter().enumerate() {
                PanelControl {
                    key: "{index}",
                    control: knob_control_stepped(
                        "count",
                        value,
                        1.0,
                        4.0,
                        Some(1.0),
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
    description = "The two visual languages side by side, same stored value: the whole-number knob is blocked (one lit of three) and reads 2; the continuous one is a smooth rounded arc with its fixed tick marks and reads 2.37. Off-grid stored values never look authoritative on a stepped control."
)]
fn stepped_vs_continuous() -> Element {
    rsx! {
        KnobStoryCard {
            PanelControl {
                control: knob_control_stepped(
                    "count",
                    2.37,
                    1.0,
                    4.0,
                    Some(1.0),
                    UiSlotFieldState::editable(),
                    UiSlotSourceState::Unset,
                ),
                on_action: move |_| {},
            }
            PanelControl {
                control: knob_control(
                    "speed",
                    2.37,
                    1.0,
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
    description = "Detail pinned open via the LABEL trigger on a bound knob: the identical slot-row detail popover, its outline merged with the whole control, including the violet binding section."
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
