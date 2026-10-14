//! The hardware bar: what the board is.
//!
//! First match wins:
//!
//! | Board | Summary | Aside |
//! |---|---|---|
//! | A stand-in | "Emulated <board>" / "Simulated <board>" | "in this tab" |
//! | Its board is known | the board ("XIAO ESP32-C6") | — |
//! | Only its chip is known | the chip ("ESP32-C6") | — |
//! | Nothing | "Not known yet" | — |
//!
//! Neutral, with no action. A stand-in says so first, because that is the
//! one thing about a runtime a person has to know without opening anything
//! (the runtime band's mark, moved here). The details hold the model, the
//! chip, the board's id (its MAC, shown here once and nowhere else), a
//! stand-in's speed or tier, Rename, Reset with its reason, and Forget
//! apart.

use lpa_devices::view::PendingLinkView;

use super::board_card_input::{BoardCardInput, offer_at};
use super::detail_sections::{danger, facts, verbs, without_empty};
use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::device_identity::{PENDING_CHIP_UNKNOWN, device_identity_line};
use crate::{OfferPath, RichLine, UiOffer, UiStatusKind};

/// The hardware bar.
pub(crate) fn hardware_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let identity = device_identity_line(input.view);
    let chip = identity.chip.as_deref().map(chip_words);
    let (summary, aside) = match (input.runtime, &identity.board, &chip) {
        (Some(runtime), _, _) => (
            format!("{} {}", stand_in_word(runtime.kind), runtime.target),
            Some(runtime.locality.to_string()),
        ),
        (None, Some(board), _) => (board.clone(), None),
        (None, None, Some(chip)) => (chip.clone(), None),
        (None, None, None) => ("Not known yet".to_string(), None),
    };
    let mut lines = Vec::new();
    if let Some(board) = identity
        .board
        .clone()
        .or_else(|| input.runtime.map(|runtime| runtime.target.clone()))
    {
        lines.push(RichLine::new("Model", board));
    }
    if let Some(chip) = chip {
        lines.push(RichLine::new("Chip", chip));
    }
    if let Some(id) = &input.view.identity_label {
        lines.push(RichLine::new("Id", id.clone()));
    }
    if let Some(runtime) = input.runtime {
        if let Some(speed) = &runtime.speed {
            lines.push(RichLine::new("Speed", speed.clone()));
        }
        if let Some(tier) = runtime.tier {
            lines.push(RichLine::new("Tier", tier));
        }
    }
    let rename = input.offer("rename");
    // Reset stays in details while work runs, refused with its reason
    // (the model refuses a reset under an activity; Cancel is the way out).
    let reset = input.offer("reset-board");
    let forget = input.offer("forget");
    UiStackBar {
        layer: BarLayer::Hardware,
        icon: "chip".to_string(),
        summary,
        aside,
        aside_icon: None,
        tone: UiStatusKind::Neutral,
        action: None,
        work: None,
        details: UiBarDetails {
            sections: without_empty(vec![
                facts("Board", lines),
                verbs(reset.map(own_words).into_iter().collect()),
                danger(forget.map(own_words).into_iter().collect()),
            ]),
            panels: rename
                .map(|rename| UiDetailPanel::Rename {
                    offer: rename.path.clone(),
                    title: input.view.title.clone(),
                })
                .into_iter()
                .collect(),
            raised: false,
        },
    }
}

/// A new board's hardware bar: the chip its boot banner named, its MAC once
/// something read it, and the way to keep it, reset it, or dismiss it.
pub(crate) fn pending_hardware_bar(
    pending: &PendingLinkView,
    board: &OfferPath,
    offers: &[UiOffer],
) -> UiStackBar {
    let chip = pending
        .detected_chip
        .as_deref()
        .map(chip_words)
        .unwrap_or_else(|| PENDING_CHIP_UNKNOWN.to_string());
    let mut lines = vec![RichLine::new("Chip", chip.clone())];
    if let Some(mac) = &pending.mac {
        lines.push(RichLine::new("Id", mac.clone()));
    }
    let verb = |name: &str| offer_at(offers, board, name).map(own_words);
    UiStackBar {
        layer: BarLayer::Hardware,
        icon: "chip".to_string(),
        summary: chip,
        aside: None,
        aside_icon: None,
        tone: UiStatusKind::Neutral,
        action: None,
        work: None,
        details: UiBarDetails {
            sections: without_empty(vec![
                facts("Board", lines),
                verbs(
                    verb("adopt")
                        .into_iter()
                        .chain(verb("reset-board"))
                        .collect(),
                ),
                danger(verb("dismiss").into_iter().collect()),
            ]),
            panels: Vec::new(),
            raised: false,
        },
    }
}

/// The words a stand-in's kind leads its board with.
fn stand_in_word(kind: &str) -> &'static str {
    match kind {
        "Emu" => "Emulated",
        _ => "Simulated",
    }
}

/// A chip family in the catalog's words ("esp32c6" → "ESP32-C6"): the
/// `soc` of the first catalog board of that family, else the family as the
/// board reported it.
pub fn chip_words(family: &str) -> String {
    lpa_boards::all_boards()
        .iter()
        .find(|board| board.family == family)
        .map_or_else(|| family.to_string(), |board| board.soc.clone())
}

/// An offer in its own words, with its own icon.
fn own_words(offer: &UiOffer) -> UiCardAction {
    UiCardAction::own_words(offer).with_icon(offer.icon.clone())
}

#[cfg(test)]
mod tests {
    use super::super::card_fixtures::CardFixture;
    use super::super::primary_action::tests::pending_view;
    use super::*;
    use crate::UiRuntimeBand;

    #[test]
    fn a_stand_in_says_emulated_or_simulated_in_this_tab() {
        let mut fixture = CardFixture::ready();
        fixture.runtime = Some(UiRuntimeBand::emu("seeed/xiao-esp32-c6", Some(0.5)));
        let bar = hardware_bar(&fixture.input());
        assert_eq!(bar.summary, "Emulated XIAO ESP32-C6");
        assert_eq!(bar.aside.as_deref(), Some("in this tab"));
        assert_eq!(line(&bar, "Speed").as_deref(), Some("0.5×"));
        fixture.runtime = Some(UiRuntimeBand::sim("seeed/xiao-esp32-c6", Some("gpu")));
        let bar = hardware_bar(&fixture.input());
        assert_eq!(bar.summary, "Simulated XIAO ESP32-C6");
        assert_eq!(line(&bar, "Tier").as_deref(), Some("GPU"));
    }

    #[test]
    fn a_known_board_names_its_model_and_its_id_once() {
        let mut fixture = CardFixture::ready();
        let bar = hardware_bar(&fixture.input());
        assert_eq!(bar.summary, "XIAO ESP32-C6");
        assert_eq!(bar.aside, None);
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        assert_eq!(bar.action, None);
        assert_eq!(line(&bar, "Chip").as_deref(), Some("ESP32-C6"));
        assert_eq!(line(&bar, "Id").as_deref(), Some("a0:f2:62:87:b4:8c"));
        assert!(!bar.summary.contains("a0:f2"), "never a MAC on the face");
        assert!(matches!(
            bar.details.panels.as_slice(),
            [UiDetailPanel::Rename { title, .. }] if title == "Porch"
        ));
        let verbs: Vec<&str> = bar
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter().map(|a| a.word.as_str()))
            .collect();
        assert_eq!(verbs, ["Reset", "Forget"]);
        let danger = bar.details.sections.last().unwrap();
        assert_eq!(danger.weight, crate::RichWeight::Danger);

        fixture.view.board_id = None;
        let bar = hardware_bar(&fixture.input());
        assert_eq!(
            bar.summary, "ESP32-C6",
            "only the chip, in the catalog's words"
        );
        fixture.view.detected_chip = None;
        assert_eq!(hardware_bar(&fixture.input()).summary, "Not known yet");
    }

    /// Ported from the remembered tile: an offline board still names the
    /// board it is (its record's board id).
    #[test]
    fn a_remembered_tile_names_the_board_and_when_it_was_heard() {
        let mut fixture = CardFixture::offline();
        assert_eq!(hardware_bar(&fixture.input()).summary, "XIAO ESP32-C6");
        fixture.view.board_id = None;
        fixture.view.detected_chip = None;
        assert_eq!(hardware_bar(&fixture.input()).summary, "Not known yet");
    }

    /// Reset refused over a link without the author tier keeps its reason.
    #[test]
    fn reset_refused_keeps_its_reason() {
        let mut fixture = CardFixture::ready().over(crate::UiLinkKind::Bluetooth);
        fixture.reset = crate::ResetReach::Request { author: false };
        let bar = hardware_bar(&fixture.input());
        let reset = bar
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter())
            .find(|action| action.word == "Reset")
            .expect("Reset");
        assert_eq!(reset.refused.as_deref(), Some(crate::RESET_NEEDS_AUTHOR));
    }

    #[test]
    fn a_new_boards_hardware_names_its_chip_and_offers_set_up_and_dismiss() {
        let mut pending = pending_view();
        let board = super::super::card_fixtures::board();
        let offers = crate::pending_link_offers(&pending, &board, crate::ResetReach::Lines);
        let bar = pending_hardware_bar(&pending, &board, &offers);
        assert_eq!(bar.summary, "ESP32-C6");
        let words: Vec<&str> = bar
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter().map(|a| a.word.as_str()))
            .collect();
        assert_eq!(words, ["Set up this device", "Reset", "Dismiss"]);
        pending.detected_chip = None;
        pending.mac = Some("60:55:f9:0a:0b:0c".to_string());
        let bar = pending_hardware_bar(&pending, &board, &offers);
        assert_eq!(bar.summary, PENDING_CHIP_UNKNOWN);
        assert_eq!(line(&bar, "Id").as_deref(), Some("60:55:f9:0a:0b:0c"));
    }

    fn line(bar: &UiStackBar, label: &str) -> Option<String> {
        bar.details
            .sections
            .iter()
            .flat_map(|section| section.lines.iter())
            .find(|line| line.label == label)
            .map(|line| line.value.clone())
    }
}
