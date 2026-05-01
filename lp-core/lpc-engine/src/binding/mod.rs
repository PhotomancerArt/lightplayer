//! Central **binding registry**: identity and metadata for edges between sources,
//! node ports, and bus channels. Resolved values live in the engine resolver
//! cache (later phases), not here.

mod binding_entry;
mod binding_error;
mod binding_id;
mod binding_registry;

pub use binding_entry::{
    BindingDraft, BindingEntry, BindingPriority, BindingSource, BindingTarget,
};
pub use binding_error::BindingError;
pub use binding_id::BindingId;
pub use binding_registry::BindingRegistry;
