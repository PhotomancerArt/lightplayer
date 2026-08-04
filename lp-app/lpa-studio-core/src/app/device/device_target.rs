//! Which device an operation addresses.

/// The board an operation acts on (multi-board M4).
///
/// One vocabulary: the CARD KEY every UI surface already holds
/// ([`crate::UiDeviceCard::identity_key`]). It is a stamped device's
/// `dev_…` uid, or an anonymous board's session key — and one resolver
/// handles both, because `identity_key` puts the uid first, so a live
/// stamped card's key IS its uid.
///
/// Before this existed, device ops resolved "the" device: the OLDEST
/// attached board, whichever card the user had clicked. With one board
/// that was correct by accident. With two it flashed the wrong board.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceTarget {
    /// The card the gesture came from.
    Card(String),
    /// The ops with no card behind them — today exactly one:
    /// `ConnectLightPlayer`, whose documented fallback is the lens
    /// session (the sim's reconnect). Resolves to the lens session when
    /// the lens is on a device, else the oldest device session.
    ///
    /// ⚠️ CLOSED SET. This arm is the one place a device op can still
    /// land on a board nobody named, so every op added here re-opens the
    /// hole M4 closed. A new op takes a card key.
    ///
    /// (The console's device log-level selector looks like a member and
    /// is not: it targets the LENS session directly — whichever runtime's
    /// console the user is reading, sim included — so it carries no
    /// target at all.)
    Ambient,
}

impl DeviceTarget {
    /// The target for a gesture that came from `card_key` — the card's
    /// [`crate::UiDeviceCard::identity_key`].
    pub fn card(card_key: impl Into<String>) -> Self {
        Self::Card(card_key.into())
    }

    /// The card key this targets, when it names one.
    pub fn card_key(&self) -> Option<&str> {
        match self {
            Self::Card(key) => Some(key),
            Self::Ambient => None,
        }
    }
}
