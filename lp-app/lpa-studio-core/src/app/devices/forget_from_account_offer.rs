//! `devices/<board>/forget-from-account`: "Remove from your account", on a
//! board the signed-in account's list holds (D10). Lasting: two clicks,
//! and the agent never presses it — it becomes the user's own button in
//! chat. Not offered signed out, to a guest (a guest has no list), nor on
//! a board the list does not hold.
//!
//! Beside it, the card's `forget` reads **Forget on this browser**, and on
//! an account board its copy says the board still shows from the account
//! ([`forget_on_this_browser_copy`]).

use lpa_devices::BoardKey;

use super::forget_from_account_op::ForgetFromAccountOp;
use crate::{ActionConfirmation, OfferPath, UiOffer};

/// The verb under a board's prefix.
pub const FORGET_FROM_ACCOUNT_VERB: &str = "forget-from-account";

/// The offer for `board`, called `title` on its card.
pub fn forget_from_account_offer(prefix: &OfferPath, board: BoardKey, title: &str) -> UiOffer {
    UiOffer::new(
        prefix.clone().child(FORGET_FROM_ACCOUNT_VERB),
        "remove",
        ForgetFromAccountOp::action_for(ForgetFromAccountOp {
            board,
            title: title.to_string(),
        }),
    )
}

/// The card's `forget` on a board the account's list holds: its words say
/// it is this browser's only, and that the board still shows from the
/// account. What it does does not change.
pub fn forget_on_this_browser_copy(mut forget: UiOffer) -> UiOffer {
    let still = "It still shows from your account on every computer; Remove from your \
                 account takes it off there.";
    let summary = format!("{} {still}", forget.summary());
    let confirmation = match forget.consequence() {
        crate::ActionConsequence::Lasting(copy) => ActionConfirmation::new(
            copy.title.clone(),
            format!("{} {still}", copy.message),
            copy.confirm_label.clone(),
        ),
        _ => ActionConfirmation::new("Forget this device?", summary.clone(), "Forget"),
    };
    forget.action = forget.action.with_summary(summary).lasting(confirmation);
    forget
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verb_is_the_boards_and_lasting() {
        let prefix = OfferPath::devices().child("mac-10bda3b08e30");
        let offer =
            forget_from_account_offer(&prefix, BoardKey::parse("10bda3b08e30").unwrap(), "Porch");
        assert_eq!(
            offer.path.to_string(),
            "devices/mac-10bda3b08e30/forget-from-account"
        );
        assert_eq!(offer.label(), "Remove from your account");
        assert!(offer.consequence().arms());
    }
}
