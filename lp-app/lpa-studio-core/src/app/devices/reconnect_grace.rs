//! A board that is rebooting holds its place on the home page.
//!
//! A flash, a Reset, turning Bluetooth on: each restarts the board, and its
//! link goes for a second or two before the board comes back on a fresh
//! one. Read straight off the model that board is Offline in between, so
//! its card left Online boards for Offline boards and came back (the
//! stable-board-order fix, 2026-10-28). For [`RECONNECT_GRACE_MS`] after a
//! link goes by itself the board is Reconnecting instead: its card stays in
//! Online boards, in its place, and its surface says "Reconnecting to this
//! board…" (the card's own Reconnecting, [`BoardConnection::Reconnecting`]).
//! Past the grace it is Offline, as before.
//!
//! The user's own Disconnect is never a reconnect (the model gives that
//! link no detached age), and neither is a stand-in powered off: a sim this
//! tab stopped is off, not rebooting. A board something else already
//! accounts for keeps that word: another tab holds it or is handing it
//! over, lightplayer.app
//! says it is online, or a Wi‑Fi or lightplayer.app connect is under way
//! or failed on it.

use lpa_devices::DeviceView;
use lpa_devices::device::DeviceStatus;

use super::board_card::BoardConnection;
use super::device_roster::DeviceRosterView;

/// How long a board whose link went by itself counts as reconnecting. A C6
/// re-enumerates on USB in a second or two and a Wi‑Fi board rejoins in a
/// few; past this a board that has not come back is Offline.
pub const RECONNECT_GRACE_MS: u64 = 10_000;

/// Whether `device` is offline only because its link went a moment ago.
pub fn in_reconnect_grace(device: &DeviceView) -> bool {
    device.status == DeviceStatus::Offline
        && device
            .detached_age_ms
            .is_some_and(|age| age < RECONNECT_GRACE_MS)
}

/// Mark every board in its grace as [`BoardConnection::Reconnecting`] in
/// `view.connections`, where the cards and the home page's sections read
/// it. A board the tab's session already speaks for keeps that word, and a
/// board something else accounts for (the module doc) is left alone.
pub fn join_reconnect_grace(view: &mut DeviceRosterView) {
    for device in &view.roster.devices {
        let accounted = view.runtime_bands.contains_key(&device.id)
            || device.held_elsewhere.is_some()
            || view.take_overs.contains_key(&device.id)
            || view.wifi_connects.contains_key(&device.id)
            || view
                .cloud_boards
                .get(&device.id)
                .is_some_and(|cloud| cloud.online);
        if !in_reconnect_grace(device) || accounted {
            continue;
        }
        view.connections
            .entry(device.id)
            .or_insert(BoardConnection::Reconnecting);
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::DeviceId;

    use super::*;
    use crate::app::devices::board_card::card_fixtures::CardFixture;

    #[test]
    fn a_board_whose_link_just_went_is_reconnecting_until_the_grace_ends() {
        let mut view = CardFixture::offline().view;
        view.detached_age_ms = Some(2_000);
        assert!(in_reconnect_grace(&view));
        view.detached_age_ms = Some(RECONNECT_GRACE_MS);
        assert!(!in_reconnect_grace(&view), "past the grace it is offline");
    }

    /// No detached age: a board loaded cold from its record, or one the
    /// user disconnected. Neither is coming back by itself.
    #[test]
    fn a_board_with_no_detached_age_is_offline() {
        let view = CardFixture::offline().view;
        assert_eq!(view.detached_age_ms, None);
        assert!(!in_reconnect_grace(&view));
    }

    #[test]
    fn an_online_board_is_never_in_grace() {
        let mut view = CardFixture::ready().view;
        view.detached_age_ms = Some(1);
        assert!(!in_reconnect_grace(&view));
    }

    #[test]
    fn the_grace_joins_the_connections_and_spares_a_stand_in_and_a_session() {
        let rebooting = |id: u64| {
            let mut view = CardFixture::offline().view;
            view.id = DeviceId(id);
            view.detached_age_ms = Some(500);
            view
        };
        let mut roster = DeviceRosterView::default();
        roster.roster.devices = vec![rebooting(1), rebooting(2), rebooting(3)];
        roster.runtime_bands.insert(
            DeviceId(2),
            crate::app::devices::runtime_band::UiRuntimeBand {
                kind: "Sim",
                target: "Desktop".to_string(),
                locality: "in this tab",
                tier: None,
                speed: None,
            },
        );
        roster.connections.insert(
            DeviceId(3),
            BoardConnection::Failed {
                reason: "no".to_string(),
            },
        );

        join_reconnect_grace(&mut roster);

        assert_eq!(
            roster.connections.get(&DeviceId(1)),
            Some(&BoardConnection::Reconnecting)
        );
        assert_eq!(roster.connections.get(&DeviceId(2)), None, "a sim is off");
        assert!(matches!(
            roster.connections.get(&DeviceId(3)),
            Some(BoardConnection::Failed { .. })
        ));
    }
}
