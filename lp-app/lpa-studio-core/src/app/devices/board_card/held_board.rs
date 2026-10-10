//! A board another tab of this browser holds, in the card's words
//! (`docs/adr/2026-10-08-the-board-card-and-one-home-page.md`, section 5,
//! "Amendment: how a port gets one holder tab").
//!
//! The state, the facts and the verb exist already (`DeviceView.held_elsewhere`,
//! `DeviceRosterView.take_overs`, the offer `devices/<board>/take-over`); this
//! file only arranges them. The primary action, the connection bar, the
//! picture's line and the corner's state line each ask it for their words, so
//! a held card says one thing everywhere.
//!
//! | The tab has | Connection bar | Primary |
//! |---|---|---|
//! | the board held by another tab | "Open in another tab", the editor-open or busy aside, orange | **Connect** (`take-over`) |
//! | it held, the holder busy | the same, the busy label aside | Connect, disabled: "Busy in the other tab: <label>" |
//! | let go on request | "Taken by another tab", orange | **Connect** (`take-over`, takes it back) |
//! | an ask out, or the board opening | work, running: "Asking the other tab…" / "Opening…" | Connect, disabled: the same words |
//! | a take-over that failed | work, striped: the reason, with Retry | **Connect** (`take-over`) |
//!
//! Orange is the colour of "someone has it"; nothing here changes a hue. The
//! words name facts only: no tab id, no key, no browser.

use lpa_devices::{HeldElsewhere, HoldLevel};

use super::board_card_input::{BoardCardInput, link_icon};
use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_card_action::UiCardAction;
use super::ui_name_bar::UiPrimary;
use crate::app::devices::take_over_offer::busy_in_the_other_tab;

/// The connection bar's summary for a board another tab holds.
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

/// The connection bar's summary: who has the board.
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

/// The primary for a board another tab holds, or whose take-over is under
/// way: Connect, which is the take-over. `None` when this tab has an open
/// link to the board (it is not held from here).
pub(crate) fn held_primary(input: &BoardCardInput<'_>) -> Option<UiPrimary> {
    if !input.no_open_link() {
        return None;
    }
    let working = input.take_over.filter(|over| !over.failed);
    let held = input.held();
    if held.is_none() && working.is_none() {
        return None;
    }
    let icon = link_icon(input.held_link());
    let unavailable = |reason: String| UiPrimary::Unavailable {
        word: "Connect".to_string(),
        icon: icon.to_string(),
        reason,
    };
    if let Some(over) = working {
        return Some(unavailable(over.words.clone()));
    }
    let held = held?;
    if let HoldLevel::Busy(label) = &held.level {
        return Some(unavailable(busy_in_the_other_tab(label)));
    }
    Some(match input.offer("take-over") {
        Some(offer) => UiPrimary::Offer(UiCardAction::press(offer, "Connect").with_icon(icon)),
        // Nothing to press (this browser cannot ask its other tabs): the
        // word still says why.
        None => unavailable(held_summary(held).to_string()),
    })
}

/// The connection bar's work for a take-over: running while the other tab
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
        assert_eq!(held_primary(&fixture.input()), None);
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
