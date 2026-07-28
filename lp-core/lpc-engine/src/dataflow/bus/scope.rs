//! Bus scopes — hierarchical channel visibility (scoped-buses ADR).
//!
//! Every project node introduces a **named scope** around its children (the
//! root project's scope is simply the outermost one), and isolating
//! invocation sites — playlist entries today — wrap each owned child in an
//! **anonymous scope**. Channels are keyed by `(scope, name)`: a consumed
//! `bus:` endpoint resolves to the nearest enclosing scope with a writer
//! (else the root scope, so unfilled channels surface where a host can fill
//! them), while a produced endpoint always writes its own nearest scope.
//! Scope assignment happens entirely at load time in the project loader; the
//! resolver and binding index just see more channel keys.

use core::fmt;

use lpc_model::{ChannelName, NodeId};

/// Identity of one bus scope.
///
/// `Project` is the named scope a project node introduces around its
/// children. `Entry` is the anonymous scope an isolating invocation site
/// wraps around one owned child, keyed by that child — distinct from
/// `Project` so a project node owned by a playlist entry gets both (its own
/// named scope nested inside the entry's anonymous one).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeId {
    /// The scope introduced by the project node with this id.
    Project(NodeId),
    /// The anonymous scope wrapped around the entry-owned child with this id.
    Entry(NodeId),
}

impl fmt::Display for ScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(node) => write!(f, "project#{}", node.as_u32()),
            Self::Entry(node) => write!(f, "entry#{}", node.as_u32()),
        }
    }
}

/// A bus channel keyed by the scope it lives in.
///
/// Two scopes each carrying a `visual.out` channel are two independent
/// channels; only the loader's scope resolution decides which one an
/// endpoint touches.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopedChannel {
    pub scope: ScopeId,
    pub channel: ChannelName,
}

impl ScopedChannel {
    pub fn new(scope: ScopeId, channel: ChannelName) -> Self {
        Self { scope, channel }
    }
}

impl fmt::Display for ScopedChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Diagnostic form only; probe/wire display formats the scope as a
        // node path (bare for the root scope) via the engine's tree.
        write!(f, "{}@{}", self.channel, self.scope)
    }
}
