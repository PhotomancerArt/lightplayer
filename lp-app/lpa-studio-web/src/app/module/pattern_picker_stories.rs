//! Stories for the Play-mode Pattern instrument (multi-pattern plan P7).
//!
//! Every story is Play mode at phone width, because the phone is where this
//! instrument lives: shared knobs on top, then the Pattern group, then the
//! playing pattern's own knobs (vision D15/D21). The picker in each is
//! built by core's own derivation from device-honest facts — one entry
//! loaded, every other known by its def's key and name.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use super::PlayModeSurface;
use super::pattern_picker_fixtures::{
    CHOKER_SET, TWENTY_FIVE, apply_picker_action, authored_tour, piece_panel, set_facts, skip_held,
    tour_held,
};

/// The phone frame every story renders in.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn Phone(children: Element) -> Element {
    rsx! {
        div { class: "tw:h-[900px] tw:w-[375px] tw:overflow-auto tw:border tw:border-border",
            {children}
        }
    }
}

#[story(
    description = "Touring: the choker's five patterns walking every 20 s (authored in the playlist file). Aurora is playing — named between prev and next, and highlighted in the list in the live family, the playlist strip's ACTIVE colour. Shared knobs above, Aurora's own knobs below."
)]
fn touring() -> Element {
    let mut facts = set_facts(CHOKER_SET, 2);
    facts.authored_tour = Some(authored_tour(20.0));
    facts.tour = authored_tour(20.0);
    rsx! {
        Phone {
            PlayModeSurface { panel: piece_panel(&facts), on_panel: move |_| {}, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "Holding: the tour is off, so the playlist stays on the playing pattern (Noise soft) and the step time is not shown — only the Tour switch that starts it. Prev/next and tapping a name still switch by hand."
)]
fn holding() -> Element {
    let facts = set_facts(CHOKER_SET, 1);
    rsx! {
        Phone {
            PlayModeSurface { panel: piece_panel(&facts), on_panel: move |_| {}, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "One pattern switched off: Twinkle is skipped in the tour (dimmed, dashed, its block empty). The switch set is held by the panel — a Play-mode choice, remembered in panel.json — so the blocks wear the engaged gold and a reset glyph releases them back to the playlist's own list. Next from Aurora goes to Scanner, passing Twinkle."
)]
fn one_skipped() -> Element {
    let mut facts = skip_held(set_facts(CHOKER_SET, 2));
    facts.skip = vec![3];
    facts.tour = authored_tour(20.0);
    facts.authored_tour = Some(authored_tour(20.0));
    rsx! {
        Phone {
            PlayModeSurface { panel: piece_panel(&facts), on_panel: move |_| {}, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "One pattern failed: Scanner did not load or compile on the device, so the playlist marked it failed and moved on (it reads the playlist's warning). It says so in the error family; the tour and next/prev pass it by; a tap tries it again."
)]
fn one_failed() -> Element {
    let mut facts = set_facts(CHOKER_SET, 5);
    facts.failed = vec![4];
    facts.tour = authored_tour(20.0);
    facts.authored_tour = Some(authored_tour(20.0));
    rsx! {
        Phone {
            PlayModeSurface { panel: piece_panel(&facts), on_panel: move |_| {}, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "Twenty-five patterns at phone width (375 px), the plan's target set: one name per row, the list scrolling inside its own box so the playing pattern's knobs stay one flick away. The tour here was turned on from Play mode (gold, with its reset glyph), every 30 s."
)]
fn twenty_five_at_phone_width() -> Element {
    let mut facts = tour_held(set_facts(TWENTY_FIVE, 12));
    facts.tour = authored_tour(30.0);
    rsx! {
        Phone {
            PlayModeSurface { panel: piece_panel(&facts), on_panel: move |_| {}, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "A two-entry playlist, fyeah-sign's shape: an idle pattern and a triggered blast. Holding (fyeah authors no tour), idle playing. The instrument is the same one — two names, and next/prev simply swap them."
)]
fn two_entries() -> Element {
    let facts = set_facts(&["idle", "blast"], 1);
    rsx! {
        Phone {
            PlayModeSurface { panel: piece_panel(&facts), on_panel: move |_| {}, on_action: move |_| {} }
        }
    }
}

#[story(
    description = "Walkable: every gesture answers the way the device does. Tap a name to play it, prev/next, switch patterns off and on, turn the tour on and step it shorter or longer — the picker re-derives from the new facts through core's own derivation."
)]
fn walk() -> Element {
    let mut facts = use_signal(|| set_facts(CHOKER_SET, 1));
    let panel = piece_panel(&facts());
    rsx! {
        Phone {
            PlayModeSurface {
                panel,
                on_panel: move |_| {},
                on_action: move |action| facts.with_mut(|facts| apply_picker_action(facts, &action)),
            }
        }
    }
}
