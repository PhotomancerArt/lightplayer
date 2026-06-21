pub enum ServerState {
    None,
    Opening,
    Ready,
    ReadingProjectState,
    ProjectSelectionRequired,
    RecoveryRequired,
    Degraded,
}
