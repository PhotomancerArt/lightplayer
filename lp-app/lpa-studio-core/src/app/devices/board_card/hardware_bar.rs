//! The Hardware row: what the board is (`docs/ux/anatomy/board-card.md`,
//! "Hardware ■"). **Hardware · model · state**, the chip icon.
//!
//! First match wins:
//!
//! | Board | Name | State |
//! |---|---|---|
//! | Identifying | (work: "identifying") | — |
//! | A stand-in | the board it stands in for ("XIAO ESP32-S3 Plus") | — |
//! | Its board is known | the model ("XIAO ESP32-C6") | — |
//! | Only its chip is known | the chip ("ESP32-C6") | "model not set" |
//! | Nothing | "unknown" | — |
//!
//! Neutral, with no action. "Simulated" is said once, on the Link row (P2):
//! the model is the model. A chip name looks like a model, so a board known
//! only by its chip says so (P1). The details hold the model, the chip, the
//! board's id (its MAC, shown here once and nowhere else), a stand-in's
//! speed or tier, Rename, Reset with its reason, and apart, the two that
//! lose something: Factory reset (erases the whole board — firmware,
//! projects, settings) and Forget. Factory reset is here, not under
//! Firmware, because it erases the board (Yona, 2026-10-19: "I expect to
//! find factory reset in the hardware section").

use lpa_devices::view::PendingLinkView;

use super::bar_work::bar_work;
use super::board_card_input::{BoardCardInput, offer_at};
use super::detail_sections::{danger, facts, verbs, without_empty};
use super::ui_bar_work::UiBarWork;
use super::ui_card_action::UiCardAction;
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::device_identity::device_identity_line;
use crate::{OfferPath, RichLine, UiOffer};

/// The state of a board known only by its chip.
pub const MODEL_NOT_SET: &str = "model not set";
/// The name of a board nothing has named.
const HARDWARE_UNKNOWN: &str = "unknown";

/// The Hardware row.
pub(crate) fn hardware_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let identity = device_identity_line(input.view);
    let chip = identity.chip.as_deref().map(chip_words);
    let (summary, aside) = match (input.runtime, &identity.board, &chip) {
        (Some(runtime), _, _) => (runtime.target.clone(), None),
        (None, Some(board), _) => (board.clone(), None),
        (None, None, Some(chip)) => (chip.clone(), Some(MODEL_NOT_SET.to_string())),
        (None, None, None) => (HARDWARE_UNKNOWN.to_string(), None),
    };
    let work = bar_work(input, BarLayer::Hardware);
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
        lines.push(RichLine::new(
            "Runs",
            format!("{} {}", runtime.kind_word(), runtime.locality),
        ));
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
    let erase = input.offer("erase");
    let forget = input.offer("forget");
    // Identifying: the work's words take the name's place ("◌ Hardware
    // identifying"), as on a new board; the model is back once it is done.
    let identifying = work
        .as_ref()
        .is_some_and(|work| work.state == super::ui_bar_work::BarWorkState::Running);
    let summary = match identifying {
        true => String::new(),
        false => summary,
    };
    UiStackBar {
        aside: aside.filter(|_| work.is_none()),
        work,
        details: UiBarDetails {
            sections: without_empty(vec![
                facts("Board", lines),
                verbs(reset.map(own_words).into_iter().collect()),
                danger(erase.into_iter().chain(forget).map(own_words).collect()),
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
        ..UiStackBar::new(BarLayer::Hardware, "chip", summary)
    }
}

/// A new board's Hardware row: identifying while it says who it is (the
/// row's work; the name bar's spinner); then the chip its boot banner
/// named, "model not set" (a flash sets it), or "unknown"; its MAC once
/// something read it, and the way to keep it, reset it, or dismiss it.
pub(crate) fn pending_hardware_bar(
    pending: &PendingLinkView,
    board: &OfferPath,
    offers: &[UiOffer],
) -> UiStackBar {
    let chip = pending.detected_chip.as_deref().map(chip_words);
    let identifying = pending.firmware_face == lpa_devices::view::FirmwareFace::Unknown;
    let mut lines = vec![RichLine::new(
        "Chip",
        chip.clone().unwrap_or_else(|| HARDWARE_UNKNOWN.to_string()),
    )];
    if let Some(mac) = &pending.mac {
        lines.push(RichLine::new("Id", mac.clone()));
    }
    let verb = |name: &str| offer_at(offers, board, name).map(own_words);
    let (summary, aside) = match (&chip, identifying) {
        // The work's words take the name's place: "◌ Hardware identifying"
        // (the chip is the details' meanwhile).
        (_, true) => (String::new(), None),
        (Some(chip), false) => (chip.clone(), Some(MODEL_NOT_SET.to_string())),
        (None, false) => (HARDWARE_UNKNOWN.to_string(), None),
    };
    UiStackBar {
        aside,
        work: identifying.then(|| UiBarWork::running("identifying", None)),
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
        ..UiStackBar::new(BarLayer::Hardware, "chip", summary)
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
    use crate::{UiRuntimeBand, UiStatusKind};

    /// "Simulated" is said once, on the Link row (P2): the Hardware row
    /// names the model.
    #[test]
    fn a_stand_in_names_its_model_alone() {
        let mut fixture = CardFixture::ready();
        fixture.runtime = Some(UiRuntimeBand::emu("seeed/xiao-esp32-c6", Some(0.5)));
        let bar = hardware_bar(&fixture.input());
        assert_eq!(bar.summary, "XIAO ESP32-C6");
        assert_eq!(bar.aside, None);
        assert_eq!(bar.kind, Some("Hardware"));
        assert_eq!(line(&bar, "Runs").as_deref(), Some("Emulated in this tab"));
        assert_eq!(line(&bar, "Speed").as_deref(), Some("0.5×"));
        fixture.runtime = Some(UiRuntimeBand::sim("seeed/xiao-esp32-c6", Some("gpu")));
        let bar = hardware_bar(&fixture.input());
        assert_eq!(bar.summary, "XIAO ESP32-C6");
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
        assert_eq!(verbs, ["Reset", "Factory reset", "Forget"]);
        let danger = bar.details.sections.last().unwrap();
        assert_eq!(danger.weight, crate::RichWeight::Danger);
        assert_eq!(
            danger.affordances.len(),
            2,
            "Factory reset and Forget, apart"
        );

        fixture.view.board_id = None;
        let bar = hardware_bar(&fixture.input());
        assert_eq!(
            bar.summary, "ESP32-C6",
            "only the chip, in the catalog's words"
        );
        assert_eq!(
            bar.aside.as_deref(),
            Some("model not set"),
            "a chip looks like a model"
        );
        fixture.view.detected_chip = None;
        assert_eq!(hardware_bar(&fixture.input()).summary, "unknown");
    }

    /// Ported from the remembered tile: an offline board still names the
    /// board it is (its record's board id).
    #[test]
    fn a_remembered_tile_names_the_board_and_when_it_was_heard() {
        let mut fixture = CardFixture::offline();
        assert_eq!(hardware_bar(&fixture.input()).summary, "XIAO ESP32-C6");
        fixture.view.board_id = None;
        fixture.view.detected_chip = None;
        assert_eq!(hardware_bar(&fixture.input()).summary, "unknown");
    }

    /// Identifying is the Hardware row's work: it finds out what the board
    /// is.
    #[test]
    fn identifying_is_the_hardware_rows_work() {
        let mut fixture =
            CardFixture::ready().with_activity(super::super::card_fixtures::activity(
                lpa_devices::ActivityKind::Identify,
                "Identifying…",
                None,
            ));
        let bar = hardware_bar(&fixture.input());
        assert_eq!(bar.summary, "", "◌ Hardware identifying");
        let work = bar.work.expect("identifying");
        assert_eq!(work.words, "identifying");
        assert!(work.cancel.is_some());
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

    /// Factory reset erases the whole board, so it is the Hardware row's
    /// (Yona, 2026-10-19: "I expect to find factory reset in the hardware
    /// section"), Lasting, apart with Forget — and never the Firmware
    /// row's. Over a link that cannot carry firmware it keeps its reason.
    #[test]
    fn factory_reset_is_the_hardware_rows_and_keeps_its_reason() {
        let mut fixture = CardFixture::ready();
        let bar = hardware_bar(&fixture.input());
        let danger = bar.details.sections.last().expect("the danger section");
        assert_eq!(danger.weight, crate::RichWeight::Danger);
        let reset = danger.affordances.first().expect("Factory reset");
        assert_eq!(reset.word, "Factory reset");
        assert!(
            reset.offer.to_string().ends_with("/erase"),
            "{}",
            reset.offer
        );
        let firmware = super::super::firmware_bar::firmware_bar(&fixture.input());
        assert!(
            firmware
                .details
                .sections
                .iter()
                .flat_map(|section| section.affordances.iter())
                .all(|action| action.word != "Factory reset"),
            "not under Firmware"
        );

        let mut fixture = CardFixture::ready().over(crate::UiLinkKind::Bluetooth);
        let bar = hardware_bar(&fixture.input());
        let reset = bar
            .details
            .sections
            .iter()
            .flat_map(|section| section.affordances.iter())
            .find(|action| action.word == "Factory reset")
            .expect("Factory reset, refused");
        assert_eq!(
            reset.refused.as_deref(),
            Some(lpa_devices::view::FIRMWARE_NEEDS_USB)
        );
    }

    #[test]
    fn a_new_boards_hardware_names_its_chip_and_offers_set_up_and_dismiss() {
        let mut pending = pending_view();
        let board = super::super::card_fixtures::board();
        let offers = crate::pending_link_offers(&pending, &board, crate::ResetReach::Lines);
        let bar = pending_hardware_bar(&pending, &board, &offers);
        assert_eq!(bar.summary, "", "◌ Hardware identifying");
        assert_eq!(line(&bar, "Chip").as_deref(), Some("ESP32-C6"));
        assert_eq!(
            bar.work.as_ref().map(|work| work.words.as_str()),
            Some("identifying")
        );
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
        assert_eq!(bar.summary, "", "◌ Hardware identifying");
        assert_eq!(line(&bar, "Id").as_deref(), Some("60:55:f9:0a:0b:0c"));
        // Settled on needing firmware: the chip, and that its model is not
        // set yet (a flash sets it).
        pending.detected_chip = Some("esp32c6".to_string());
        pending.firmware_face = lpa_devices::view::FirmwareFace::Blank;
        let bar = pending_hardware_bar(&pending, &board, &offers);
        assert_eq!(
            (bar.summary.as_str(), bar.aside.as_deref()),
            ("ESP32-C6", Some("model not set"))
        );
        assert_eq!(bar.work, None);
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
