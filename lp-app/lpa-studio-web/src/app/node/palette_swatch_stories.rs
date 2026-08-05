//! Stories for the palette swatch — a panel control's closed face (M4 P3).
//!
//! The design question these answer is the mode-adaptive face: a HELD
//! palette is one strip, a CYCLE is its member set as one segmented band
//! plus the auto-denominated step rate. Everything else is the panel chrome
//! every other control already wears — the label is the detail trigger, the
//! readout is one compact chip, violet means bound.
//!
//! The chevron promises a chooser that does not exist yet: P4 turns the
//! band into its trigger, and the band is deliberately inert until then.

use dioxus::prelude::*;
use lpa_studio_core::{UiNodeDirtyState, UiSlotFieldState};
use lpa_studio_web_story_macros::story;

use crate::app::node::PanelControl;
use crate::app::node::face_story_fixtures::palette_swatch_control;
use crate::app::node::node_story_fixtures::{palette_cycle, sunset_gradient};
use lpc_model::GradientConfig;

/// Swatches are wide controls, so the story card gives them a card-width
/// column rather than the knob row's inline strip.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SwatchStoryCard(children: Element) -> Element {
    rsx! {
        div { class: "tw:grid tw:w-full tw:max-w-[420px] tw:gap-5 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6",
            {children}
        }
    }
}

fn held_palette() -> GradientConfig {
    GradientConfig::Static(sunset_gradient())
}

#[story(
    description = "A held palette: one full-width strip — that IS the palette, completely — with the name above it and its stop count in the readout. The chevron says a chooser lives behind the band."
)]
fn held() -> Element {
    rsx! {
        SwatchStoryCard {
            PanelControl {
                control: palette_swatch_control(
                    "Palette",
                    &held_palette(),
                    UiSlotFieldState::editable(),
                    false,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "A cycling palette: the member SET as equal segments of one band, so the control says `these, in turn` at a glance, and the rate rides the readout as `↻ 4 · 3/min` — the same auto-denominated units the phasor speed knob uses. No live member ring: a panel control has no phase reading in hand (the timebase φ lives on the clock face's probe), so highlighting which member is showing right now would be a guess."
)]
fn cycle() -> Element {
    rsx! {
        SwatchStoryCard {
            PanelControl {
                control: palette_swatch_control(
                    "Palette",
                    &palette_cycle(),
                    UiSlotFieldState::editable(),
                    false,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The two modes side by side, and a frozen cycle under them: a `step 0` cycle holds whichever member it is on, so it reads `↻ 2 · held` rather than a rate it does not have."
)]
fn modes() -> Element {
    let frozen = GradientConfig::Cycle {
        set: match palette_cycle() {
            GradientConfig::Cycle { set, .. } => set.into_iter().take(2).collect(),
            GradientConfig::Static(gradient) => vec![gradient],
        },
        step_seconds: 0.0,
        fade_seconds: 0.0,
    };
    rsx! {
        SwatchStoryCard {
            PanelControl {
                control: palette_swatch_control(
                    "Held",
                    &held_palette(),
                    UiSlotFieldState::editable(),
                    false,
                ),
                on_action: move |_| {},
            }
            PanelControl {
                control: palette_swatch_control(
                    "Cycling",
                    &palette_cycle(),
                    UiSlotFieldState::editable(),
                    false,
                ),
                on_action: move |_| {},
            }
            PanelControl {
                control: palette_swatch_control(
                    "Frozen",
                    &frozen,
                    UiSlotFieldState::editable(),
                    false,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "State treatments, no new color family: a BOUND swatch wears the violet frame and its readout leads in violet, because a config channel is driving the slot — what comes back from that channel is a summary in WORDS (on the readout's tooltip and in the label popup), so the strips keep showing the authored palette. Under it, an unsaved edit's warning label."
)]
fn states() -> Element {
    let mut bound = palette_swatch_control(
        "Palette",
        &palette_cycle(),
        UiSlotFieldState::editable(),
        true,
    );
    bound.live_value = Some(lpa_studio_core::app::project::format_gradient_summary(
        &palette_cycle(),
    ));
    rsx! {
        SwatchStoryCard {
            PanelControl { control: bound, on_action: move |_| {} }
            PanelControl {
                control: palette_swatch_control(
                    "Palette",
                    &held_palette(),
                    UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty),
                    false,
                ),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Detail pinned open via the LABEL trigger — the same slot-row popover every other panel control opens, with the palette's authored value stated as its summary line rather than the 24-entry padded storage."
)]
fn detail_open() -> Element {
    rsx! {
        div { class: "tw:flex tw:min-h-[420px] tw:w-full tw:max-w-[520px] tw:items-start tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-6 tw:pl-24",
            PanelControl {
                control: palette_swatch_control(
                    "Palette",
                    &held_palette(),
                    UiSlotFieldState::editable(),
                    false,
                ),
                detail_initially_open: true,
                on_action: move |_| {},
            }
        }
    }
}
