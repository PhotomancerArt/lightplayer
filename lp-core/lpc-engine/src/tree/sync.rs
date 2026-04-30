//! Tree delta generation for client/server sync.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/07-sync.md`.

use alloc::vec::Vec;
use lpc_model::FrameId;
use lpc_wire::WireTreeDelta;

use super::{NodeEntry, NodeTree};

/// Generate tree deltas since a given frame.
///
/// Returns deltas for:
/// 1. Entries whose `created_frame > since` (or all entries if `since == 0`) → `WireTreeDelta::Created`
/// 2. Entries whose `children_ver > since` (and not newly created) → `WireTreeDelta::ChildrenChanged`
/// 3. Entries whose `change_frame > since` (and not newly created) → `WireTreeDelta::EntryChanged`
///
/// `since == 0` is a special case that returns all entries (bulk sync). This allows
/// the initial sync to work correctly even though root is created at frame 0.
///
/// `Created` deltas are emitted in parent-before-child order (depth-first pre-order).
pub fn tree_deltas_since<N>(tree: &NodeTree<N>, since: FrameId) -> Vec<WireTreeDelta>
where
    N: Clone,
{
    let mut deltas = Vec::new();

    // First pass: collect all live entries
    let entries: Vec<&NodeEntry<N>> = tree.entries().collect();

    // Pass 1: Created entries
    // If since == 0, return all entries. Otherwise, return entries with created_frame > since.
    // Emit in parent-before-child order by iterating from root.
    collect_created_deltas(tree, tree.root(), since, &mut deltas);

    // Collect created ids for exclusion from other delta types
    let created_ids: alloc::collections::BTreeSet<lpc_model::NodeId> = deltas
        .iter()
        .filter_map(|d| {
            if let WireTreeDelta::Created { id, .. } = d {
                Some(*id)
            } else {
                None
            }
        })
        .collect();

    // Pass 2: ChildrenChanged (children_ver > since, but not newly created)
    for entry in &entries {
        if entry.children_ver.0 > since.0 && !created_ids.contains(&entry.id) {
            deltas.push(WireTreeDelta::ChildrenChanged {
                id: entry.id,
                children: entry.children.clone(),
                children_ver: entry.children_ver,
            });
        }
    }

    // Pass 3: EntryChanged (change_frame > since, but not newly created)
    for entry in &entries {
        if entry.change_frame.0 > since.0 && !created_ids.contains(&entry.id) {
            deltas.push(WireTreeDelta::EntryChanged {
                id: entry.id,
                status: entry.status.clone(),
                state: (&entry.state).into(),
                change_frame: entry.change_frame,
            });
        }
    }

    deltas
}

/// Recursively collect Created deltas in parent-before-child order.
///
/// If `since == 0`, all entries are included (bulk sync).
/// Otherwise, only entries with `created_frame > since` are included.
fn collect_created_deltas<N>(
    tree: &NodeTree<N>,
    id: lpc_model::NodeId,
    since: FrameId,
    deltas: &mut Vec<WireTreeDelta>,
) where
    N: Clone,
{
    if let Some(entry) = tree.get(id) {
        let include = since.0 == 0 || entry.created_frame.0 > since.0;
        if include {
            deltas.push(WireTreeDelta::Created {
                id: entry.id,
                path: entry.path.clone(),
                parent: entry.parent,
                child_kind: entry.child_kind.clone(),
                children: entry.children.clone(),
                status: entry.status.clone(),
                state: (&entry.state).into(),
                created_frame: entry.created_frame,
                change_frame: entry.change_frame,
                children_ver: entry.children_ver,
            });
        }

        // Recurse to children (depth-first pre-order)
        let children: Vec<lpc_model::NodeId> = entry.children.clone();
        for child_id in children {
            collect_created_deltas(tree, child_id, since, deltas);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tree_deltas_since;
    use crate::tree::{EntryState, NodeTree};
    use alloc::vec;
    use alloc::vec::Vec;
    use lpc_model::{FrameId, NodeId, NodeName, TreePath};
    use lpc_wire::{SlotIdx, WireChildKind, WireEntryState, WireTreeDelta};

    fn make_tree() -> NodeTree<()> {
        NodeTree::new(TreePath::parse("/root.show").unwrap(), FrameId::new(0))
    }

    #[test]
    fn deltas_since_zero_returns_all_created() {
        let mut tree = make_tree();
        let root = tree.root();

        // Add some children
        let a = tree
            .add_child(
                root,
                NodeName::parse("a").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input { source: SlotIdx(0) },
                FrameId::new(1),
            )
            .unwrap();
        let b = tree
            .add_child(
                root,
                NodeName::parse("b").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input { source: SlotIdx(1) },
                FrameId::new(2),
            )
            .unwrap();

        let deltas = tree_deltas_since(&tree, FrameId::new(0));

        // Should have 3 Created deltas (root + a + b)
        let created: Vec<&WireTreeDelta> = deltas
            .iter()
            .filter(|d| matches!(d, WireTreeDelta::Created { .. }))
            .collect();
        assert_eq!(created.len(), 3);

        // Verify all ids are present
        let ids: Vec<NodeId> = created
            .iter()
            .map(|d| {
                if let WireTreeDelta::Created { id, .. } = **d {
                    id
                } else {
                    panic!()
                }
            })
            .collect();
        assert!(ids.contains(&root));
        assert!(ids.contains(&a));
        assert!(ids.contains(&b));
    }

    #[test]
    fn deltas_no_op_when_since_is_current() {
        let mut tree = make_tree();
        let root = tree.root();

        tree.add_child(
            root,
            NodeName::parse("a").unwrap(),
            NodeName::parse("vis").unwrap(),
            WireChildKind::Input { source: SlotIdx(0) },
            FrameId::new(1),
        )
        .unwrap();

        let deltas = tree_deltas_since(&tree, FrameId::new(1));
        let created: Vec<&WireTreeDelta> = deltas
            .iter()
            .filter(|d| matches!(d, WireTreeDelta::Created { .. }))
            .collect();
        // Only root was created at frame 0, so nothing new at frame 1
        assert!(created.is_empty());
    }

    #[test]
    fn deltas_include_status_change_as_entry_changed() {
        let mut tree = make_tree();
        let root = tree.root();

        let a = tree
            .add_child(
                root,
                NodeName::parse("a").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input { source: SlotIdx(0) },
                FrameId::new(1),
            )
            .unwrap();

        // Change status at frame 5
        tree.get_mut(a)
            .unwrap()
            .set_status(lpc_wire::WireNodeStatus::Ok, FrameId::new(5));

        let deltas = tree_deltas_since(&tree, FrameId::new(0));

        // Bulk sync since frame 0 should include all entries
        let created: Vec<&WireTreeDelta> = deltas
            .iter()
            .filter(|d| matches!(d, WireTreeDelta::Created { .. }))
            .collect();

        assert_eq!(created.len(), 2); // root + a

        // Since frame 1: a was created at frame 1, so 1 > 1 is false, no Created
        let deltas = tree_deltas_since(&tree, FrameId::new(1));
        let created: Vec<&WireTreeDelta> = deltas
            .iter()
            .filter(|d| matches!(d, WireTreeDelta::Created { .. }))
            .collect();
        assert_eq!(created.len(), 0); // a already seen at frame 1

        // Now check deltas since frame 4 (after a was created but before status change)
        let deltas = tree_deltas_since(&tree, FrameId::new(4));
        let changed: Vec<&WireTreeDelta> = deltas
            .iter()
            .filter(|d| matches!(d, WireTreeDelta::EntryChanged { .. }))
            .collect();
        assert_eq!(changed.len(), 1);
        if let WireTreeDelta::EntryChanged { id, status, .. } = changed[0] {
            assert_eq!(*id, a);
            assert!(matches!(status, lpc_wire::WireNodeStatus::Ok));
        }
    }

    #[test]
    fn deltas_include_children_changed() {
        let mut tree = make_tree();
        let root = tree.root();

        // Add child at frame 5
        let a = tree
            .add_child(
                root,
                NodeName::parse("a").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input { source: SlotIdx(0) },
                FrameId::new(5),
            )
            .unwrap();

        // Get deltas since frame 1 (before child was added)
        let deltas = tree_deltas_since(&tree, FrameId::new(1));

        // Root's children changed (a was added)
        let children_changed: Vec<&WireTreeDelta> = deltas
            .iter()
            .filter(|d| matches!(d, WireTreeDelta::ChildrenChanged { .. }))
            .collect();
        assert_eq!(children_changed.len(), 1);
        if let WireTreeDelta::ChildrenChanged { id, children, .. } = children_changed[0] {
            assert_eq!(*id, root);
            assert!(children.contains(&a));
        }

        // A was just created, so no separate ChildrenChanged for it (empty children)
    }

    #[test]
    fn deltas_emit_created_in_parent_before_child_order() {
        let mut tree = make_tree();
        let root = tree.root();

        // Create nested structure: root -> parent -> child
        let parent = tree
            .add_child(
                root,
                NodeName::parse("parent").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Sidecar {
                    name: NodeName::parse("parent").unwrap(),
                },
                FrameId::new(1),
            )
            .unwrap();
        let child = tree
            .add_child(
                parent,
                NodeName::parse("child").unwrap(),
                NodeName::parse("fx").unwrap(),
                WireChildKind::Input { source: SlotIdx(0) },
                FrameId::new(2),
            )
            .unwrap();

        let deltas = tree_deltas_since(&tree, FrameId::new(0));

        // Extract Created deltas in order
        let created_order: Vec<NodeId> = deltas
            .iter()
            .filter_map(|d| {
                if let WireTreeDelta::Created { id, .. } = d {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        // Should be: root (0), parent (1), child (2) in that order
        assert_eq!(created_order, vec![root, parent, child]);
    }

    #[test]
    fn deltas_after_removal_shows_children_changed_not_destroyed() {
        let mut tree = make_tree();
        let root = tree.root();

        let a = tree
            .add_child(
                root,
                NodeName::parse("a").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input { source: SlotIdx(0) },
                FrameId::new(1),
            )
            .unwrap();

        // Remove a at frame 5
        tree.remove_subtree(a, FrameId::new(5)).unwrap();

        // Get deltas since frame 1
        let deltas = tree_deltas_since(&tree, FrameId::new(1));

        // Should have ChildrenChanged for root (a was removed)
        // No Destroyed delta - client infers from ChildrenChanged
        let children_changed: Vec<&WireTreeDelta> = deltas
            .iter()
            .filter(|d| matches!(d, WireTreeDelta::ChildrenChanged { .. }))
            .collect();
        assert_eq!(children_changed.len(), 1);
        if let WireTreeDelta::ChildrenChanged { id, children, .. } = children_changed[0] {
            assert_eq!(*id, root);
            assert!(!children.contains(&a)); // a not in children anymore
        }

        // a's Created should NOT be in deltas (it's tombstoned)
        let created: Vec<&WireTreeDelta> = deltas
            .iter()
            .filter(|d| matches!(d, WireTreeDelta::Created { .. }))
            .collect();
        assert!(created.is_empty());
    }

    /// Full round-trip test: server tree → deltas → client mirror
    #[test]
    fn tree_round_trip_server_to_client() {
        use lp_engine_client::{ClientNodeTree, apply_tree_deltas};

        // Build server tree
        let mut server_tree = make_tree();
        let root = server_tree.root();

        let a = server_tree
            .add_child(
                root,
                NodeName::parse("a").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input { source: SlotIdx(0) },
                FrameId::new(1),
            )
            .unwrap();
        let b = server_tree
            .add_child(
                root,
                NodeName::parse("b").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input { source: SlotIdx(1) },
                FrameId::new(2),
            )
            .unwrap();

        // Generate deltas for initial sync (since=0)
        let deltas = tree_deltas_since(&server_tree, FrameId::new(0));

        // Apply to client
        let mut client_tree = ClientNodeTree::new();
        apply_tree_deltas(&mut client_tree, &deltas, FrameId::new(0)).unwrap();

        // Verify client matches server
        assert_eq!(client_tree.len(), 3);
        assert!(client_tree.get(root).is_some());
        assert!(client_tree.get(a).is_some());
        assert!(client_tree.get(b).is_some());

        // Verify path index works
        assert_eq!(
            client_tree.lookup_path(&TreePath::parse("/root.show").unwrap()),
            Some(root)
        );
        assert_eq!(
            client_tree.lookup_path(&TreePath::parse("/root.show/a.vis").unwrap()),
            Some(a)
        );
        assert_eq!(
            client_tree.lookup_path(&TreePath::parse("/root.show/b.vis").unwrap()),
            Some(b)
        );

        // Verify children relationships
        let client_root = client_tree.get(root).unwrap();
        assert!(client_root.children.contains(&a));
        assert!(client_root.children.contains(&b));

        // Now mutate server: remove 'a', wake 'b' (Pending -> Alive) and set status
        server_tree.remove_subtree(a, FrameId::new(5)).unwrap();
        {
            let b_entry = server_tree.get_mut(b).unwrap();
            b_entry.set_state(EntryState::Alive(()), FrameId::new(5));
            b_entry.set_status(lpc_wire::WireNodeStatus::Ok, FrameId::new(5));
        }

        // Get deltas since frame 2 (after b was created)
        let deltas = tree_deltas_since(&server_tree, FrameId::new(2));

        // Apply to client
        apply_tree_deltas(&mut client_tree, &deltas, FrameId::new(5)).unwrap();

        // Verify client updated correctly
        assert!(client_tree.get(a).is_none()); // a removed
        assert!(client_tree.get(b).is_some()); // b still there
        let client_b = client_tree.get(b).unwrap();
        assert!(matches!(client_b.status, lpc_wire::WireNodeStatus::Ok));
        assert!(matches!(client_b.state, WireEntryState::Alive));

        // Verify root's children list updated
        let client_root = client_tree.get(root).unwrap();
        assert!(!client_root.children.contains(&a));
        assert!(client_root.children.contains(&b));
        assert_eq!(client_root.children.len(), 1);
    }
}
