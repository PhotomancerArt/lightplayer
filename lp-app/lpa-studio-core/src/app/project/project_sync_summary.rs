use crate::{ProjectRuntimeSummary, ProjectSyncPhase, UiIssue};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectSyncSummary {
    pub phase: ProjectSyncPhase,
    pub revision: i64,
    /// Revision at which the pending-edit overlay mirror was last stamped
    /// from a server response (zero: never mirrored / no overlay mutation).
    pub overlay_revision: i64,
    pub node_count: usize,
    pub root_node_count: usize,
    pub slot_root_count: usize,
    pub resource_count: usize,
    pub shape_count: usize,
    pub shapes_complete: bool,
    pub runtime: Option<ProjectRuntimeSummary>,
    pub issue: Option<UiIssue>,
}
