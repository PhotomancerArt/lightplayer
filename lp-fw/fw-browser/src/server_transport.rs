//! In-memory server transport for browser worker protocol frames.

use lpc_shared::transport::{Incoming, Link, LinkId, ServerTransport};
use lpc_wire::{ClientMessage, TransportError, WireServerMessage};

/// Queue-backed transport between `BrowserFirmwareRuntime` and `LpServer`.
pub(crate) struct BrowserServerTransport {
    incoming: Vec<ClientMessage>,
    outgoing: Vec<WireServerMessage>,
    closed: bool,
}

impl BrowserServerTransport {
    /// Create an empty in-memory server transport.
    pub(crate) fn new() -> Self {
        Self {
            incoming: Vec::new(),
            outgoing: Vec::new(),
            closed: false,
        }
    }

    /// Queue a client protocol message for the next runtime tick.
    pub(crate) fn push_incoming(&mut self, msg: ClientMessage) {
        self.incoming.push(msg);
    }

    /// Drain server protocol messages emitted during recent ticks.
    pub(crate) fn take_outgoing(&mut self) -> Vec<WireServerMessage> {
        core::mem::take(&mut self.outgoing)
    }
}

impl ServerTransport for BrowserServerTransport {
    async fn send(&mut self, _link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }
        self.outgoing.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }
        Ok(if self.incoming.is_empty() {
            None
        } else {
            Some(Incoming::primary(self.incoming.remove(0)))
        })
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }
        Ok(core::mem::take(&mut self.incoming)
            .into_iter()
            .map(Incoming::primary)
            .collect())
    }

    fn links(&self) -> Vec<Link> {
        vec![Link::PRIMARY]
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.closed = true;
        Ok(())
    }
}
