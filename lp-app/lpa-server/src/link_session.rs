//! What the server remembers about one link.

use lpc_access::{OpenTo, Tier};
use lpc_shared::transport::LinkTrust;

/// Per-link server state, created the first time a link is seen and
/// dropped when its transport reports it closed.
///
/// `trust` is copied from the transport (a property of the link, never of a
/// message); `granted` is what a successful login on THIS link earned.
/// Neither survives the link: a reconnect starts over, while the device's
/// login backoff (which lives on the server, not here) does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkSession {
    pub trust: LinkTrust,
    pub granted: Option<Tier>,
}

impl LinkSession {
    /// A link seen for the first time: its trust, and no login.
    #[must_use]
    pub fn new(trust: LinkTrust) -> Self {
        Self {
            trust,
            granted: None,
        }
    }

    /// The tier this link holds right now.
    ///
    /// - Trusted → edit, always (physical possession is the recovery path).
    /// - Untrusted → the higher of what its login granted and what the
    ///   device is open to (a play login on a board open at edit holds
    ///   edit); with neither, nothing.
    /// - Keyed → the same, where the grant is the tier of the key its
    ///   secure handshake matched (the anonymous key grants nothing).
    /// - Relayed → its handshake's grant **only**: the device's `open`
    ///   ("Anyone nearby") never reaches through the relay.
    #[must_use]
    pub fn effective_tier(&self, device_open: OpenTo) -> Option<Tier> {
        match self.trust {
            LinkTrust::Trusted => Some(Tier::Edit),
            LinkTrust::Untrusted | LinkTrust::Keyed => self.granted.max(device_open.tier()),
            LinkTrust::Relayed => self.granted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_links_hold_edit_whatever_else_is_true() {
        let mut session = LinkSession::new(LinkTrust::Trusted);
        assert_eq!(session.effective_tier(OpenTo::Nobody), Some(Tier::Edit));
        session.granted = Some(Tier::Play);
        assert_eq!(session.effective_tier(OpenTo::Play), Some(Tier::Edit));
    }

    #[test]
    fn keyed_links_hold_the_higher_of_their_handshake_grant_and_open() {
        let mut session = LinkSession::new(LinkTrust::Keyed);
        assert_eq!(session.effective_tier(OpenTo::Nobody), None);
        assert_eq!(session.effective_tier(OpenTo::Play), Some(Tier::Play));
        assert_eq!(session.effective_tier(OpenTo::Edit), Some(Tier::Edit));
        session.granted = Some(Tier::Play);
        assert_eq!(session.effective_tier(OpenTo::Nobody), Some(Tier::Play));
    }

    /// The relay's second lock: whatever the device is open to, a relayed
    /// link holds only what its key granted.
    #[test]
    fn relayed_links_hold_their_grant_and_never_open() {
        let mut session = LinkSession::new(LinkTrust::Relayed);
        for open in [OpenTo::Nobody, OpenTo::Play, OpenTo::Edit] {
            assert_eq!(session.effective_tier(open), None, "{open:?}");
        }
        session.granted = Some(Tier::Play);
        assert_eq!(session.effective_tier(OpenTo::Edit), Some(Tier::Play));
        session.granted = Some(Tier::Edit);
        assert_eq!(session.effective_tier(OpenTo::Nobody), Some(Tier::Edit));
    }

    #[test]
    fn untrusted_links_hold_the_higher_of_their_grant_and_open() {
        let mut session = LinkSession::new(LinkTrust::Untrusted);
        assert_eq!(session.effective_tier(OpenTo::Nobody), None);
        assert_eq!(session.effective_tier(OpenTo::Play), Some(Tier::Play));
        assert_eq!(session.effective_tier(OpenTo::Edit), Some(Tier::Edit));
        session.granted = Some(Tier::Edit);
        assert_eq!(session.effective_tier(OpenTo::Nobody), Some(Tier::Edit));
        assert_eq!(session.effective_tier(OpenTo::Play), Some(Tier::Edit));
        session.granted = Some(Tier::Play);
        assert_eq!(
            session.effective_tier(OpenTo::Edit),
            Some(Tier::Edit),
            "a play login on a board open at edit still authors"
        );
    }
}
