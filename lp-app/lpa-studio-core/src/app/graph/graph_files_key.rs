//! When the open project's files may have changed.

/// The open project's revisions, as the Graph watches them: when none of
/// these moved, no file the Graph read can have changed, so it doesn't
/// read them again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphFilesKey {
    /// The project's filesystem root on the board: another root is
    /// another project.
    pub root: String,
    /// The running project's revision, as the last sync read it.
    pub revision: Option<i64>,
    /// The overlay's revision: every acknowledged edit moves it.
    pub overlay: i64,
    /// The local edit journal's count: an edit not yet acknowledged.
    pub edits: u64,
}
