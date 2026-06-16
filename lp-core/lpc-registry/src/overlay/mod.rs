//! Apply pending edit model operations to node definitions.

mod apply_error;
mod apply_slot;
pub mod inventory_change_summary;
pub mod project_inventory_derivation;

pub use apply_error::EditApplyError;
pub use apply_slot::serialize_slot_draft;
pub(crate) use apply_slot::{apply_slot_overlay_to_def, parse_def_bytes};
