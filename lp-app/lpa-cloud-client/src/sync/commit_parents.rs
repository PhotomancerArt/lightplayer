//! What a pushed commit names as its parents.

use alloc::vec::Vec;

use lpc_cloud_api::HeadInfo;
use lpc_history::{ContentHash, EventKind, ProjectHistory};

/// The parents a push of `head` must name.
///
/// Two sources, and both matter:
///
/// - **Structural** — what the head-advancing event actually built on: the
///   previous version for a save, and *both sides* for a join. The join case
///   is the load-bearing one: naming both is what collapses a two-head
///   frontier back to one (`ProjectRefs::apply_push` consumes every head a
///   push names as a parent).
/// - **Observed** — any service head this history already knows. A client
///   that saved three times since its last push has a structural parent the
///   service never saw, and would leave its own previous head standing as a
///   sibling. Naming the head we descend from fixes that without lying: we
///   do descend from it.
///
/// `head` itself is never its own parent, even though a join's `kept` side
/// equals it — the frontier rule handles the tree-equals-head case
/// separately, and a self-parent in the DAG would be a fiction with no
/// upside.
pub(crate) fn parents_of_head(
    history: &ProjectHistory,
    head: ContentHash,
    service_heads: &[HeadInfo],
) -> Vec<ContentHash> {
    let mut parents = Vec::new();
    let add = |hash: ContentHash, parents: &mut Vec<ContentHash>| {
        if hash != head && !parents.contains(&hash) {
            parents.push(hash);
        }
    };

    match last_head_advancing(history) {
        Some(EventKind::Joined { kept, set_aside }) => {
            add(*kept, &mut parents);
            add(*set_aside, &mut parents);
        }
        _ => {
            let line = line_of(history);
            if line.len() >= 2 {
                add(line[line.len() - 2], &mut parents);
            }
        }
    }

    for service_head in service_heads {
        if history.knows(service_head.tree) {
            add(service_head.tree, &mut parents);
        }
    }
    parents
}

/// The head-advancing sequence, replayed out of the event log.
///
/// `ProjectHistory` keeps this internally and does not hand it out; the rule
/// it applies is in its own module docs (origin version, saves, joins' kept
/// side) and is restated here rather than guessed at.
fn line_of(history: &ProjectHistory) -> Vec<ContentHash> {
    let mut line = Vec::new();
    for event in history.events() {
        match &event.kind {
            kind if kind.is_origin() => {
                if let Some(version) = kind.origin_version() {
                    line.push(version);
                }
            }
            EventKind::Saved { version } => line.push(*version),
            EventKind::Joined { kept, .. } => line.push(*kept),
            _ => {}
        }
    }
    line
}

fn last_head_advancing(history: &ProjectHistory) -> Option<&EventKind> {
    history
        .events()
        .iter()
        .rev()
        .map(|event| &event.kind)
        .find(|kind| {
            matches!(kind, EventKind::Saved { .. } | EventKind::Joined { .. })
                || (kind.is_origin() && kind.origin_version().is_some())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{created, hash};

    #[test]
    fn a_save_names_the_version_before_it() {
        let mut history = ProjectHistory::new(created(1.0)).unwrap();
        history.record_save(hash(b"v1"), 2.0);
        assert!(parents_of_head(&history, hash(b"v1"), &[]).is_empty());

        history.record_save(hash(b"v2"), 3.0);
        assert_eq!(
            parents_of_head(&history, hash(b"v2"), &[]),
            alloc::vec![hash(b"v1")]
        );
    }

    /// The frontier-collapsing case: a join names both sides, so a push of
    /// it consumes both service heads.
    #[test]
    fn a_join_names_both_sides() {
        let mut history = ProjectHistory::new(created(1.0)).unwrap();
        history.record_save(hash(b"mine"), 2.0);
        history
            .record_join(hash(b"mine"), hash(b"theirs"), 3.0)
            .unwrap();

        let parents = parents_of_head(&history, hash(b"mine"), &[]);
        assert_eq!(parents, alloc::vec![hash(b"theirs")]);
    }

    /// Several unpushed saves: the service's head is still named, or it
    /// would survive as a sibling of our own line.
    #[test]
    fn a_known_service_head_is_named_even_when_it_is_not_the_structural_parent() {
        let mut history = ProjectHistory::new(created(1.0)).unwrap();
        history.record_save(hash(b"v1"), 2.0);
        history.record_save(hash(b"v2"), 3.0);
        history.record_save(hash(b"v3"), 4.0);

        let service = [
            head(hash(b"v1")),
            // a head we have never heard of is not ours to consume
            head(hash(b"stranger")),
        ];
        let parents = parents_of_head(&history, hash(b"v3"), &service);
        assert_eq!(parents, alloc::vec![hash(b"v2"), hash(b"v1")]);
    }

    fn head(tree: ContentHash) -> HeadInfo {
        HeadInfo {
            tree,
            parents: alloc::vec![],
        }
    }
}
