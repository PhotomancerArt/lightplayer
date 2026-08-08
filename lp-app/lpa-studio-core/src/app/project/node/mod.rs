//! Project node controller-domain types.
//!
//! A project node has two identities in Studio. [`ProjectNodeAddress`] is the
//! stable authored address used to preserve controller state across syncs.
//! [`ProjectNodeTarget`] adds the current runtime `NodeId` for actions that
//! need to talk back to the server.

pub mod import_pattern;
pub mod module_export_op;
pub mod node_clear_debug_op;
pub mod node_controller;
pub mod node_create_op;
pub(in crate::app::project) mod node_face_builder;
pub mod node_import_op;
pub mod node_naming;
pub mod node_remove_op;
pub mod node_remove_preflight;
pub mod node_revert_op;
pub mod node_share_op;
pub(in crate::app::project) mod node_space_section;
pub mod panel_write_op;
pub mod playlist_activate_op;
pub mod project_node_address;
pub mod project_node_target;
pub mod ui_add_node_menu;

pub use module_export_op::ModuleExportOp;
pub use node_clear_debug_op::NodeClearDebugOp;
pub(in crate::app::project) use node_controller::root_slot_key;
pub use node_controller::{NodeController, NodeControllerState, ProjectProductSubscriptionIntent};
pub use node_create_op::{NodeCreateOp, UiAttachTarget};
pub use node_import_op::NodeImportOp;
pub use node_remove_op::NodeRemoveOp;
pub use node_remove_preflight::UiNodeRemovePreflight;
pub use node_revert_op::NodeRevertOp;
pub use node_share_op::{NodeCopyOp, NodePasteOp};
pub use panel_write_op::{PanelAutoSaveOp, PanelClearOp, PanelWriteOp};
pub use playlist_activate_op::PlaylistActivateOp;
pub use project_node_address::ProjectNodeAddress;
pub use project_node_target::ProjectNodeTarget;
pub use ui_add_node_menu::{
    UiAddNodeMenu, UiAddNodeMenuEntry, UiImportablePattern, add_node_menu, gate_add_node_menu,
    set_import_source,
};
