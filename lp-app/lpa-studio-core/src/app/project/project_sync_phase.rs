#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectSyncPhase {
    #[default]
    Empty,
    SyncingShapes,
    SyncingProject,
    Ready,
    Failed,
}
