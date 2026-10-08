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
/// remembered address is `ip`; disabled while it is being reached
/// (`connecting`). Routine: it only opens a socket.
pub fn connect_wifi_offer(
    prefix: &OfferPath,
    device: DeviceId,
    ip: &str,
    connecting: bool,
) -> UiOffer {
    let action = WifiConnectOp::action_for(WifiConnectOp::Board { device }).with_summary(format!(
        "Reach this board on your network, at {ip} (the address it last gave)."
    ));
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
}
