//! [`OfferPress`]: which offer an action was pressed from, and with what
//! values.

use crate::{OfferArgs, OfferPath};

/// Where a pressed action came from: the offer's path and the values the
/// press carried (as handed over, before defaults were filled).
///
/// [`crate::UiOffer::press`] stamps it on the action it returns
/// ([`crate::UiAction::offer_press`]). It is provenance, not identity: two
/// actions that do the same thing are equal wherever they were pressed
/// from. What reads it is the app agent's card: a card that handed the
/// user an offer is answered by any press of that offer, whatever values
/// the user settled on — those values are part of what the agent hears.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfferPress {
    /// The offer that was pressed.
    pub path: OfferPath,
    /// The values the press carried.
    pub args: OfferArgs,
}
