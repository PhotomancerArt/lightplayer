//! The one place a board's card is mounted on the home page (PD3).
//!
//! Online boards and Offline boards draw every board through
//! [`BoardCardSlot`], keyed by the section entry core listed
//! ([`UiHomeBoard`]). Today the slot hosts the cards the Devices page drew:
//! the pending link's card, the roster card and the offline tile. The board
//! card (M2) replaces this body, for all three kinds, and touches nothing
//! else on the page.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceRosterView, DeviceView, PendingLinkView, RememberedView, UiAction, UiExampleCard,
    UiHomeBoard, UiHomeBoardKind, UiPackageCard, split_roster,
};

use super::offline_board_tile::OfflineBoardTile;
use crate::app::home::device_roster_card::{DeviceRosterCard, PendingLinkCard};

/// A board's card, by the kind of entry core listed. A key with no matching
/// view in the roster draws nothing.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BoardCardSlot(
    /// The section's entry: which board, and where it stands.
    entry: UiHomeBoard,
    /// The roster and the side maps the card reads (feeds, bands, access,
    /// Wi‑Fi, LAN, layout, updates, editor addresses).
    devices: DeviceRosterView,
    /// The empty face's picker reads the page's own lists: there is no
    /// separate device-side project source.
    projects: Vec<UiPackageCard>,
    examples: Vec<UiExampleCard>,
    on_action: EventHandler<UiAction>,
) -> Element {
    match slot_view(&entry, &devices) {
        Some(SlotView::Pending(pending)) => rsx! {
            PendingLinkCard { pending, on_action }
        },
        Some(SlotView::Connected(card)) => {
            let id = card.id;
            rsx! {
                DeviceRosterCard {
                    // The running face's Open needs the device's editor
                    // address (its registry uid); a board still
                    // identifying has none.
                    open_uid: devices.open_addresses.get(&id.0).cloned(),
                    // The board's own picture, joined at the app view;
                    // absent = the slot's sentence.
                    feed: devices.feeds.get(&id).cloned(),
                    // The runtime band, for a device that is not silicon;
                    // absent = a real board.
                    runtime: devices.runtime_bands.get(&id).cloned(),
                    // Its login line and access panel (BLE M6).
                    access: devices.access.get(&id).cloned(),
                    // Its Wi‑Fi row (Wi‑Fi roadmap M5).
                    wifi: devices.wifi.get(&id).cloned(),
                    // How a board on the LAN is reached (Wi-Fi M6 P07).
                    lan: devices.lan_links.get(&id).cloned(),
                    // Its files across a layout change (the C6
                    // repartition).
                    layout: devices.layout.get(&id).cloned(),
                    // Its firmware-update words (direction C).
                    update: devices.updates.get(&id).cloned(),
                    card: *card,
                    projects,
                    examples,
                    on_action,
                }
            }
        }
        Some(SlotView::Remembered(remembered)) => rsx! {
            OfflineBoardTile { entry: remembered, on_action }
        },
        None => rsx! {},
    }
}

/// The roster view a section entry names, by its kind.
#[derive(Debug, PartialEq)]
enum SlotView {
    Pending(PendingLinkView),
    Connected(Box<DeviceView>),
    Remembered(RememberedView),
}

/// Find the entry's view: a pending link by its device handle, a connected
/// board in the roster, a remembered board in the roster's own split. `None`
/// when the roster no longer holds it (a view between two emissions), or
/// holds it under another kind.
fn slot_view(entry: &UiHomeBoard, devices: &DeviceRosterView) -> Option<SlotView> {
    match entry.kind {
        UiHomeBoardKind::Pending => devices
            .roster
            .pending
            .iter()
            .find(|pending| pending.device == entry.id)
            .cloned()
            .map(SlotView::Pending),
        UiHomeBoardKind::Connected => split_roster(devices)
            .connected
            .into_iter()
            .find(|card| card.id == entry.id)
            .map(|card| SlotView::Connected(Box::new(card))),
        UiHomeBoardKind::Remembered => split_roster(devices)
            .remembered
            .into_iter()
            .find(|remembered| remembered.id == entry.id)
            .map(SlotView::Remembered),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{
        DeviceEscape, DeviceId, DeviceStatus, RosterView, UiHomeBoard, build_home_sections,
    };

    #[test]
    fn a_key_with_no_matching_view_draws_nothing() {
        let devices = roster(vec![device(1, DeviceStatus::Ready)]);
        for kind in [
            UiHomeBoardKind::Pending,
            UiHomeBoardKind::Connected,
            UiHomeBoardKind::Remembered,
        ] {
            assert_eq!(slot_view(&entry(9, kind), &devices), None, "{kind:?}");
        }
    }

    #[test]
    fn a_board_listed_under_another_kind_draws_nothing() {
        let devices = roster(vec![device(1, DeviceStatus::Ready)]);
        assert_eq!(
            slot_view(&entry(1, UiHomeBoardKind::Remembered), &devices),
            None,
            "a connected board is not an offline tile"
        );
        assert!(matches!(
            slot_view(&entry(1, UiHomeBoardKind::Connected), &devices),
            Some(SlotView::Connected(card)) if card.id == DeviceId(1)
        ));
    }

    #[test]
    fn every_entry_core_lists_finds_its_view() {
        let devices = roster(vec![
            device(1, DeviceStatus::Ready),
            device(2, DeviceStatus::Offline),
        ]);
        let sections = build_home_sections(&[], &devices);
        let entries: Vec<&UiHomeBoard> = sections.online.iter().chain(&sections.offline).collect();
        assert_eq!(entries.len(), 2);
        for entry in entries {
            assert!(slot_view(entry, &devices).is_some(), "{entry:?}");
        }
        assert!(matches!(
            slot_view(&sections.offline[0], &devices),
            Some(SlotView::Remembered(remembered)) if remembered.id == DeviceId(2)
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

    fn roster(devices: Vec<DeviceView>) -> DeviceRosterView {
        DeviceRosterView {
            roster: RosterView {
                devices,
                pending: Vec::new(),
            },
            transport_available: true,
            usb_available: true,
            ..DeviceRosterView::default()
        }
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
