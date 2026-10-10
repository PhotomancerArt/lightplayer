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
//! | `reset-board` | the recovery for a chip parked silent in its ROM downloader; disabled on a network link (Bluetooth, Wi‑Fi, the relay), whose Reset is a request to a board that has not answered yet; not on a port another tab holds (its lines are that tab's) |
//! | `dismiss` | the projection's escape (it says Forget; dismissing hands the grant back), Lasting through its meta; not on a port another tab holds (handing the grant back can pull the port from that tab) |
//!
//! A pending link is not a device, so its adopt and dismiss address the
//! LINK; its flash and reset address the provisional device the link was
//! minted with (adoption keeps the id).

use lpa_devices::Action;
use lpa_devices::view::PendingLinkView;

use super::device_affordance::pending_escape_action;
use super::device_flash_offer::flash_pending_offer;
use super::device_reset_reach::{RESET_WAITS_FOR_ANSWER, ResetReach};
use super::devices_op::DevicesOp;
use crate::{OfferPath, UiOffer};

/// Every offer `pending`'s card makes, under `prefix` (`devices/<board>`).
/// `reset`: how the link would restart the board ([`ResetReach`]).
pub fn pending_link_offers(
    pending: &PendingLinkView,
    prefix: &OfferPath,
    reset: ResetReach,
) -> Vec<UiOffer> {
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
    // Over a cable, Reset pulses the reset lines — the way out of a chip
    // that prints nothing. Over a network link it would be a restart
    // request to a board that has not said hello yet (and whose password
    // nobody has checked), so it waits for the answer. A port another tab
    // holds is that tab's, lines and all: no Reset here.
    if !pending.held_by_tab {
        let action = DevicesOp::action_for(Action::ResetBoard {
            device: pending.device,
        });
        offers.push(UiOffer::new(
            at("reset-board"),
            "reset",
            match reset {
                ResetReach::Lines => action,
                ResetReach::Request { .. } => action.disabled(RESET_WAITS_FOR_ANSWER),
            },
        ));
    }
    // Every escape the projection grants a pending link dismisses it; one
    // verb, however many it names. Not a port another tab holds: dismissing
    // hands the site's grant back (`port.forget()`), which can pull the
    // port out from under the tab that holds it.
    if let Some(escape) = pending.escapes.first().filter(|_| !pending.held_by_tab) {
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
        let offers =
            pending_link_offers(&pending(FirmwareFace::Blank), &prefix(), ResetReach::Lines);
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
        let offers = pending_link_offers(
            &pending(FirmwareFace::Unknown),
            &prefix(),
            ResetReach::Lines,
        );
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

    /// A network link (Bluetooth, Wi‑Fi, the relay) still identifying has
    /// no reset lines, and its Reset would ask a board that has not answered:
    /// drawn disabled, saying it waits for the answer — never "needs USB".
    #[test]
    fn over_a_network_link_reset_waits_for_the_board_to_answer() {
        let offers = pending_link_offers(
            &pending(FirmwareFace::Unknown),
            &prefix(),
            ResetReach::Request { author: false },
        );
        let reset = offers
            .iter()
            .find(|offer| offer.path.last() == Some("reset-board"))
            .unwrap();
        assert_eq!(
            reset.action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: RESET_WAITS_FOR_ANSWER.to_string()
            }
        );
    }

    /// A port another tab holds offers no dismiss (dismissing hands the
    /// site's grant back, which can pull the port from that tab) and no
    /// Reset (its lines are that tab's).
    #[test]
    fn a_port_another_tab_holds_cannot_be_dismissed_or_reset() {
        let mut held = pending(FirmwareFace::Unknown);
        held.held_by_tab = true;

        let offers = pending_link_offers(&held, &prefix(), ResetReach::Lines);

        for verb in ["/dismiss", "/reset-board"] {
            assert!(
                !paths(&offers).iter().any(|path| path.ends_with(verb)),
                "{verb}: {:?}",
                paths(&offers)
            );
        }
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
            held_by_tab: false,
            escapes: vec![Escape::Forget],
        }
    }
}
