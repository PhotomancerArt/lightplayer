//! LightPlayer engine client API.
//!
//! This crate provides a client-side view of the engine state, allowing clients
//! to query project information and track status changes without direct access
//! to the engine internals.

#![no_std]

extern crate alloc;

pub mod api;
pub mod project;
pub mod test_util;

pub use api::ClientApi;
pub use project::{ClientNodeEntry, ClientProjectView, StatusChange};
