//! The one place a board's card is mounted on the home page (PD3).
//!
//! Online boards and Offline boards draw every board through
//! [`BoardCardSlot`], keyed by the section entry core listed
//! ([`UiHomeBoard`]). The slot draws the board's [`BoardCard`]: the card
//! core built for it ([`DeviceRosterView::cards`]), for all three kinds —
//! a new board, a board online, a board offline. The page decides where a
//! card sits, never what it says.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceRosterView, UiAction, UiBoardCard, UiBoardPresence, UiExampleCard, UiHomeBoard,
    UiHomeBoardKind, UiPackageCard,
};

use crate::app::board_card::BoardCard;

/// A board's card, found by the entry core listed. An entry with no card
/// of its kind draws nothing.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BoardCardSlot(
    /// The section's entry: which board, and where it stands.
    entry: UiHomeBoard,
    /// The roster, whose `cards` core built for this view.
    devices: DeviceRosterView,
    /// The project pick reads the page's own lists: there is no separate
    /// device-side project source.
    projects: Vec<UiPackageCard>,
    examples: Vec<UiExampleCard>,
    on_action: EventHandler<UiAction>,
) -> Element {
    match slot_card(&entry, &devices) {
        Some(card) => rsx! {
            BoardCard { card, projects, examples, on_action }
        },
        None => rsx! {},
    }
}

/// The card core built for the entry's board, when its presence is the
/// entry's kind: a new board is a pending link, an online board a connected
/// one, an offline board a remembered one. `None` when the roster no longer
/// holds it (a view between two emissions), or holds it under another
/// kind.
fn slot_card(entry: &UiHomeBoard, devices: &DeviceRosterView) -> Option<UiBoardCard> {
    let presence = match entry.kind {
        UiHomeBoardKind::Pending => UiBoardPresence::New,
        UiHomeBoardKind::Connected => UiBoardPresence::Online,
        UiHomeBoardKind::Remembered => UiBoardPresence::Offline,
    };
    devices
        .cards
        .iter()
        .find(|card| card.device == entry.id && card.presence == presence)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{
        BoardRef, DeviceEscape, DeviceId, DeviceStatus, DeviceView, OfferPath, RosterCardsInput,
        RosterView, UiOfferTree, build_home_sections, roster_board_cards,
    };

    #[test]
    fn a_key_with_no_matching_card_draws_nothing() {
        let devices = roster(vec![device(1, DeviceStatus::Ready)]);
        for kind in [
            UiHomeBoardKind::Pending,
            UiHomeBoardKind::Connected,
            UiHomeBoardKind::Remembered,
        ] {
            assert_eq!(slot_card(&entry(9, kind), &devices), None, "{kind:?}");
        }
    }

    #[test]
    fn a_board_listed_under_another_kind_draws_nothing() {
        let devices = roster(vec![device(1, DeviceStatus::Ready)]);
        assert_eq!(
            slot_card(&entry(1, UiHomeBoardKind::Remembered), &devices),
            None,
            "an online board is not an offline card"
        );
        assert!(matches!(
            slot_card(&entry(1, UiHomeBoardKind::Connected), &devices),
            Some(card) if card.device == DeviceId(1)
        ));
    }

    #[test]
    fn every_entry_core_lists_finds_its_card() {
        let devices = roster(vec![
            device(1, DeviceStatus::Ready),
            device(2, DeviceStatus::Offline),
        ]);
        let sections = build_home_sections(&[], &devices);
        let entries: Vec<&UiHomeBoard> = sections.online.iter().chain(&sections.offline).collect();
        assert_eq!(entries.len(), 2);
        for entry in entries {
            assert!(slot_card(entry, &devices).is_some(), "{entry:?}");
        }
        assert!(matches!(
            slot_card(&sections.offline[0], &devices),
            Some(card) if card.device == DeviceId(2) && card.presence == UiBoardPresence::Offline
        ));
    }

    fn entry(id: u64, kind: UiHomeBoardKind) -> UiHomeBoard {
        UiHomeBoard {
            id: DeviceId(id),
            kind,
            title: format!("board {id}"),
            status: String::new(),
            project: None,
            row_verbs: Vec::new(),
        }
    }

    /// A roster of `devices`, with the cards core builds for them (each
    /// placed at `devices/new-<id>`).
    fn roster(devices: Vec<DeviceView>) -> DeviceRosterView {
        let mut tree = UiOfferTree::new();
        for device in &devices {
            tree.place_device(
                device.id,
                OfferPath::board(&BoardRef::New(device.id.0 as u32)),
            );
        }
        let mut roster = DeviceRosterView {
            roster: RosterView {
                devices,
                pending: Vec::new(),
            },
            transport_available: true,
            usb_available: true,
            ..DeviceRosterView::default()
        };
        roster.cards = roster_board_cards(&RosterCardsInput {
            roster: &roster,
            offers: &tree,
            projects: &[],
            lens: None,
            now: 0.0,
        });
        roster
    }

    fn device(id: u64, status: DeviceStatus) -> DeviceView {
        DeviceView {
            id: DeviceId(id),
            title: format!("board {id}"),
            status,
            state_label: String::new(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: None,
            firmware_face: lpa_studio_core::DeviceFirmwareFace::Unknown,
            remembered_firmware: None,
            degraded: None,
            loaded_project: lpa_studio_core::DeviceLoadedProject::Unknown,
            engine_fps: None,
            link_counters: None,
            can_receive_project: false,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            escapes: vec![DeviceEscape::Forget],
            update_blocked: None,
            last_update_outcome: None,
        }
    }
}
