//! Studio project editor controller and sync model.
//!
//! The project editor intentionally keeps several trees separate:
//!
//! - The **mirror tree** is `lpc_view::ProjectView`, updated from LightPlayer
//!   project sync responses. It is a client/protocol model and knows nothing
//!   about Studio UI concepts.
//! - The **controller tree** is the Studio business-logic layer that reconciles
//!   against the mirror tree. It owns project editor identity, actions, local
//!   interaction state, and future slot/product behavior without depending on a
//!   particular UI framework.
//! - The **DTO tree** is the data-driven render model emitted by controllers,
//!   primarily `UiNodeView` and its child `Ui*` structs.
//! - The **component tree** lives in `lpa-studio-web`; Dioxus components own
//!   browser-specific view state such as popovers, animation, and transient
//!   layout mechanics.
//!
//! `ProjectSync` is still the bridge from the mirror tree to the current editor
//! DTOs. The controller tree will make that bridge explicit so incoming project
//! updates can reconcile durable node and slot controllers before rendering.

pub mod demo_project;
pub mod loaded_project_choice;
pub mod project_connect_result;
pub mod project_controller;
pub mod project_editor_op;
pub mod project_editor_target;
pub mod project_editor_view;
pub mod project_inventory_summary;
pub mod project_node_tree_view;
pub mod project_node_view;
pub mod project_op;
pub mod project_runtime_summary;
pub mod project_slot_view;
pub mod project_snapshot;
pub mod project_state;
pub mod project_sync;
pub mod project_sync_phase;
pub mod project_sync_run;
pub mod project_sync_summary;
pub mod project_value_format;

pub use loaded_project_choice::LoadedProjectChoice;
pub use project_connect_result::ProjectConnectResult;
pub use project_controller::ProjectController;
pub use project_editor_op::ProjectEditorOp;
pub use project_editor_target::ProjectEditorTarget;
pub use project_editor_view::ProjectEditorView;
pub use project_inventory_summary::ProjectInventorySummary;
pub use project_node_tree_view::{
    ProjectNodeStatusTone, ProjectNodeStatusView, ProjectNodeTreeItem, ProjectNodeTreeView,
};
pub use project_node_view::ProjectNodeView;
pub use project_op::ProjectOp;
pub use project_runtime_summary::ProjectRuntimeSummary;
pub use project_slot_view::{
    ProjectSlotGroupView, ProjectSlotIssueView, ProjectSlotRowView, ProjectSlotValueView,
};
pub use project_snapshot::ProjectSnapshot;
pub use project_state::ProjectState;
pub use project_sync::ProjectSync;
pub use project_sync_phase::ProjectSyncPhase;
pub use project_sync_run::ProjectSyncRun;
pub use project_sync_summary::ProjectSyncSummary;
pub use project_value_format::{format_lp_value, format_slot_map_key};
