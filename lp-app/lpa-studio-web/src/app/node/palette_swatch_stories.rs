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

/// One engaged swatch: a panel writer holds the channel, so the frame and
/// readout wear whatever the engaged family resolves to in this subtree.
fn engaged_swatch(label: &str) -> lpa_studio_core::UiPanelControl {
    let mut control =
        palette_swatch_control(label, &palette_cycle(), UiSlotFieldState::editable(), true);
    if let Some(target) = control.panel_target.as_mut() {
        target.engaged = true;
    }
    control.live_value = Some(lpa_studio_core::app::project::format_gradient_summary(
        &palette_cycle(),
    ));
    control
}

const CANDIDATE_LABEL_CLASS: &str =
    "tw:text-[10px] tw:font-bold tw:uppercase tw:tracking-[0.08em] tw:text-subtle-foreground";

#[story(
    description = "GATE DECISION — the engaged color family, three candidates on the same engaged swatch (a panel writer holds the channel). A: the shipped stand-in, the existing amber `status-attention` family. B: the spike's gold (#e4c065), raw — border and text both wear the bright value, as the spike drew it. C: a minted `status-engaged` family — the same gold hue laddered like every other status family (dark tinted bg, mid border, bright text), which is what a real token family would ship as. The vars are overridden story-locally; the app is NOT restyled."
)]
fn engaged_family_candidates() -> Element {
    rsx! {
        SwatchStoryCard {
            div { class: "tw:grid tw:gap-1.5",
                span { class: CANDIDATE_LABEL_CLASS, "A — amber status-attention (shipped stand-in)" }
                PanelControl { control: engaged_swatch("Palette"), on_action: move |_| {} }
            }
            div {
                class: "tw:grid tw:gap-1.5",
                style: "--studio-status-attention-bg: rgba(228,192,101,.10); --studio-status-attention-border: #e4c065; --studio-status-attention-text: #e4c065;",
                span { class: CANDIDATE_LABEL_CLASS, "B — the spike's gold, raw (#e4c065)" }
                PanelControl { control: engaged_swatch("Palette"), on_action: move |_| {} }
            }
            div {
                class: "tw:grid tw:gap-1.5",
                style: "--studio-status-attention-bg: #292213; --studio-status-attention-border: #8a6f35; --studio-status-attention-text: #e8c56b;",
                span { class: CANDIDATE_LABEL_CLASS, "C — minted status-engaged (gold, laddered)" }
                PanelControl { control: engaged_swatch("Palette"), on_action: move |_| {} }
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
