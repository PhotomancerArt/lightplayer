//! A project's head frontier, and the one rule that moves it.

use alloc::vec::Vec;
use lpc_cloud_api::{HeadInfo, PushOutcome};
use lpc_history::ContentHash;

use crate::model::head_ref::HeadRef;

/// The set of tips a project's history currently has — normally exactly
/// one.
///
/// More than one head means two clients pushed from the same base and
/// neither was blocked (D5). That state is legal, visible, and temporary:
/// a client resolves it with a clobber join whose push names both heads as
/// parents, which collapses the frontier back to one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectRefs {
    /// The frontier, kept sorted by tree hash so iteration is deterministic.
    pub heads: Vec<HeadRef>,
}

impl ProjectRefs {
    /// An empty frontier — a published project with no commits yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the project has no commits yet.
    pub fn is_empty(&self) -> bool {
        self.heads.is_empty()
    }

    /// The client-facing view of the frontier.
    pub fn to_head_infos(&self) -> Vec<HeadInfo> {
        self.heads.iter().map(HeadRef::to_head_info).collect()
    }

    /// Fold a push into the frontier and report what it did.
    ///
    /// One rule covers every case: the pushed commit **consumes** every head
    /// it names as a parent (and any head that is already the pushed tree),
    /// and takes their place. What is left over stays.
    ///
    /// - parents = the sole head → nothing survives → fast-forward,
    ///   [`PushOutcome::Advanced`].
    /// - parents = both heads (a clobber join) → nothing survives → the
    ///   frontier collapses to the join, [`PushOutcome::Advanced`].
    /// - parents name a stale or absent base → the old head survives
    ///   alongside → [`PushOutcome::NewHead`]. **This is never an error**:
    ///   the divergence is recorded, not refused.
    pub fn apply_push(&mut self, tree: ContentHash, parents: &[ContentHash]) -> PushOutcome {
        let mut survivors: Vec<HeadRef> = self
            .heads
            .iter()
            .filter(|head| head.tree != tree && !parents.contains(&head.tree))
            .cloned()
            .collect();
        let consumed_everything = survivors.is_empty();
        survivors.push(HeadRef {
            tree,
            parents: parents.to_vec(),
        });
        survivors.sort_by(|a, b| a.tree.cmp(&b.tree));
        self.heads = survivors;

        if consumed_everything {
            PushOutcome::Advanced
        } else {
            PushOutcome::NewHead
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(data: &[u8]) -> ContentHash {
        ContentHash::of(data)
    }

    /// The service-level push cases (fast-forward, second head, join
    /// collapse) are covered in `cloud_service.rs`. This one is awkward to
    /// reach through the service and is exactly where a naive
    /// "replace or append" rule goes wrong: a push that consumes *one* of
    /// two heads must leave the other standing.
    #[test]
    fn partial_consumption_leaves_the_other_head_standing() {
        let mut refs = ProjectRefs::new();
        refs.apply_push(hash(b"a"), &[]);
        refs.apply_push(hash(b"b"), &[]);
        assert_eq!(refs.heads.len(), 2);

        let outcome = refs.apply_push(hash(b"a2"), &[hash(b"a")]);
        assert_eq!(outcome, PushOutcome::NewHead);
        let trees: Vec<ContentHash> = refs.heads.iter().map(|head| head.tree).collect();
        assert!(trees.contains(&hash(b"a2")));
        assert!(trees.contains(&hash(b"b")));
        assert!(!trees.contains(&hash(b"a")));
    }

    /// Re-pushing the head the project already has is idempotent, not a
    /// second head.
    #[test]
    fn re_pushing_an_existing_head_does_not_duplicate_it() {
        let mut refs = ProjectRefs::new();
        refs.apply_push(hash(b"a"), &[]);
        let outcome = refs.apply_push(hash(b"a"), &[]);
        assert_eq!(outcome, PushOutcome::Advanced);
        assert_eq!(refs.heads.len(), 1);
    }
}
