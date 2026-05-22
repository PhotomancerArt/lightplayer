//! Parsed node definition registry, filesystem sync, and commit promotion.

mod def_shell;
mod def_source;
mod def_walker;
mod node_def_entry;
mod node_def_id;
mod node_def_registry;
mod node_def_state;
mod node_def_updates;
mod parse_ctx;
mod registry_change;
mod registry_error;
mod source_bridge;
mod source_deps;
mod sync_result;

pub use def_source::DefSource;
pub(crate) use def_walker::resolve_node_locator;
pub use node_def_entry::NodeDefEntry;
pub use node_def_id::NodeDefId;
#[cfg(feature = "diff")]
pub(crate) use node_def_registry::apply_ops_to_node_def;
pub use node_def_registry::{NodeDefRegistry, serialize_slot_draft};
pub use node_def_state::{NodeDefState, ValidationErrorPlaceholder};
pub use node_def_updates::NodeDefUpdates;
pub use parse_ctx::ParseCtx;
pub use registry_change::RegistryChange;
pub use registry_error::RegistryError;
pub use sync_result::{DefChangeDetail, SourceRevisionBump, SyncResult};
