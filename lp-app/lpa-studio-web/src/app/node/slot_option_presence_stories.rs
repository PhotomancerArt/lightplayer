//! Stories for option-presence rendering: option-ness as PRESENCE instead of
//! a boolean-looking toggle, with the retired candidates kept side by side.
//!
//! Fixture (the acid test): `brightness: Option<u32>` in set/unset ×
//! clean/dirty, plus `gamma_correction: Option<bool>` set/unset — the
//! double-toggle problem the rework killed (with the retired toggle, a set
//! `Option<bool>` rendered the bool control NEXT TO a look-alike some/none
//! toggle). Every style holds the stable-width value cell in both flip
//! directions: the editor and the "not set" chip render into one
//! reserved-width box ([`OptionPresenceCell`]), so a presence flip cannot
//! reflow the row — the set→unset jump is gone by construction (adjacent
//! set/unset rows in the baselines are the pixel record: the cell edges
//! align in every state).
//!
//! P5 implemented P4's recommendation: **presence-in-row is the live
//! default** (the toggle is deleted); candidates B (child row) and C
//! (checkbox square) stay story-only for the P7 review swap. The width-tier
//! story documents the P4 width-hint caveat's handling: kinds wider than the
//! base 8rem box reserve a wider cell ([`OptionPresenceWidth`]), always as a
//! min — never a max — so wider editors span from the leading edge only and
//! the trailing gesture anchors never move.

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectNodeAddress, ProjectSlotAddress, ProjectSlotRoot, SlotPath, UiConfigSlot,
    UiNodeDirtyState, UiSlotEditorHint, UiSlotFieldState, UiSlotOptionality, UiSlotSourceState,
    UiSlotValue,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::{ConfigSlotRow, OptionPresenceStyle};

#[story(
    label = "Option Presence A In Row",
    description = "The LIVE DEFAULT (P5): no toggle on option rows — unset is a dashed stable-width `not set` chip with a plus set affordance, set is the value editor in the same reserved cell with a trash clear affordance; brightness Option<u32> set/unset × clean/dirty plus the Option<bool> acid rows. Adjacent set/unset rows pin the jump regression: the cell and gesture-button edges align in both flip directions."
)]
pub(crate) fn presence_in_row() -> Element {
    candidate_rows(OptionPresenceStyle::PresenceInRow)
}

#[story(
    label = "Option Presence Width Tiers",
    description = "The width-hint pass over the P4 caveat: editors wider than the base 8rem box reserve a wider cell per value-kind class (vec3 and XY = wide, slider and mat4 = extra-wide), as a min not a max, so a set wide-kind editor never collapses below its intrinsic footprint; the unset chip always holds the base box (an absent value carries no kind), and the end-aligned value area keeps every trailing gesture anchor fixed regardless."
)]
pub(crate) fn width_tiers() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            ConfigSlotRow {
                slot: optional_value("origin", "Origin", UiSlotValue::vec3([0.25, 0.5, 0.75])),
                depth: 0,
                index: 0,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: optional_value("intensity", "Intensity", slider_value(0.4)),
                depth: 0,
                index: 1,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: optional_value("anchor", "Anchor", xy_value([0.25, 0.75])),
                depth: 0,
                index: 2,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: optional_unset("fade_curve", "Fade curve"),
                depth: 0,
                index: 3,
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    label = "Option Presence B Child Row",
    description = "Story-only candidate B: the option row is presence-only (`set`/`not set` chips in the stable-width cell, plus set/clear affordances) and the interior value renders as an indented child row while set — the map-entry rhyme; same fixture states as the live default."
)]
pub(crate) fn child_row() -> Element {
    candidate_rows(OptionPresenceStyle::ChildRow)
}

#[story(
    label = "Option Presence C Checkbox",
    description = "Story-only candidate C (minimal delta): the trailing gesture slot keeps a compact presence indicator, restyled as a checkbox-square (check glyph when set, empty square when unset) so it cannot be read as the slider family, over the same stable-width value cell; same fixture states."
)]
pub(crate) fn checkbox_square() -> Element {
    candidate_rows(OptionPresenceStyle::CheckboxSquare)
}

#[story(
    label = "Option Presence Candidates",
    description = "The live default (presence in row) beside the two story-only candidates over the Option<bool> acid pair (set + unset): only the default leaves a single boolean-shaped control on the set gamma row; the user swaps the pick at the P7 review if preferred."
)]
pub(crate) fn candidates_side_by_side() -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-4",
            {candidate_section("Live default — presence in row", OptionPresenceStyle::PresenceInRow)}
            {candidate_section("Candidate B — child value row", OptionPresenceStyle::ChildRow)}
            {candidate_section("Candidate C — checkbox square", OptionPresenceStyle::CheckboxSquare)}
        }
    }
}

/// One candidate's acid-test strip: the `Option<bool>` pair (set then
/// unset) preceded by the `Option<u32>` set/unset pair, so both the
/// double-toggle read and the width stability compare across sections.
fn candidate_section(title: &'static str, style: OptionPresenceStyle) -> Element {
    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:gap-1",
            h3 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground",
                "{title}"
            }
            div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-muted tw:divide-y tw:divide-border-muted",
                ConfigSlotRow {
                    slot: optional_brightness(true, false),
                    depth: 0,
                    index: 0,
                    option_presence: style,
                    on_action: move |_| {},
                }
                ConfigSlotRow {
                    slot: optional_brightness(false, false),
                    depth: 0,
                    index: 1,
                    option_presence: style,
                    on_action: move |_| {},
                }
                ConfigSlotRow {
                    slot: optional_gamma(true),
                    depth: 0,
                    index: 2,
                    option_presence: style,
                    on_action: move |_| {},
                }
                ConfigSlotRow {
                    slot: optional_gamma(false),
                    depth: 0,
                    index: 3,
                    option_presence: style,
                    on_action: move |_| {},
                }
            }
        }
    }
}

/// The full per-candidate state matrix: `brightness: Option<u32>` set/unset
/// × clean/dirty, then the `gamma_correction: Option<bool>` acid pair.
/// Adjacent set/unset rows make the stable-width cell verifiable in the
/// baseline PNG: the value boxes' leading edges align across rows.
fn candidate_rows(style: OptionPresenceStyle) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:divide-y tw:divide-border-muted",
            ConfigSlotRow {
                slot: optional_brightness(true, false),
                depth: 0,
                index: 0,
                option_presence: style,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: optional_brightness(false, false),
                depth: 0,
                index: 1,
                option_presence: style,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: optional_brightness(true, true),
                depth: 0,
                index: 2,
                option_presence: style,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: optional_brightness(false, true),
                depth: 0,
                index: 3,
                option_presence: style,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: optional_gamma(true),
                depth: 0,
                index: 4,
                option_presence: style,
                on_action: move |_| {},
            }
            ConfigSlotRow {
                slot: optional_gamma(false),
                depth: 0,
                index: 5,
                option_presence: style,
                on_action: move |_| {},
            }
        }
    }
}

/// `brightness: Option<u32>` in one of its four states. Dirty set = an
/// unsaved value edit on the interior `some`; dirty unset = a pending clear
/// (the structural edit entry sits at the option path itself) — both wear
/// the amber chrome plus the inline revert.
fn optional_brightness(included: bool, dirty: bool) -> UiConfigSlot {
    let address = story_slot_address("brightness");
    let state = if dirty {
        UiSlotFieldState::editable().with_dirty(UiNodeDirtyState::Dirty)
    } else {
        UiSlotFieldState::editable()
    };
    let slot = if included {
        UiConfigSlot::value("brightness", "Brightness", UiSlotValue::u32(255))
            .with_optionality(UiSlotOptionality::included(true))
    } else {
        UiConfigSlot::empty("brightness", "Brightness")
            .with_optionality(UiSlotOptionality::excluded(true))
            .with_source(UiSlotSourceState::Unset)
    };
    let slot = slot.with_address(address.clone()).with_state(state);
    if dirty {
        slot.with_edit_entry_address(address)
    } else {
        slot
    }
}

/// `gamma_correction: Option<bool>` — the acid test: when set, the value is
/// a REAL bool control, so whatever renders the option-ness beside it must
/// not read as a second boolean.
fn optional_gamma(included: bool) -> UiConfigSlot {
    let address = story_slot_address("gamma_correction");
    let slot = if included {
        UiConfigSlot::value(
            "gamma_correction",
            "Gamma correction",
            UiSlotValue::bool(true),
        )
        .with_optionality(UiSlotOptionality::included(true))
    } else {
        UiConfigSlot::empty("gamma_correction", "Gamma correction")
            .with_optionality(UiSlotOptionality::excluded(true))
            .with_source(UiSlotSourceState::Unset)
    };
    slot.with_address(address)
        .with_state(UiSlotFieldState::editable())
}

/// A set, editable optional value row (width-tier story fixture).
fn optional_value(key: &str, label: &str, value: UiSlotValue) -> UiConfigSlot {
    UiConfigSlot::value(key, label, value)
        .with_address(story_slot_address(key))
        .with_optionality(UiSlotOptionality::included(true))
        .with_state(UiSlotFieldState::editable())
}

/// An unset optional row: the body is `Empty` (no kind), so the chip always
/// holds the base-tier box.
fn optional_unset(key: &str, label: &str) -> UiConfigSlot {
    UiConfigSlot::empty(key, label)
        .with_address(story_slot_address(key))
        .with_optionality(UiSlotOptionality::excluded(true))
        .with_source(UiSlotSourceState::Unset)
        .with_state(UiSlotFieldState::editable())
}

fn slider_value(value: f32) -> UiSlotValue {
    UiSlotValue::f32(value).with_editor(UiSlotEditorHint::Slider {
        min: 0.0,
        max: 1.0,
        step: None,
    })
}

fn xy_value(value: [f32; 2]) -> UiSlotValue {
    UiSlotValue::vec2(value).with_editor(UiSlotEditorHint::Xy)
}

fn story_slot_address(path: &str) -> ProjectSlotAddress {
    ProjectSlotAddress::new(
        ProjectNodeAddress::parse("/demo.project/clock.clock").expect("valid story node address"),
        ProjectSlotRoot::def(),
        SlotPath::parse(path).expect("valid story slot path"),
    )
}
