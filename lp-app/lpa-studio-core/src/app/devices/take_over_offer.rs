//! `devices/<board>/take-over`: Connect, on the card of a board another
//! Studio tab of this browser holds.
//!
//! WHEN it is offered is the studio controller's to decide (it holds the
//! asks in flight): a board wearing the fact "another tab holds it"
//! (`DeviceView.held_elsewhere`), not a runtime. While it is offered the
//! plain `connect` is not (`device_offers`): that connect would fight the
//! other tab for the port. WHAT it is — its words, its level, its waiting
//! state — is decided here, once, for the controller and for the stories
//! that draw it.
//!
//! What pressing it costs is what it closes in the other tab, by the level
//! that tab last announced:
//!
//! | level | consequence |
//! |---|---|
//! | `Watching` | `Routine`: nothing of the user's is open there |
//! | `Open` (and a level not said yet, read as `Open`) | `Undoable`: it closes the other tab's open editor; the undo is pressing Connect there |
//! | `Busy(label)` | disabled, "Busy in the other tab: <label>": the holder would refuse |
//!
//! A board held by its network slot is reached, once let go, by the board's
//! ordinary connect over Wi‑Fi or through lightplayer.app; with neither road
//! from this tab it is disabled, "No way to reach it from here".
//!
//! It is never Lasting, and needs no user activation: nothing is lost for
//! good, and no browser chooser opens.

use lpa_devices::{DeviceId, HoldLevel};

use super::take_over_op::TakeOverOp;
use super::take_over_state::TAKE_OVER_NO_WAY;
use crate::{OfferPath, UiOffer};

/// Why the verb waits while the other tab is being asked.
pub const TAKE_OVER_ASKING: &str = "Asking the other tab\u{2026}";

/// Why the verb is disabled while the holder works on the board.
pub fn busy_in_the_other_tab(label: &str) -> String {
    format!("Busy in the other tab: {label}")
}

/// The verb under `prefix` (`devices/<board>`) for `device`, whose holder
/// last said `level`; disabled while the holder is being asked (`asking`),
/// and when this tab could not reach the board once let go (`reachable`
/// false: held by its network slot, with no road from here).
pub fn take_over_offer(
    prefix: &OfferPath,
    device: DeviceId,
    level: &HoldLevel,
    asking: bool,
    reachable: bool,
) -> UiOffer {
    let action = TakeOverOp::action_for(TakeOverOp { device });
    let action = match level {
        HoldLevel::Watching => action,
        HoldLevel::Open => action.undoable(),
        HoldLevel::Busy(label) => action.disabled(busy_in_the_other_tab(label)),
    };
    let action = match (reachable, level) {
        (false, HoldLevel::Watching | HoldLevel::Open) => action.disabled(TAKE_OVER_NO_WAY),
        _ => action,
    };
    let action = match asking {
        true => action.disabled(TAKE_OVER_ASKING),
        false => action,
    };
    UiOffer::new(prefix.clone().child("take-over"), "connect", action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ActionEnablement;

    #[test]
    fn the_verb_is_the_boards_and_costs_what_it_closes_over_there() {
        let prefix = OfferPath::devices().child("mac-a0f26287b48e");

        let watching = take_over_offer(&prefix, DeviceId(3), &HoldLevel::Watching, false, true);
        assert_eq!(
            watching.path.to_string(),
            "devices/mac-a0f26287b48e/take-over"
        );
        assert_eq!(watching.label(), "Connect");
        assert!(watching.is_enabled());
        assert!(watching.consequence().is_routine());

        let open = take_over_offer(&prefix, DeviceId(3), &HoldLevel::Open, false, true);
        assert!(open.is_enabled());
        assert!(!open.consequence().is_routine() && !open.consequence().arms());

        let busy = take_over_offer(
            &prefix,
            DeviceId(3),
            &HoldLevel::Busy("Flashing \u{b7} 40%".to_string()),
            false,
            true,
        );
        assert_eq!(
            busy.action.meta().enablement,
            ActionEnablement::Disabled {
                reason: "Busy in the other tab: Flashing \u{b7} 40%".to_string()
            }
        );
    }

    #[test]
    fn it_waits_while_the_other_tab_is_asked_and_never_needs_the_user() {
        let prefix = OfferPath::devices().child("mac-a0f26287b48e");
        for level in [
            HoldLevel::Watching,
            HoldLevel::Open,
            HoldLevel::Busy("Pushing".to_string()),
        ] {
            let offer = take_over_offer(&prefix, DeviceId(3), &level, false, true);
            assert!(!offer.consequence().arms(), "{level:?}: never Lasting");
            assert!(!offer.action.meta().needs_user_activation, "{level:?}");
        }
        let asking = take_over_offer(&prefix, DeviceId(3), &HoldLevel::Watching, true, true);
        assert_eq!(
            asking.action.meta().enablement,
            ActionEnablement::Disabled {
                reason: TAKE_OVER_ASKING.to_string()
            }
        );
    }

    #[test]
    fn a_board_this_tab_has_no_road_to_says_so() {
        let prefix = OfferPath::devices().child("mac-a0f26287b48e");
        let offer = take_over_offer(&prefix, DeviceId(3), &HoldLevel::Open, false, false);
        assert_eq!(
            offer.action.meta().enablement,
            ActionEnablement::Disabled {
                reason: "No way to reach it from here".to_string()
            }
        );
        // A busy holder's reason comes first: it would refuse anyway.
        let busy = take_over_offer(
            &prefix,
            DeviceId(3),
            &HoldLevel::Busy("Pushing".to_string()),
            false,
            false,
        );
        assert_eq!(
            busy.action.meta().enablement,
            ActionEnablement::Disabled {
                reason: "Busy in the other tab: Pushing".to_string()
            }
        );
    }
}
