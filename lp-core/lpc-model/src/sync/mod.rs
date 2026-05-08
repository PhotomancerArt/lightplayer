//! Synchronization revision primitives.
//!
//! A [`Revision`] is the monotonic change marker for the shared synchronized
//! state model. Slot values, slot containers, shape registries, and future
//! mutable authored data use revisions to describe when observable state last
//! changed. This keeps sync/change tracking separate from schema or file-format
//! versions.

pub mod current_revision;
pub mod revision;
pub mod with_revision;
