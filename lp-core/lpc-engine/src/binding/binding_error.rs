use core::fmt;

use lpc_model::{ChannelName, Kind};

use super::BindingId;
use super::BindingPriority;

/// Errors from [`crate::binding::BindingRegistry`] operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BindingError {
    /// The registry exhausted its non-zero `u32` id space.
    IdExhausted,
    /// No binding with this id (e.g. unregister).
    UnknownBinding { id: BindingId },
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
            Self::IdExhausted => f.write_str("binding id space exhausted"),
            Self::UnknownBinding { id } => write!(f, "unknown binding id {id}"),
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
