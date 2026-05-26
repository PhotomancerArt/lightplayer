//! Parsed node definition registry, filesystem sync, and commit promotion.

mod def_shell;
mod def_walker;
mod node_def_entry;
mod node_def_id;
mod node_def_loc;
mod node_def_registry;
mod node_def_state;
mod node_def_updates;
mod parse_ctx;
mod registry_change;
mod registry_error;
mod source_bridge;
mod sync_error;
mod sync_op;
mod sync_outcome;
mod sync_result;

pub(crate) use def_walker::resolve_node_specifier;
pub use node_def_entry::NodeDefEntry;
pub use node_def_id::NodeDefId;
pub use node_def_loc::NodeDefLoc;
#[cfg(feature = "diff")]
pub(crate) use node_def_registry::apply_ops_to_node_def;
pub use node_def_registry::{NodeDefRegistry, serialize_slot_draft};
pub use node_def_state::{NodeDefState, ValidationErrorPlaceholder};
pub use node_def_updates::NodeDefUpdates;
pub use parse_ctx::ParseCtx;
#[allow(deprecated, reason = "legacy sync op alias for migration")]
pub use registry_change::RegistryChange;
pub use registry_error::RegistryError;
pub use sync_error::SyncError;
pub use sync_op::SyncOp;
pub use sync_outcome::SyncOutcome;
pub use sync_result::{DefChangeDetail, SyncResult};
