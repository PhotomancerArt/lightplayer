//! A board another tab of this browser holds, in the card's words
//! (`docs/adr/2026-10-08-the-board-card-and-one-home-page.md`, section 5,
//! "Amendment: how a port gets one holder tab").
//!
//! The state, the facts and the verb exist already (`DeviceView.held_elsewhere`,
//! `DeviceRosterView.take_overs`, the offer `devices/<board>/take-over`); this
//! file only arranges them. The link row, the picture's line and the
//! corner's state line each ask it for their words, so a held card says one
//! thing everywhere; the surface and the primary
//! ([`super::board_surface`], [`super::primary_action`]) say the state in
//! sentences and press `take-over` as **Connect here**.
//!
//! | The tab has | Connection bar | Surface | Primary |
//! |---|---|---|---|
//! | the board held by another tab | "Open in another tab", the editor-open or busy aside | "This board is open in another tab." | **Connect here** (`take-over`) |
//! | it held, the holder busy | the same, the busy label aside | "This board is busy in another tab." | — |
//! | let go on request | "Taken by another tab" | "Another tab took this board." | **Connect here** (`take-over`, takes it back) |
//! | an ask out, or the board opening | work, running: "Asking the other tab…" / "Opening…" | "Moving this board to this tab." | the spinner |
//! | a take-over that failed | work, striped: the reason, with Retry | "Studio couldn't move this board to this tab." | **Connect here** (`take-over`) |
//!
//! The row of the link it is held on says "someone has it" quietly now: the surface is the one
//! loud place (the rows go quiet). The words name facts only: no tab id, no
//! key, no browser.

use lpa_devices::{HeldElsewhere, HoldLevel};

use super::board_card_input::BoardCardInput;
use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_card_action::UiCardAction;
use crate::app::devices::take_over_offer::busy_in_the_other_tab;

/// The link row's summary for a board another tab holds.
pub const OPEN_IN_ANOTHER_TAB: &str = "Open in another tab";
/// … for a board this tab let go because the other tab asked.
pub const TAKEN_BY_ANOTHER_TAB: &str = "Taken by another tab";
/// The aside when the holder has its editor open on the board.
pub const EDITOR_OPEN_ASIDE: &str = "editor open";
/// The corner's picture line: a held board's picture is the library's.
pub const HELD_PICTURE_LINE: &str = "The last picture another tab saved.";
/// … and when no tab has saved one yet.
pub const HELD_NO_PICTURE_LINE: &str = "Another tab has this board. It has saved no picture yet.";

const HAS_IT_SENTENCE: &str = "Another tab of this browser has this board open";
const EDITOR_OPEN_CLAUSE: &str = ", and its editor is open";
const TOOK_IT_SENTENCE: &str = "Another tab of this browser took this board from this tab";
const TAKE_OVER_CLOSES_SENTENCE: &str =
    "Taking it over closes it there. You can take it back the same way.";

/// The link row's summary: who has the board.
pub fn held_summary(held: &HeldElsewhere) -> &'static str {
    match held.taken_from_here {
        true => TAKEN_BY_ANOTHER_TAB,
        false => OPEN_IN_ANOTHER_TAB,
    }
}

/// The bar's aside: that the holder's editor is open, or what it is busy
/// with. Nothing for a tab that only holds the board, or one that let go.
pub fn held_aside(held: &HeldElsewhere) -> Option<String> {
    if held.taken_from_here {
        return None;
    }
    match &held.level {
        HoldLevel::Watching => None,
        HoldLevel::Open => Some(EDITOR_OPEN_ASIDE.to_string()),
        HoldLevel::Busy(label) => Some(label.clone()),
    }
}

/// The details' notice, plainly: who has it and what taking it costs.
/// `undoable` is the `take-over` offer's own consequence (it closes an
/// editor over there).
pub fn held_sentence(held: &HeldElsewhere, undoable: bool) -> String {
    let mut sentence = match (held.taken_from_here, &held.level) {
        (true, _) => format!("{TOOK_IT_SENTENCE}."),
        (false, HoldLevel::Watching) => format!("{HAS_IT_SENTENCE}."),
        (false, HoldLevel::Open) => format!("{HAS_IT_SENTENCE}{EDITOR_OPEN_CLAUSE}."),
        (false, HoldLevel::Busy(label)) => {
            format!("{HAS_IT_SENTENCE}. {}.", busy_in_the_other_tab(label))
        }
    };
    if undoable {
        sentence.push(' ');
        sentence.push_str(TAKE_OVER_CLOSES_SENTENCE);
    }
    sentence
}

/// The link row's work for a take-over: running while the other tab
/// is asked and while the board opens here; striped, with Retry on the same
/// `take-over`, when it ended without the board.
pub(crate) fn take_over_work(input: &BoardCardInput<'_>) -> Option<UiBarWork> {
    let over = input.take_over.filter(|_| input.no_open_link())?;
    let state = match over.failed {
        false => BarWorkState::Running,
        true => BarWorkState::Failed {
            retry: input
                .offer("take-over")
                .map(|offer| UiCardAction::press(offer, "Retry").with_icon("retry")),
        },
    };
    Some(UiBarWork {
        words: over.words.clone(),
        detail: None,
        percent: None,
        state,
        cancel: None,
        other_device: false,
    })
}

#[cfg(test)]
mod tests {
    use lpa_devices::HoldVia;

    use super::super::card_fixtures::CardFixture;
    use super::*;

    #[test]
    fn the_summary_says_who_has_it_and_the_aside_what_they_are_doing() {
        let mut held = held(HoldLevel::Watching, false);
        assert_eq!(held_summary(&held), "Open in another tab");
        assert_eq!(held_aside(&held), None);

        held.level = HoldLevel::Open;
        assert_eq!(held_aside(&held).as_deref(), Some("editor open"));

        held.level = HoldLevel::Busy("Updating \u{b7} 42%".to_string());
        assert_eq!(held_aside(&held).as_deref(), Some("Updating \u{b7} 42%"));

        held.taken_from_here = true;
        assert_eq!(held_summary(&held), "Taken by another tab");
        assert_eq!(held_aside(&held), None, "a tab that let go has no aside");
    }

    #[test]
    fn the_sentence_names_the_facts_plainly_and_what_taking_it_costs() {
        assert_eq!(
            held_sentence(&held(HoldLevel::Watching, false), false),
            "Another tab of this browser has this board open."
        );
        assert_eq!(
            held_sentence(&held(HoldLevel::Open, false), true),
            "Another tab of this browser has this board open, and its editor is open. \
             Taking it over closes it there. You can take it back the same way."
        );
        assert_eq!(
            held_sentence(&held(HoldLevel::Busy("Pushing".to_string()), false), false),
            "Another tab of this browser has this board open. Busy in the other tab: Pushing."
        );
        assert_eq!(
            held_sentence(&held(HoldLevel::Watching, true), false),
            "Another tab of this browser took this board from this tab."
        );
    }

    #[test]
    fn a_board_this_tab_has_a_link_to_is_not_held_from_here() {
        let mut fixture = CardFixture::ready();
        fixture.view.held_elsewhere = Some(held(HoldLevel::Watching, false));
        fixture.view.held_elsewhere.as_mut().unwrap().via = HoldVia::Network;
        assert!(fixture.input().held().is_none());
        assert!(take_over_work(&fixture.input()).is_none());
    }

    fn held(level: HoldLevel, taken_from_here: bool) -> HeldElsewhere {
        HeldElsewhere {
            via: HoldVia::Usb,
            level,
            taken_from_here,
        }
    }
}
