//! LightPlayer engine client API.
//!
//! This crate provides a client-side view of the engine state, allowing clients
//! to query project information and track status changes without direct access
//! to the engine internals.

#![no_std]

extern crate alloc;

pub mod api;
pub mod project;
pub mod prop;
pub mod slot;
pub mod tree;

pub use api::ClientApi;
pub use project::{
    ClientResourceCache, NodeEntryView, ProjectReadApplyError, ProjectView, StatusChangeView,
    apply_project_read_response,
};
pub use slot::{PendingSlotMutation, SlotMirrorError, SlotMirrorView};
pub use tree::{ApplyError, NodeTreeView, TreeEntryView, apply_tree_delta, apply_tree_deltas};
