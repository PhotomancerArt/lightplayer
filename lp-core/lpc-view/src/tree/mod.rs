//! Client-side tree mirror and delta application.
//!
//! This module provides the client-side mirror of the server tree, maintained
//! by applying `WireTreeDelta`s from the server.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/07-sync.md`.

pub mod apply;
pub mod node_tree_view;
pub mod tree_entry_view;

pub use apply::{ApplyError, apply_tree_delta, apply_tree_deltas};
pub use node_tree_view::NodeTreeView;
pub use tree_entry_view::TreeEntryView;
