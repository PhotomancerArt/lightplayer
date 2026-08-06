//! What a membership row grants.

/// A member's role on one project.
///
/// There are exactly two, and the distinction is narrow on purpose: both
/// roles read and write the project; only the owner is undeletable. Finer
/// roles (viewer, admin) are a later product decision, not a shape to
/// speculatively carve now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberRole {
    /// The account that published the project. Cannot be removed.
    Owner,
    /// An account granted access by the owner (or another member).
    Member,
}
