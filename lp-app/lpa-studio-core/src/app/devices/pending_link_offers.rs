//! Every verb a pending link's card offers — a port Studio is still
//! identifying, or one that settled on a face but has not been kept — under
//! the link's board ref, usually `devices/new-<n>/…`.
//!
//! The card's own conditions, verb for verb:
//!
//! | path verb | offered when |
//! |---|---|
//! | `flash` | the link settled on a needs-firmware verdict ([`flash_pending_offer`]); pressing it adopts the link |
//! | `adopt` | "Set up this device", where Flash is not already that gesture |
//! | `reset-board` | always: the recovery for a chip parked silent in its ROM downloader; disabled over Bluetooth |
//! | `dismiss` | the projection's escape (it says Forget; dismissing hands the grant back), Lasting through its meta |
//!
//! A pending link is not a device, so its adopt and dismiss address the
//! LINK; its flash and reset address the provisional device the link was
//! minted with (adoption keeps the id).

use lpa_devices::Action;
use lpa_devices::view::PendingLinkView;

use super::device_affordance::pending_escape_action;
use super::device_flash::RESET_NEEDS_USB;
use super::device_flash_offer::flash_pending_offer;
use super::devices_op::DevicesOp;
use crate::{OfferPath, UiOffer};

/// Every offer `pending`'s card makes, under `prefix` (`devices/<board>`).
pub fn pending_link_offers(pending: &PendingLinkView, prefix: &OfferPath) -> Vec<UiOffer> {
    let at = |verb: &str| prefix.clone().child(verb);
    let mut offers = Vec::new();
    if let Some(flash) = flash_pending_offer(pending, prefix.clone()) {
        offers.push(flash);
    }
    if pending.can_adopt && !pending.needs_firmware() {
        offers.push(UiOffer::new(
            at("adopt"),
            "add",
            DevicesOp::action_for(Action::AdoptLink { link: pending.link }),
        ));
    }
    let reset = DevicesOp::action_for(Action::ResetBoard {
        device: pending.device,
    });
    offers.push(UiOffer::new(
        at("reset-board"),
        "reset",
        match pending.is_over_bluetooth() {
            true => reset.disabled(RESET_NEEDS_USB),
            false => reset,
        },
    ));
    // Every escape the projection grants a pending link dismisses it; one
    // verb, however many it names.
    if let Some(escape) = pending.escapes.first() {
        offers.push(UiOffer::new(
            at("dismiss"),
            "remove",
            pending_escape_action(*escape, pending.link),
        ));
    }
    offers
}

#[cfg(test)]
mod tests {
    use lpa_devices::view::{Escape, FirmwareFace};
    use lpa_devices::{DeviceId, LinkId};

    use super::*;

    #[test]
    fn a_blank_chip_flashes_resets_and_dismisses_but_does_not_adopt_twice() {
        let offers = pending_link_offers(&pending(FirmwareFace::Blank), &prefix());
        assert_eq!(
            paths(&offers),
            [
                "devices/new-3/flash",
                "devices/new-3/reset-board",
                "devices/new-3/dismiss"
            ],
            "on a needs-firmware verdict Flash IS the adopting gesture"
        );
        assert!(
            offers[0].consequence().is_routine(),
            "a blank chip loses nothing"
        );
        assert!(
            offers[2].consequence().arms(),
            "dismiss hands the grant back"
        );
        assert_eq!(
            offers[2].action.op_as::<DevicesOp>().unwrap().action(),
            &Action::DismissLink { link: LinkId(4) }
        );
    }

    #[test]
    fn an_identifying_link_can_be_kept_reset_or_dismissed() {
        let offers = pending_link_offers(&pending(FirmwareFace::Unknown), &prefix());
        assert_eq!(
            paths(&offers),
            [
                "devices/new-3/adopt",
                "devices/new-3/reset-board",
                "devices/new-3/dismiss"
            ]
        );
        assert_eq!(offers[0].label(), "Set up this device");
        assert_eq!(
            offers[0].action.op_as::<DevicesOp>().unwrap().action(),
            &Action::AdoptLink { link: LinkId(4) }
        );
    }

    #[test]
    fn over_bluetooth_reset_is_drawn_disabled() {
        let mut link = pending(FirmwareFace::Unknown);
        link.firmware_blocked = Some(lpa_devices::view::FIRMWARE_NEEDS_USB.to_string());
        let offers = pending_link_offers(&link, &prefix());
        let reset = offers
            .iter()
            .find(|offer| offer.path.last() == Some("reset-board"))
            .unwrap();
        assert!(!reset.is_enabled());
    }

    fn paths(offers: &[UiOffer]) -> Vec<String> {
        offers.iter().map(|offer| offer.path.to_string()).collect()
    }

    fn prefix() -> OfferPath {
        OfferPath::board(&crate::BoardRef::New(3))
    }

    fn pending(face: FirmwareFace) -> PendingLinkView {
        PendingLinkView {
            link: LinkId(4),
            device: DeviceId(3),
            title: "New device".to_string(),
            state_label: "New device found".to_string(),
            detail: None,
            can_adopt: true,
            firmware_face: face,
            detected_chip: Some("esp32c6".to_string()),
            mac: None,
            firmware_blocked: None,
            escapes: vec![Escape::Forget],
        }
    }
}
