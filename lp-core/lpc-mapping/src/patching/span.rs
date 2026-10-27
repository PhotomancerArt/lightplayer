//! A port as a row of spans: segments, and the gaps between them
//! (names.html "Span", "Gap"). How an output card draws a port.

use alloc::vec::Vec;

use crate::part::TreeId;
use crate::patching::output_name::OutputName;
use crate::patching::port_key::PortKey;

/// A stretch of a port.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Span {
    /// A segment's lamps.
    Segment { id: TreeId, at: u32, channels: u32 },
    /// Channels no segment is patched to.
    Gap { at: u32, channels: u32 },
}

/// One port of one output, as a row of spans in channel order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortSpans {
    pub output: OutputName,
    pub port: PortKey,
    pub spans: Vec<Span>,
}

impl Span {
    /// Its first channel.
    #[must_use]
    pub fn at(&self) -> u32 {
        match self {
            Span::Segment { at, .. } | Span::Gap { at, .. } => *at,
        }
    }

    /// How many channels it covers.
    #[must_use]
    pub fn channels(&self) -> u32 {
        match self {
            Span::Segment { channels, .. } | Span::Gap { channels, .. } => *channels,
        }
    }
}
