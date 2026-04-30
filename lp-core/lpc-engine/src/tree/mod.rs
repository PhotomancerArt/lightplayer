//! The node tree container and lazy lifecycle types.
//!
//! This module holds the server-side tree implementation: `NodeTree`, `NodeEntry`,
//! `EntryState`, and mutation errors. It mirrors the design in
//! `docs/roadmaps/2026-04-28-node-runtime/design/01-tree.md`.
//!
//! Generic over `N` — the payload type when a node is `Alive`. In M3 this is
//! `()` (no Node trait yet). When the Node trait lands, this becomes
//! `Box<dyn Node>`.

pub mod entry_state;
pub mod node_entry;
pub mod node_tree;
pub mod sync;
pub mod tree_error;

pub use entry_state::EntryState;
pub use node_entry::NodeEntry;
pub use node_tree::NodeTree;
pub use sync::tree_deltas_since;
pub use tree_error::TreeError;
