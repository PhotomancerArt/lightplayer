//! Structural bus-scope identity (modules.md R1/R2).
//!
//! A scope is introduced either by a module node (one scope enclosing its
//! project children) or by an isolating invocation site — today exactly a
//! playlist entry, which wraps its owned child in an anonymous **sink**
//! scope. Scope identity is engine state stored on the runtime node entry
//! (`RuntimeNodeEntry::scope`), assigned by `ensure_runtime_spine` on BOTH
//! load and apply so an edited project can never wear different scopes than
//! a reloaded one. It is NOT a load-time side table: `Pending`/`Failed`
//! entries carry it too (R1 — the engine always answers), and reattach
//! replaces payloads, never entries.

use alloc::format;
use alloc::string::String;

use lpc_model::{NodeId, TreePath};

/// Identity of one bus scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeRef {
    /// The scope a module node introduces around its project children.
    Module {
        /// The introducing module node.
        owner: NodeId,
    },
    /// The anonymous sink scope one playlist entry wraps its child in
    /// (R2). `Sink` is a modeled property — resolution and probes honor it
    /// by construction, never via per-layer filters.
    Sink {
        /// The isolating playlist node.
        owner: NodeId,
        /// The authored entry key introducing this sink.
        entry: u32,
    },
}

impl ScopeRef {
    /// The node that introduces this scope.
    pub fn owner(&self) -> NodeId {
        match self {
            Self::Module { owner } | Self::Sink { owner, .. } => *owner,
        }
    }

    /// R2's isolating property: channels in a sink scope never surface on
    /// enclosing listings and are never resolved by unscoped demand.
    pub fn is_sink(&self) -> bool {
        matches!(self, Self::Sink { .. })
    }

    /// The stable string identity of this scope, given its owner's tree
    /// path. This becomes the persisted panel-state key prefix
    /// (`<scope-path>/<channel>`), so it must be stable under sibling
    /// reorder (names and authored entry keys, never indices) and across
    /// reattach/reload (tree paths, never runtime ids). A sink scope keys
    /// by the ENTRY (`…/entries[k]`), not the entry's child node path, so
    /// swapping which node an entry plays keeps the entry's panel state —
    /// state follows the slot, not the content.
    pub fn persist_path(&self, owner_path: &TreePath) -> String {
        match self {
            Self::Module { .. } => format!("{owner_path}"),
            Self::Sink { entry, .. } => format!("{owner_path}/entries[{entry}]"),
        }
    }
}
