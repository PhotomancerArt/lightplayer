//! `devices/<board>/connect-relay`: "Connect through lightplayer.app" on the
//! tile of a board Studio has met and holds no link to.
//!
//! WHEN it is offered is the studio controller's to decide (it holds the
//! relay transport, the account's keys and the roster): a remembered board
//! (offline) that has said its MAC, not a runtime, while someone is signed
//! in. WHAT it is — its words, its level, its waiting state — is decided
//! here, once, for the controller and for the stories that draw it.
//!
//! It is the least way in through the relay (the network transport's PR C):
//! no list of the account's boards, no sweep, no automatic move onto the
//! LAN. The board must already be one Studio met over USB, Bluetooth or
//! Wi‑Fi, and it must hold the account's key (plugged in once, signed in).

use lpa_devices::DeviceId;

use super::relay_connect_op::RelayConnectOp;
use crate::{OfferPath, UiOffer};

/// Why the verb waits while it is being reached.
pub const RELAY_CONNECTING: &str = "Connecting through lightplayer.app\u{2026}";

/// The verb under `prefix` (`devices/<board>`) for `device`; disabled while
/// it is being reached (`connecting`). Routine: it only opens a socket.
pub fn connect_relay_offer(prefix: &OfferPath, device: DeviceId, connecting: bool) -> UiOffer {
    let action = RelayConnectOp::action_for(RelayConnectOp { device });
    UiOffer::new(
        prefix.clone().child("connect-relay"),
        "wifi",
        match connecting {
            true => action.disabled(RELAY_CONNECTING),
            false => action,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verb_is_the_boards_and_waits_while_it_is_reached() {
        let prefix = OfferPath::devices().child("mac-a0f26287b48e");
        let offer = connect_relay_offer(&prefix, DeviceId(3), false);
        assert_eq!(
            offer.path.to_string(),
            "devices/mac-a0f26287b48e/connect-relay"
        );
        assert!(offer.is_enabled());
        assert!(offer.consequence().is_routine());
        assert_eq!(offer.label(), "Connect through lightplayer.app");
        let waiting = connect_relay_offer(&prefix, DeviceId(3), true);
        assert_eq!(
            waiting.action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: RELAY_CONNECTING.to_string()
            }
        );
    }
}
