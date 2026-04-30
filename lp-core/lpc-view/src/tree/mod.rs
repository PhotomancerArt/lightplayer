//! Client-side tree mirror and delta application.
//!
//! This module provides the client-side mirror of the server tree, maintained
//! by applying `WireTreeDelta`s from the server.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/07-sync.md`.

pub mod apply;
pub mod client_node_tree;
pub mod client_tree_entry;

pub use apply::{ApplyError, apply_tree_delta, apply_tree_deltas};
pub use client_node_tree::ClientNodeTree;
pub use client_tree_entry::ClientTreeEntry;
