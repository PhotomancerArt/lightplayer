//! `devices/<board>/sign-in`: "Sign in" on a board card's lightplayer.app
//! row while nobody is signed in — the row's fix, and a new board's to-do.
//!
//! WHEN it is offered is the studio controller's to decide (it holds the
//! relay transport and the account's keys): a board that is not a runtime,
//! on a page that can reach lightplayer.app, while no account's keys are
//! held. WHAT it is is decided here ([`super::SignInOp`]).

use super::sign_in_op::SignInOp;
use crate::{OfferPath, UiOffer};

/// The verb's path segment.
pub const SIGN_IN_VERB: &str = "sign-in";

/// The verb under `prefix` (`devices/<board>`).
pub fn sign_in_offer(prefix: &OfferPath) -> UiOffer {
    UiOffer::new(
        prefix.clone().child(SIGN_IN_VERB),
        "login",
        SignInOp::action(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Routine to the board, but the person's own click: it leaves the
    /// page for their account.
    #[test]
    fn the_verb_is_the_boards_and_needs_the_persons_click() {
        let prefix = OfferPath::devices().child("mac-a0f26287b48e");
        let offer = sign_in_offer(&prefix);
        assert_eq!(offer.path.to_string(), "devices/mac-a0f26287b48e/sign-in");
        assert_eq!(offer.label(), "Sign in");
        assert!(offer.consequence().is_routine());
        assert!(offer.action.meta().needs_user_activation);
        assert!(offer.is_enabled());
    }
}
