//! An override: what a person sets on one node, by its id, over what its
//! part's start and its parent's flow give it (names.html "Override").
//!
//! Each field is overridden on its own: give ring 3 a port and a channel and
//! its output still comes from the circle. An override never moves anything
//! but its own node's start; what follows carries on from there. Overrides
//! are by id and never retargeted: one whose node is gone is reported and
//! kept (D12). (The file is `override_entry.rs`: `override` is a reserved
//! word.)

use crate::patching::direction::Direction;
use crate::patching::start::{ChannelOp, OutputOp, PortOp};

/// What one node overrides. Each `None` takes what the node would get.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Override {
    pub output: Option<OutputOp>,
    pub port: Option<PortOp>,
    pub channel: Option<ChannelOp>,
    /// On no lamp: it makes no span, and the cursor doesn't move past it.
    /// On a part, every node under it is off too.
    pub off: bool,
    /// Which way its data runs. On a part, the default for every segment
    /// under it that doesn't say; on a segment, absolute (F14).
    pub direction: Option<Direction>,
    /// A wrapping shift of its lamps within its run, in lamps.
    pub offset: Option<u32>,
}

impl Override {
    /// Overrides nothing, so it can go.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Override::default()
    }
}
