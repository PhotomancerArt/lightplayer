//! Tree types for the node runtime spine.
//!
//! This module holds domain-agnostic structural types for the node tree:
//! child kinds, lifecycle states, and sync deltas.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/01-tree.md` and
//! `docs/roadmaps/2026-04-28-node-runtime/design/07-sync.md`.

pub mod child_kind;
pub mod entry_state_view;
pub mod tree_delta;
pub mod tree_path;

pub use child_kind::{ChildKind, SlotIdx};
pub use entry_state_view::EntryStateView;
pub use tree_delta::TreeDelta;
