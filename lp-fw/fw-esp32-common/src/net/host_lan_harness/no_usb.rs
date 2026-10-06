//! The mux's primary transport on the host harness: a USB cable with nothing
//! plugged into it. No link, nothing received, everything addressed to it
//! dropped — a board on the desk with only its network up.

use alloc::vec::Vec;

use lpc_shared::transport::{Incoming, Link, LinkId, ServerTransport};
use lpc_wire::{TransportError, WireServerMessage};

use crate::link_upkeep::LinkUpkeep;
use crate::radio_link::FrameBufHolder;

/// A USB link that never comes up.
pub struct NoUsb;

impl FrameBufHolder for NoUsb {}

impl LinkUpkeep for NoUsb {}

impl ServerTransport for NoUsb {
    async fn send(&mut self, _link: LinkId, _msg: WireServerMessage) -> Result<(), TransportError> {
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        Ok(Vec::new())
    }

    fn links(&self) -> Vec<Link> {
        Vec::new()
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}
