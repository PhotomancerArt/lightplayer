//! How an observed package version relates to a project's line.

/// Relation of an observed version (e.g. what a device is carrying) to a
/// project's history line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRelation {
    /// The observed version is the line's head — up to date.
    AtHead,
    /// The observed version is in the line's history, or was set aside by
    /// a clobber join — a fast-forward (push) brings it current.
    Behind,
    /// The observed version is not known to the history — a genuine
    /// divergence. Never destructive to resolve: connect-as-pull banks
    /// device copies, and joins keep both sides reachable.
    Diverged,
}
