//! What the server remembers about one link.

use lpc_access::Tier;
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
    /// - Untrusted → what its login granted; failing that, play when the
    ///   device is explicitly `open`; failing that, nothing.
    #[must_use]
    pub fn effective_tier(&self, device_open: bool) -> Option<Tier> {
        match self.trust {
            LinkTrust::Trusted => Some(Tier::Edit),
            LinkTrust::Untrusted => {
                self.granted
                    .or(if device_open { Some(Tier::Play) } else { None })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_links_hold_edit_whatever_else_is_true() {
        let mut session = LinkSession::new(LinkTrust::Trusted);
        assert_eq!(session.effective_tier(false), Some(Tier::Edit));
        session.granted = Some(Tier::Play);
        assert_eq!(session.effective_tier(true), Some(Tier::Edit));
    }

    #[test]
    fn untrusted_links_hold_their_grant_then_open_then_nothing() {
        let mut session = LinkSession::new(LinkTrust::Untrusted);
        assert_eq!(session.effective_tier(false), None);
        assert_eq!(session.effective_tier(true), Some(Tier::Play));
        session.granted = Some(Tier::Edit);
        assert_eq!(session.effective_tier(false), Some(Tier::Edit));
        assert_eq!(session.effective_tier(true), Some(Tier::Edit));
    }
}
