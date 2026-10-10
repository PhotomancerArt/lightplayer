//! `devices/<board>/done`: close the session open on this board.
//!
//! Done is how a connected board's card goes back to its facts (the ADR's
//! §4: "Done closes the session, and the card shows its facts again"), and
//! how the editor's docked card ends the session it docks. It is
//! `RuntimeOp::CloseDeviceLens`, the one road a session ends by: the
//! mirror drops, the pool's session goes, and the board's wire goes back to
//! the roster. Nothing about the board changes.
//!
//! WHEN it is offered is the studio controller's to decide (it holds the
//! pool): on the board an attached session is on, whether the home page
//! holds it (Connect, Edit from a card) or an address opened it. WHAT it is
//! is decided here.

use crate::{OfferPath, RuntimeOp, UiAction, UiOffer};

/// The verb's path segment.
pub const DONE_VERB: &str = "done";

/// `devices/<board>/done` under `prefix`: Routine, one click — the session
/// can be opened again by Connect, and nothing is lost on the board.
pub fn device_done_offer(prefix: &OfferPath) -> UiOffer {
    UiOffer::new(
        prefix.clone().child(DONE_VERB),
        "check",
        UiAction::from_op(RuntimeOp::NODE_ID, RuntimeOp::CloseDeviceLens)
            .with_label("Done")
            .with_summary("Close this board's controls. The board keeps running.")
            .with_icon("check"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn done_closes_the_session_in_one_routine_click() {
        let prefix = OfferPath::board(&crate::BoardRef::Mac(
            lpa_devices::BoardKey::parse("a0:f2:62:87:b4:8c").unwrap(),
        ));
        let offer = device_done_offer(&prefix);
        assert_eq!(offer.path.to_string(), "devices/mac-a0f26287b48c/done");
        assert_eq!(offer.label(), "Done");
        assert_eq!(offer.icon, "check");
        assert!(offer.is_enabled());
        assert!(offer.consequence().is_routine());
        assert_eq!(
            offer.action.op_as::<RuntimeOp>(),
            Some(&RuntimeOp::CloseDeviceLens)
        );
    }
}
