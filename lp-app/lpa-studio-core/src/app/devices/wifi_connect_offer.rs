//! `devices/<board>/connect-wifi`: "Connect over Wi‑Fi" on the tile of a
//! board this browser remembers a Wi‑Fi address for and holds no link to.
//!
//! WHEN it is offered is the studio controller's to decide (it holds the
//! address book, the LAN transport and the roster): a remembered board
//! (offline), not a runtime, an address known, a page that reaches the LAN.
//! WHAT it is — its words, its level, its waiting state — is decided here,
//! once, for the controller and for the stories that draw it.

use lpa_devices::DeviceId;

use super::add_device_offers::WIFI_CONNECTING;
use super::wifi_connect_op::WifiConnectOp;
use crate::{OfferPath, UiOffer};

/// The verb under `prefix` (`devices/<board>`), for `device` whose
/// Wi‑Fi address is `ip`; disabled while it is being reached
/// (`connecting`). Routine: it only opens a socket.
pub fn connect_wifi_offer(
    prefix: &OfferPath,
    device: DeviceId,
    ip: &str,
    connecting: bool,
) -> UiOffer {
    let action = WifiConnectOp::action_for(WifiConnectOp::Board {
        device,
        then_relay: false,
    })
    .with_summary(format!(
        "Reach this board on your network, at {ip} (the address it last gave)."
    ));
    wifi_offer(prefix, action, connecting)
}

/// The same verb on a board that has no Connect of its own (one this
/// browser never met, known by the account's list): it is the board's
/// Connect, so a try that fails goes on through lightplayer.app in the same
/// press.
pub fn connect_wifi_first_offer(
    prefix: &OfferPath,
    device: DeviceId,
    ip: &str,
    connecting: bool,
) -> UiOffer {
    let action = WifiConnectOp::action_for(WifiConnectOp::Board {
        device,
        then_relay: true,
    })
    .with_summary(format!(
        "Reach this board on your network, at {ip}; if it doesn't answer there, \
         through lightplayer.app."
    ));
    wifi_offer(prefix, action, connecting)
}

fn wifi_offer(prefix: &OfferPath, action: crate::UiAction, connecting: bool) -> UiOffer {
    UiOffer::new(
        prefix.clone().child("connect-wifi"),
        "wifi",
        match connecting {
            true => action.disabled(WIFI_CONNECTING),
            false => action,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verb_names_the_address_and_waits_while_it_is_reached() {
        let prefix = OfferPath::devices().child("mac-a0f26287b48e");
        let offer = connect_wifi_offer(&prefix, DeviceId(3), "192.168.1.40", false);
        assert_eq!(
            offer.path.to_string(),
            "devices/mac-a0f26287b48e/connect-wifi"
        );
        assert!(offer.is_enabled());
        assert!(offer.consequence().is_routine());
        assert!(offer.summary().contains("192.168.1.40"));
        let waiting = connect_wifi_offer(&prefix, DeviceId(3), "192.168.1.40", true);
        assert_eq!(
            waiting.action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: WIFI_CONNECTING.to_string()
            }
        );
    }

    #[test]
    fn a_boards_first_road_says_it_goes_on_through_lightplayer_app() {
        let prefix = OfferPath::devices().child("mac-a0f26287b48e");
        let offer = connect_wifi_first_offer(&prefix, DeviceId(3), "192.168.1.40:80", false);
        assert_eq!(
            offer.path.to_string(),
            "devices/mac-a0f26287b48e/connect-wifi"
        );
        assert!(offer.summary().contains("192.168.1.40:80"));
        assert!(offer.summary().contains("lightplayer.app"));
        assert_eq!(
            offer.action.op_as::<WifiConnectOp>(),
            Some(&WifiConnectOp::Board {
                device: DeviceId(3),
                then_relay: true
            })
        );
    }
}
