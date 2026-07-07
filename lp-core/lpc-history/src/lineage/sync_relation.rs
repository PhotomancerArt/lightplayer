//! How an observed package version relates to a project's line.

/// Relation of an observed version (e.g. what a device is carrying) to a
/// project's history line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRelation {
    /// The observed version is the line's head — up to date.
    AtHead,
    /// The observed version is in the line's history — a fast-forward
    /// (push) brings it current.
    Behind,
    /// The observed version is not in the line — a genuine fork.
    /// Never destructive to resolve: connect-as-pull already banked it.
    Diverged,
}
