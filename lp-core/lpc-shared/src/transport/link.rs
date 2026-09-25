//! A link a transport currently has open.

use super::link_id::LinkId;
use super::link_trust::LinkTrust;

/// One open link and its trust, as a transport reports it
/// ([`super::ServerTransport::links`]) so unsolicited frames (hello,
/// heartbeat) can be addressed to each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Link {
    pub id: LinkId,
    pub trust: LinkTrust,
}

impl Link {
    /// The one trusted link of a single-link transport.
    pub const PRIMARY: Link = Link {
        id: LinkId::PRIMARY,
        trust: LinkTrust::Trusted,
    };
}
