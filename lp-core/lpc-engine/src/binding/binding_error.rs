use core::fmt;

use lpc_model::{ChannelName, Kind, NodeId};

use super::BindingPriority;

/// Errors from validating node-owned runtime bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BindingError {
    /// Binding owner does not exist in the node tree.
    UnknownOwner { owner: NodeId },
    /// Another binding on the same channel already uses a different [`Kind`].
    KindMismatch {
        channel: ChannelName,
        established: Kind,
        attempted: Kind,
    },
    /// Two providers targeting the same bus channel share the same priority.
    DuplicateProviderPriority {
        channel: ChannelName,
        priority: BindingPriority,
    },
}

impl fmt::Display for BindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOwner { owner } => write!(f, "unknown binding owner {owner:?}"),
            Self::KindMismatch {
                channel,
                established,
                attempted,
            } => write!(
                f,
                "kind mismatch on bus channel {channel}: established {established:?}, attempted {attempted:?}",
            ),
            Self::DuplicateProviderPriority { channel, priority } => write!(
                f,
                "duplicate provider priority {priority} on bus channel {channel}",
            ),
        }
    }
}
