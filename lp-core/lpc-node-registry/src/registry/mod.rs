//! Parsed node definition registry, filesystem sync, and commit promotion.

mod changes;
mod commit;
mod commit_error;
mod effective_projection;
mod effective_read;
mod inventory;
mod load;
mod node_def_entry;
mod node_def_loc;
mod node_def_registry;
mod node_def_state;
mod node_def_updates;
mod overlay_mutation;
mod parse_ctx;
pub mod path_validation;
mod queue_edit;
mod registry_change;
mod registry_error;
mod sync;
mod sync_error;
mod sync_op;
mod sync_outcome;
mod sync_result;

#[cfg(feature = "diff")]
pub(crate) use crate::edit_apply::apply_ops_to_node_def;
pub use crate::edit_apply::serialize_slot_draft;
pub use commit_error::CommitError;
pub use node_def_entry::NodeDefEntry;
pub use node_def_loc::NodeDefLoc;
pub use node_def_registry::NodeDefRegistry;
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
