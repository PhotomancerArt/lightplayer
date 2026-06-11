//! Apply pending edit model operations to node definitions and artifacts.

mod artifact_projection;
mod edit_error;
mod slot_edit_apply;

pub(crate) use artifact_projection::project_artifact_bytes;
pub use edit_error::EditError;
pub use slot_edit_apply::serialize_slot_draft;
pub(crate) use slot_edit_apply::{apply_op_to_def, parse_def_bytes};
