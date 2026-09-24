//! A client message, tagged with the link it arrived on.

use lpc_wire::ClientMessage;

use super::link::Link;
use super::link_id::LinkId;
use super::link_trust::LinkTrust;

/// What [`super::ServerTransport::receive`] yields: the message, the link
/// it came in on, and that link's trust — both set by the transport.
///
/// The server keys its per-link session (login grant) on `link` and answers
/// on the same link; `trust` decides the link's tier before any login.
#[derive(Debug, Clone)]
pub struct Incoming {
    pub link: LinkId,
    pub trust: LinkTrust,
    pub msg: ClientMessage,
}

impl Incoming {
    /// A message on the one trusted link of a single-link transport.
    #[must_use]
    pub fn primary(msg: ClientMessage) -> Self {
        Self::on(Link::PRIMARY, msg)
    }

    /// A message on `link`.
    #[must_use]
    pub fn on(link: Link, msg: ClientMessage) -> Self {
        Self {
            link: link.id,
            trust: link.trust,
            msg,
        }
    }

    /// The link this message arrived on, with its trust.
    #[must_use]
    pub fn link(&self) -> Link {
        Link {
            id: self.link,
            trust: self.trust,
        }
    }
}
