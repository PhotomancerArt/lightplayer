//! ResolveError — focused error type for slot resolution failures.
//!
//! [`SessionResolveError`] is the structured error for [`super::ResolveSession`]
//! and [`super::ResolveHost`] (engine demand path).

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::String;

use lpc_model::ChannelName;
use lpc_model::NodeId;

use crate::shader_abi::LpsValueToModelConversionError;

use super::query_key::QueryKey;
use super::resolve_trace::ResolveTraceError;

/// Error during demand-driven resolution in [`super::ResolveSession`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionResolveError {
    Cycle {
        query: QueryKey,
    },
    NoBusProvider {
        channel: ChannelName,
    },
    AmbiguousBusBinding {
        channel: ChannelName,
    },
    UnresolvedConsumedSlot {
        node: NodeId,
        slot: lpc_model::SlotPath,
    },
    /// A consumed slot read through an option's `some` whose authored option
    /// holds nothing, with nothing written on its channel either: absent, not
    /// broken. Carries no path so the per-frame read of an absent option
    /// allocates nothing.
    AbsentOption {
        node: NodeId,
    },
    Trace(ResolveTraceError),
    Other(String),
}

impl SessionResolveError {
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}

impl From<ResolveTraceError> for SessionResolveError {
    fn from(value: ResolveTraceError) -> Self {
        match value {
            ResolveTraceError::Cycle { query } => Self::Cycle { query },
        }
    }
}

impl From<LpsValueToModelConversionError> for SessionResolveError {
    fn from(err: LpsValueToModelConversionError) -> Self {
        Self::Other(format!("{err}"))
    }
}

impl core::fmt::Display for SessionResolveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Cycle { query } => write!(f, "resolve cycle at {query:?}"),
            Self::NoBusProvider { channel } => {
                write!(f, "no bus provider for channel {channel:?}")
            }
            Self::AmbiguousBusBinding { channel } => write!(
                f,
                "ambiguous bus binding (equal top priority) for channel {channel:?}",
            ),
            Self::UnresolvedConsumedSlot { node, slot } => {
                write!(f, "unresolved consumed slot node={node:?} slot={slot:?}",)
            }
            Self::AbsentOption { node } => write!(f, "{ABSENT_OPTION_MESSAGE} node={node:?}"),
            Self::Trace(e) => write!(f, "{e:?}"),
            Self::Other(msg) => f.write_str(msg),
        }
    }
}

impl core::error::Error for SessionResolveError {}

/// Error during slot resolution in the binding cascade.
///
/// Carries a descriptive message for debugging, and one typed case: an
/// absent option ([`Self::is_absent_option`]), which a node reading an
/// optional field meets every frame and must not have to recognise by text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveError {
    pub message: Cow<'static, str>,
    absent_option: bool,
}

/// The text of an absent-option error. It contains the model's "option slot
/// is none" so a reader that still recognises the case by text agrees.
const ABSENT_OPTION_MESSAGE: &str = "option slot is none";

impl ResolveError {
    /// Create a new resolve error with a message.
    pub fn new(message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            message: message.into(),
            absent_option: false,
        }
    }

    /// The slot names the `some` of an option that holds nothing, and nothing
    /// is written on its channel. Allocation-free.
    pub fn absent_option() -> Self {
        Self {
            message: Cow::Borrowed(ABSENT_OPTION_MESSAGE),
            absent_option: true,
        }
    }

    /// True for [`Self::absent_option`]: the value is absent, which a reader
    /// of an optional field treats as `None`. Every other error is a real
    /// failure.
    pub fn is_absent_option(&self) -> bool {
        self.absent_option
    }

    /// Create an error for a missing target node in NodeProp resolution.
    pub fn target_node_not_found(node_path: impl Into<String>) -> Self {
        Self::new(format!(
            "NodeProp target node not found: {}",
            node_path.into()
        ))
    }

    /// Create an error for a missing property on a target node.
    pub fn target_prop_not_found(
        node_path: impl Into<String>,
        prop_path: impl Into<String>,
    ) -> Self {
        Self::new(format!(
            "NodeProp property not found on target {}: {}",
            node_path.into(),
            prop_path.into()
        ))
    }

    /// True when this error is the "channel has no provider anywhere"
    /// case — the one shape callers may treat as a legitimate empty
    /// channel (module mirrors render cleared, R7). Keyed off the
    /// [`SessionResolveError::NoBusProvider`] display form, which the
    /// tick-resolver bridge flattens to a message; keep the two in sync.
    pub fn is_no_bus_provider(&self) -> bool {
        self.message.starts_with("no bus provider")
    }

    /// Create an error for unresolvable binding.
    pub fn unresolvable(prop_path: impl Into<String>) -> Self {
        Self::new(format!(
            "Could not resolve binding for property: {}",
            prop_path.into()
        ))
    }
}

impl From<SessionResolveError> for ResolveError {
    /// The tick-facing form of a session error: the absent case stays typed
    /// and allocation-free; everything else flattens to its display text.
    fn from(err: SessionResolveError) -> Self {
        match err {
            SessionResolveError::AbsentOption { .. } => Self::absent_option(),
            other => Self::new(format!("{other}")),
        }
    }
}

impl core::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl core::error::Error for ResolveError {}

#[cfg(test)]
mod tests {
    use super::ResolveError;

    #[test]
    fn resolve_error_new_stores_message() {
        let err = ResolveError::new("test message");
        assert_eq!(err.message, "test message");
    }

    #[test]
    fn target_node_not_found_formats_correctly() {
        let err = ResolveError::target_node_not_found("/show/node1");
        assert!(err.message.contains("target node not found"));
        assert!(err.message.contains("/show/node1"));
    }

    #[test]
    fn target_prop_not_found_formats_correctly() {
        let err = ResolveError::target_prop_not_found("/show/node1", "outputs.color");
        assert!(err.message.contains("NodeProp property not found"));
        assert!(err.message.contains("/show/node1"));
        assert!(err.message.contains("outputs.color"));
    }

    #[test]
    fn unresolvable_formats_correctly() {
        let err = ResolveError::unresolvable("params.speed");
        assert!(err.message.contains("Could not resolve"));
        assert!(err.message.contains("params.speed"));
    }

    #[test]
    fn absent_option_is_typed_and_allocation_free() {
        let (err, allocs) = crate::test_alloc_counter::measure(ResolveError::absent_option);
        assert!(err.is_absent_option());
        assert!(err.message.contains("option slot is none"));
        assert_eq!(allocs.allocs, 0, "{allocs:?}");
        assert!(!ResolveError::new("option slot is none").is_absent_option());
    }

    #[test]
    fn session_absent_option_bridges_to_the_typed_case() {
        let node = lpc_model::NodeId::new(3);
        let (err, allocs) = crate::test_alloc_counter::measure(|| {
            ResolveError::from(super::SessionResolveError::AbsentOption { node })
        });
        assert!(err.is_absent_option());
        assert_eq!(allocs.allocs, 0, "{allocs:?}");
        let other = ResolveError::from(super::SessionResolveError::UnresolvedConsumedSlot {
            node,
            slot: lpc_model::SlotPath::parse("cycle.some").expect("path"),
        });
        assert!(!other.is_absent_option());
        assert!(other.message.contains("unresolved consumed slot"));
    }

    #[test]
    fn display_trait_works() {
        let err = ResolveError::new("test");
        let s = alloc::format!("{err}");
        assert_eq!(s, "test");
    }
}
