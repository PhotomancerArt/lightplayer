//! Apply pending edit model operations to node definitions and artifacts.

mod apply_asset;
mod apply_error;
mod apply_slot;


pub(crate) use apply_asset::project_artifact_bytes;
pub use apply_error::EditApplyError;
pub use apply_slot::serialize_slot_draft;
pub(crate) use apply_slot::{apply_op_to_def, parse_def_bytes};
