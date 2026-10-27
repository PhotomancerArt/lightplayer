//! A flow: a part's high-level patching, how the starts of its children come
//! out (names.html "Flow").
//!
//! **Flows are generators** (D4): a flow never moves the cursor itself. It
//! renders a start for each child, and the patcher sees only starts, so a
//! new flow is a new [`Flow::render`] arm and neither the patcher nor how an
//! override is stored has to change.

use crate::patching::amount::Amount;
use crate::patching::port_key::PortKey;
use crate::patching::start::{ChannelOp, OutputOp, PortOp, Start};

/// How a part's children follow each other.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Flow {
    /// Each child where the last one ended: every child keeps.
    #[default]
    EndToEnd,
    /// Child k on the k-th port from `from`, at its first lamp. Absolute
    /// (spike D2): the slots are kept, as jacks are physical.
    OnePerPort { from: PortKey },
}

impl Flow {
    /// The start it renders for child `k`.
    #[must_use]
    pub fn render(&self, k: usize) -> Start {
        match self {
            Flow::EndToEnd => Start::KEEP,
            Flow::OnePerPort { from } => Start {
                output: OutputOp::Keep,
                port: PortOp::Set {
                    key: *from,
                    plus: k as u32,
                },
                channel: ChannelOp::Set(Amount::lamps(0)),
            },
        }
    }
}
