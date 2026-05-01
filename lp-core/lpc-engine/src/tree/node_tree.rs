//! The node tree container: flat slot storage with path and sibling indices.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/01-tree.md` §NodeTree.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lpc_model::{FrameId, NodeId, NodeName, NodePathSegment, TreePath};
use lpc_source::SrcNodeConfig;
use lpc_wire::WireChildKind;

use crate::artifact::ArtifactRef;

use super::{NodeEntry, TreeError};

/// The node tree container.
///
/// Generic over `N` — the payload type in `EntryState::Alive(N)`. In M3 this
/// is `()` (no Node trait yet). When the Node trait lands, this becomes
/// `Box<dyn Node>`.
#[derive(Debug)]
pub struct NodeTree<N> {
    nodes: Vec<Option<NodeEntry<N>>>,
    by_path: BTreeMap<TreePath, NodeId>,
    by_sibling: BTreeMap<(NodeId, NodeName), NodeId>,
    next_id: u32,
    root: NodeId,
}

impl<N> NodeTree<N> {
    /// Create a new tree with a root node at the given path and frame.
    pub fn new(root_path: TreePath, frame: FrameId) -> Self {
        let root_id = NodeId::new(0);
        let root_entry = NodeEntry::new(root_id, root_path.clone(), None, None, frame);

        let mut nodes = Vec::new();
        nodes.push(Some(root_entry));

        let mut by_path = BTreeMap::new();
        by_path.insert(root_path, root_id);

        Self {
            nodes,
            by_path,
            by_sibling: BTreeMap::new(),
            next_id: 1,
            root: root_id,
        }
    }

    /// Get the root node id.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Get a reference to an entry by id.
    pub fn get(&self, id: NodeId) -> Option<&NodeEntry<N>> {
        self.nodes.get(id.0 as usize).and_then(|opt| opt.as_ref())
    }

    /// Get a mutable reference to an entry by id.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut NodeEntry<N>> {
        self.nodes
            .get_mut(id.0 as usize)
            .and_then(|opt| opt.as_mut())
    }

    /// Look up a node by its path.
    pub fn lookup_path(&self, path: &TreePath) -> Option<NodeId> {
        self.by_path.get(path).copied()
    }

    /// Look up a sibling by parent id and name.
    pub fn lookup_sibling(&self, parent: NodeId, name: NodeName) -> Option<NodeId> {
        self.by_sibling.get(&(parent, name)).copied()
    }

    /// Iterate over all live entries (skips tombstones).
    pub fn entries(&self) -> impl Iterator<Item = &NodeEntry<N>> {
        self.nodes.iter().filter_map(|opt| opt.as_ref())
    }

    /// Iterate over all live entries mutably (skips tombstones).
    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut NodeEntry<N>> {
        self.nodes.iter_mut().filter_map(|opt| opt.as_mut())
    }

    /// Add a child to a parent node.
    ///
    /// Returns the new child's `NodeId` on success.
    pub fn add_child(
        &mut self,
        parent: NodeId,
        name: NodeName,
        ty: NodeName,
        child_kind: WireChildKind,
        config: SrcNodeConfig,
        artifact: ArtifactRef,
        frame: FrameId,
    ) -> Result<NodeId, TreeError> {
        // Validate parent exists and is in the tree
        let parent_path = self
            .get(parent)
            .ok_or(TreeError::UnknownNode(parent))?
            .path
            .clone();

        // Check for sibling name collision
        let sibling_key = (parent, name.clone());
        if self.by_sibling.contains_key(&sibling_key) {
            return Err(TreeError::SiblingNameCollision { parent, name });
        }

        // Construct child's path
        let mut child_path = parent_path;
        child_path.0.push(NodePathSegment {
            name: name.clone(),
            ty,
        });

        // Allocate new id
        let child_id = NodeId::new(self.next_id);
        self.next_id += 1;

        // Create entry
        let child_entry = NodeEntry::new_spine(
            child_id,
            child_path.clone(),
            Some(parent),
            Some(child_kind),
            config,
            artifact,
            frame,
        );

        // Ensure nodes vec is large enough
        let idx = child_id.0 as usize;
        if idx >= self.nodes.len() {
            self.nodes.resize_with(idx + 1, || None);
        }
        self.nodes[idx] = Some(child_entry);

        // Update indices
        self.by_path.insert(child_path, child_id);
        self.by_sibling.insert(sibling_key, child_id);

        // Add to parent's children list and bump parent's children_ver
        if let Some(p) = self.get_mut(parent) {
            p.children.push(child_id);
            p.children_ver = frame;
        }

        Ok(child_id)
    }

    /// Remove a subtree (depth-first, children-first).
    ///
    /// Tombstones every descendant slot. Forbidden on root.
    pub fn remove_subtree(&mut self, id: NodeId, frame: FrameId) -> Result<(), TreeError> {
        if id == self.root {
            return Err(TreeError::RootMutation);
        }

        // Collect the fields we need up front to avoid borrow issues
        let (children_to_remove, parent, path) = {
            let entry = self.get(id).ok_or(TreeError::UnknownNode(id))?;
            (entry.children.clone(), entry.parent, entry.path.clone())
        };

        // Recursively remove children first (depth-first)
        for child_id in children_to_remove {
            self.remove_subtree(child_id, frame)?;
        }

        // Tombstone this entry
        let idx = id.0 as usize;
        if let Some(slot) = self.nodes.get_mut(idx) {
            if let Some(e) = slot.take() {
                // Remove from indices
                self.by_path.remove(&e.path);
                if let Some(name) = e.path.0.last().map(|seg| seg.name.clone()) {
                    if let Some(p) = e.parent {
                        self.by_sibling.remove(&(p, name));
                    }
                }
            }
        }

        // Remove from parent's children list and bump parent's children_ver
        if let Some(parent_id) = parent {
            if let Some(p) = self.get_mut(parent_id) {
                p.children.retain(|&cid| cid != id);
                p.children_ver = frame;
            }
        }

        // Also remove from by_path in case the entry was already tombstoned above
        self.by_path.remove(&path);

        Ok(())
    }

    /// Get the number of live entries (excludes tombstones).
    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|opt| opt.is_some()).count()
    }

    /// Returns true if the tree has no live entries (only possible if root was removed, which is forbidden).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the next id that would be allocated (for testing/debugging).
    pub fn next_id(&self) -> u32 {
        self.next_id
    }
}

#[cfg(test)]
mod tests {
    use super::NodeTree;
    use crate::artifact::ArtifactRef;
    use crate::tree::test_placeholder_spine;
    use alloc::vec::Vec;
    use lpc_model::{FrameId, NodeId, NodeName, TreePath};
    use lpc_source::{SrcArtifactSpec, SrcNodeConfig};
    use lpc_wire::{WireChildKind, WireSlotIndex};

    fn make_tree() -> NodeTree<()> {
        NodeTree::new(TreePath::parse("/root.show").unwrap(), FrameId::new(0))
    }

    fn spine_placeholder() -> (SrcNodeConfig, ArtifactRef) {
        test_placeholder_spine()
    }

    #[test]
    fn tree_add_child_stores_config_and_artifact() {
        let mut tree = make_tree();
        let root = tree.root();
        let cfg = SrcNodeConfig::new(SrcArtifactSpec(alloc::string::String::from("child.lp")));
        let art = ArtifactRef::from_raw(9);
        let child = tree
            .add_child(
                root,
                NodeName::parse("n").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg.clone(),
                art,
                FrameId::new(1),
            )
            .unwrap();
        let entry = tree.get(child).unwrap();
        assert_eq!(entry.config, cfg);
        assert_eq!(entry.artifact, art);
        assert!(entry.resolver_cache.is_empty());
    }

    #[test]
    fn tree_new_has_root() {
        let tree = make_tree();
        assert_eq!(tree.root(), NodeId::new(0));
        assert_eq!(tree.len(), 1);
        let root = tree.get(tree.root()).unwrap();
        assert!(root.parent.is_none());
        assert!(root.child_kind.is_none());
    }

    #[test]
    fn tree_add_child_increases_len() {
        let mut tree = make_tree();
        let root = tree.root();
        let (cfg, art) = spine_placeholder();
        let child = tree
            .add_child(
                root,
                NodeName::parse("fluid").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg,
                art,
                FrameId::new(1),
            )
            .unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(child, NodeId::new(1));

        let entry = tree.get(child).unwrap();
        assert_eq!(entry.parent, Some(root));
        assert!(entry.child_kind.is_some());
    }

    #[test]
    fn tree_add_child_bumps_parent_children_ver() {
        let mut tree = make_tree();
        let root = tree.root();
        let frame = FrameId::new(5);
        let (cfg, art) = spine_placeholder();
        tree.add_child(
            root,
            NodeName::parse("a").unwrap(),
            NodeName::parse("vis").unwrap(),
            WireChildKind::Sidecar {
                name: NodeName::parse("a").unwrap(),
            },
            cfg,
            art,
            frame,
        )
        .unwrap();
        let root_entry = tree.get(root).unwrap();
        assert_eq!(root_entry.children_ver.0, 5);
    }

    #[test]
    fn tree_sibling_name_collision_fails() {
        let mut tree = make_tree();
        let root = tree.root();
        let name = NodeName::parse("foo").unwrap();
        let ty = NodeName::parse("vis").unwrap();

        let (cfg1, art1) = spine_placeholder();
        tree.add_child(
            root,
            name.clone(),
            ty.clone(),
            WireChildKind::Sidecar { name: name.clone() },
            cfg1,
            art1,
            FrameId::new(1),
        )
        .unwrap();

        let (cfg2, art2) = spine_placeholder();
        let result = tree.add_child(
            root,
            name.clone(),
            ty,
            WireChildKind::Sidecar { name: name.clone() },
            cfg2,
            art2,
            FrameId::new(2),
        );
        assert!(result.is_err());
    }

    #[test]
    fn tree_lookup_path_finds_entry() {
        let mut tree = make_tree();
        let root = tree.root();
        let (cfg, art) = spine_placeholder();
        let child = tree
            .add_child(
                root,
                NodeName::parse("fluid").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg,
                art,
                FrameId::new(1),
            )
            .unwrap();

        let found = tree.lookup_path(&TreePath::parse("/root.show/fluid.vis").unwrap());
        assert_eq!(found, Some(child));
    }

    #[test]
    fn tree_lookup_sibling_finds_entry() {
        let mut tree = make_tree();
        let root = tree.root();
        let name = NodeName::parse("lfo").unwrap();
        let (cfg, art) = spine_placeholder();
        let child = tree
            .add_child(
                root,
                name.clone(),
                NodeName::parse("mod").unwrap(),
                WireChildKind::Sidecar { name: name.clone() },
                cfg,
                art,
                FrameId::new(1),
            )
            .unwrap();

        let found = tree.lookup_sibling(root, name);
        assert_eq!(found, Some(child));
    }

    #[test]
    fn tree_remove_subtree_tombstones_entry() {
        let mut tree = make_tree();
        let root = tree.root();
        let (cfg, art) = spine_placeholder();
        let child = tree
            .add_child(
                root,
                NodeName::parse("temp").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg,
                art,
                FrameId::new(1),
            )
            .unwrap();

        tree.remove_subtree(child, FrameId::new(2)).unwrap();
        assert!(tree.get(child).is_none());
        assert_eq!(tree.len(), 1); // Only root remains
    }

    #[test]
    fn tree_remove_subtree_bumps_parent_children_ver() {
        let mut tree = make_tree();
        let root = tree.root();
        let (cfg, art) = spine_placeholder();
        let child = tree
            .add_child(
                root,
                NodeName::parse("temp").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg,
                art,
                FrameId::new(1),
            )
            .unwrap();

        tree.remove_subtree(child, FrameId::new(10)).unwrap();
        let root_entry = tree.get(root).unwrap();
        assert_eq!(root_entry.children_ver.0, 10);
        assert!(root_entry.children.is_empty());
    }

    #[test]
    fn tree_cannot_remove_root() {
        let mut tree = make_tree();
        let result = tree.remove_subtree(tree.root(), FrameId::new(1));
        assert!(result.is_err());
    }

    #[test]
    fn tree_remove_subtree_is_depth_first() {
        let mut tree = make_tree();
        let root = tree.root();

        // Create grandchild -> child -> root chain
        let (cfg_p, art_p) = spine_placeholder();
        let child = tree
            .add_child(
                root,
                NodeName::parse("parent").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Sidecar {
                    name: NodeName::parse("parent").unwrap(),
                },
                cfg_p,
                art_p,
                FrameId::new(1),
            )
            .unwrap();

        let (cfg_g, art_g) = spine_placeholder();
        let grandchild = tree
            .add_child(
                child,
                NodeName::parse("nested").unwrap(),
                NodeName::parse("fx").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg_g,
                art_g,
                FrameId::new(2),
            )
            .unwrap();

        assert_eq!(tree.len(), 3);

        // Remove the middle node - should also remove grandchild
        tree.remove_subtree(child, FrameId::new(3)).unwrap();

        assert!(tree.get(child).is_none());
        assert!(tree.get(grandchild).is_none());
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn tree_entries_iterator_skips_tombstones() {
        let mut tree = make_tree();
        let root = tree.root();

        let (cfg_a, art_a) = spine_placeholder();
        let a = tree
            .add_child(
                root,
                NodeName::parse("a").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg_a,
                art_a,
                FrameId::new(1),
            )
            .unwrap();
        let (cfg_b, art_b) = spine_placeholder();
        let b = tree
            .add_child(
                root,
                NodeName::parse("b").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(1),
                },
                cfg_b,
                art_b,
                FrameId::new(2),
            )
            .unwrap();

        tree.remove_subtree(a, FrameId::new(3)).unwrap();

        let ids: Vec<NodeId> = tree.entries().map(|e| e.id).collect();
        assert_eq!(ids.len(), 2); // root + b
        assert!(ids.contains(&root));
        assert!(ids.contains(&b));
        assert!(!ids.contains(&a));
    }

    #[test]
    fn tree_next_id_never_reused() {
        let mut tree = make_tree();
        let root = tree.root();

        let (cfg_a, art_a) = spine_placeholder();
        let a = tree
            .add_child(
                root,
                NodeName::parse("a").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg_a,
                art_a,
                FrameId::new(1),
            )
            .unwrap();
        assert_eq!(a.0, 1);

        tree.remove_subtree(a, FrameId::new(2)).unwrap();

        let (cfg_b, art_b) = spine_placeholder();
        let b = tree
            .add_child(
                root,
                NodeName::parse("b").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg_b,
                art_b,
                FrameId::new(3),
            )
            .unwrap();
        // b should get a new id, not reuse 1
        assert_eq!(b.0, 2);
    }
}
