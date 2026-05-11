//! Runtime binding vocabulary for edges between produced slots, consumed slots,
//! literals, and bus channels.
//!
//! Bindings are stored on node-tree entries. This module defines the data and
//! validation errors, but it does not own binding lifecycle or allocate global
//! binding ids.

mod binding_entry;
mod binding_error;
mod binding_set;

pub(crate) use binding_entry::channels_touched;
pub use binding_entry::{
    BindingDraft, BindingEntry, BindingPriority, BindingRef, BindingSource, BindingTarget,
};
pub use binding_error::BindingError;
pub use binding_set::BindingSet;
