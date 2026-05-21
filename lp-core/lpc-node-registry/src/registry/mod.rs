//! NodeDefRegistry — parsed node definition storage (M2).

mod def_shell;
mod def_source;
mod def_walker;
mod node_def_entry;
mod node_def_id;
mod node_def_registry;
mod node_def_state;
mod node_def_updates;
mod parse_ctx;
mod registry_error;

pub use def_source::DefSource;
pub use node_def_entry::NodeDefEntry;
pub use node_def_id::NodeDefId;
pub use node_def_registry::NodeDefRegistry;
pub use node_def_state::{NodeDefState, ValidationErrorPlaceholder};
pub use node_def_updates::NodeDefUpdates;
pub use parse_ctx::ParseCtx;
pub use registry_error::RegistryError;
