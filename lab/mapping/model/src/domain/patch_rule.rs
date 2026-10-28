//! Rules on components: the high-level patching a person authors
//! (theory "Patching"; Yona's D2, 2026-10-25).
//!
//! **Rules are generators.** A rule never moves the cursor itself: it
//! renders cursor steps (`cursor_op.rs`) for the objects it covers, and the
//! walker sees only steps. So a new rule is a new generator here, and
//! neither the walker nor how an override is stored has to change (Yona:
//! "we can extend it later and we won't regret what we build now").
//!
//! Two settings, on the fixture and on every component:
//!
//! - **Start**, where the thing begins: Follows (where the walk is), At an
//!   output's port and channel, or the Next port. The fixture's must be At:
//!   nothing flows across fixtures.
//! - **Children**, on a container (the fixture, a group, a circle, a
//!   repeat): how its children follow each other. End to end, or One per
//!   port from a port — absolute, so each child keeps its slot when one is
//!   moved elsewhere (jacks are physical).

use crate::domain::cursor_op::{Amount, ChannelOp, CursorOps, OutputOp, PortOp};
use crate::domain::node::NodeId;

/// Where something begins.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Start {
    /// Where the walk already is: no step of its own.
    #[default]
    Follows,
    /// This channel of this port of this output.
    At {
        output: NodeId,
        port: String,
        channel: Amount,
    },
    /// The next port, from its first channel.
    NextPort,
}

/// How a container's children follow each other.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Children {
    /// Each where the last one ended.
    #[default]
    EndToEnd,
    /// Child k on the k-th port from `from`, at its first channel.
    OnePerPort { from: String },
}

/// The rules on one component (or the fixture).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ComponentRules {
    pub start: Start,
    pub children: Children,
}

impl ComponentRules {
    /// Nothing set: follows, end to end.
    pub fn is_default(&self) -> bool {
        *self == ComponentRules::default()
    }
}

impl Start {
    /// The steps it renders for the thing it is on; `None` when it follows
    /// (it adds nothing).
    pub fn render(&self) -> Option<CursorOps> {
        match self {
            Start::Follows => None,
            Start::At {
                output,
                port,
                channel,
            } => Some(CursorOps::at(output, port.clone(), *channel)),
            Start::NextPort => Some(CursorOps {
                output: OutputOp::Keep,
                port: PortOp::Add(1),
                channel: ChannelOp::Set(Amount::ZERO),
            }),
        }
    }
}

impl Children {
    /// The steps it renders for child `k`.
    pub fn render(&self, k: usize) -> CursorOps {
        match self {
            Children::EndToEnd => CursorOps::KEEP,
            Children::OnePerPort { from } => CursorOps {
                output: OutputOp::Keep,
                port: PortOp::Set {
                    key: from.clone(),
                    plus: k as u32,
                },
                channel: ChannelOp::Set(Amount::ZERO),
            },
        }
    }
}
