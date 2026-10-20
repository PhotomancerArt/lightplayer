//! When a radio link opens: at subscribe, or a moment later for the MTU.
//!
//! A radio link's frames are sized once, from the connection's ATT MTU when
//! the central enables notifications ([`super::radio_link_config`]), and the
//! session keeps that size for its life. Mac Chrome and Bluefy exchange the
//! MTU before they subscribe. Android Chrome, reconnecting to a board whose
//! GATT table it has cached, can write the CCCD first and exchange the MTU
//! after, so a link opened at once would be sized for the Bluetooth minimum
//! (12-byte payloads) and stay there
//! (docs/defects/2026-10-17-an-android-reconnect-opens-the-ble-link-at-att-mtu-23.md).
//!
//! So a central that subscribes while the connection is still at 23 gets a
//! short grace: the board polls the MTU and opens the link the moment it
//! rises, or at the end of [`MTU_GRACE_US`] at whatever it is. The host stack
//! answers the exchange itself and raises no event for it, hence the poll.
//! Nothing is lost by waiting: a frame the central writes before the link
//! opens is dropped, and the board's own SYN at open starts the handshake.

use lp_link::Micros;

/// The ATT MTU every connection starts at, and keeps until an exchange.
pub const BLE_MIN_ATT_MTU: u16 = 23;

/// How long a central that subscribed at [`BLE_MIN_ATT_MTU`] has to raise the
/// MTU before its link opens anyway.
///
/// The desk's Pixel 8a (Android 14, Chrome 155) exchanged it ~1 s after its
/// CCCD write, behind its own connection-parameter request (2026-10-17), so
/// twice that. Studio gives a Bluetooth link 5 s from GATT open to the
/// board's hello (`lpa_link`'s `DEFAULT_READY_DEADLINE`), which the grace, the
/// handshake and the hello fit inside even for a central that never
/// exchanges.
pub const MTU_GRACE_US: Micros = 2_000_000;

/// How often the board looks at the MTU during the grace.
pub const MTU_POLL_US: Micros = 50_000;

/// What to do with a subscribed connection whose link has not opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MtuGraceDecision {
    /// Open the link now, at the connection's current ATT MTU.
    Open,
    /// Look at the MTU again at this time.
    PollAt(Micros),
}

/// Whether a connection that subscribed at `subscribed_at` opens its link now,
/// at `now`, with its ATT MTU at `att_mtu`.
pub fn mtu_grace_decision(att_mtu: u16, subscribed_at: Micros, now: Micros) -> MtuGraceDecision {
    let deadline = subscribed_at.saturating_add(MTU_GRACE_US);
    if att_mtu > BLE_MIN_ATT_MTU || now >= deadline {
        MtuGraceDecision::Open
    } else {
        MtuGraceDecision::PollAt(now.saturating_add(MTU_POLL_US).min(deadline))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_central_that_exchanged_the_mtu_first_opens_at_once() {
        for mtu in [24u16, 185, 247, 517] {
            assert_eq!(
                mtu_grace_decision(mtu, 7_000, 7_000),
                MtuGraceDecision::Open
            );
        }
    }

    #[test]
    fn a_central_at_the_minimum_waits_and_opens_when_the_mtu_rises() {
        let at = 5_000_000;
        assert_eq!(
            mtu_grace_decision(23, at, at),
            MtuGraceDecision::PollAt(at + MTU_POLL_US)
        );
        assert_eq!(
            mtu_grace_decision(23, at, at + 400_000),
            MtuGraceDecision::PollAt(at + 400_000 + MTU_POLL_US)
        );
        // Android's exchange lands a moment after its CCCD write.
        assert_eq!(
            mtu_grace_decision(247, at, at + 450_000),
            MtuGraceDecision::Open
        );
    }

    #[test]
    fn a_central_that_never_exchanges_opens_at_the_minimum_when_the_grace_ends() {
        let at = 5_000_000;
        assert_eq!(
            mtu_grace_decision(23, at, at + MTU_GRACE_US - 10_000),
            MtuGraceDecision::PollAt(at + MTU_GRACE_US),
            "the last poll lands on the deadline, not past it"
        );
        assert_eq!(
            mtu_grace_decision(23, at, at + MTU_GRACE_US),
            MtuGraceDecision::Open
        );
        assert_eq!(
            mtu_grace_decision(23, at, at + 3 * MTU_GRACE_US),
            MtuGraceDecision::Open
        );
    }

    #[test]
    fn the_grace_is_far_inside_the_subscribe_and_login_deadline() {
        assert!(MTU_GRACE_US * 5 <= super::super::LOGIN_DEADLINE_MS * 1_000);
    }

    #[test]
    fn the_grace_leaves_studio_room_for_the_handshake_and_hello() {
        // lpa_link's DEFAULT_READY_DEADLINE (an app crate firmware cannot
        // depend on): 5 s from GATT open to the board's hello.
        const STUDIO_READY_DEADLINE_US: Micros = 5_000_000;
        assert!(MTU_GRACE_US * 2 <= STUDIO_READY_DEADLINE_US);
    }
}
