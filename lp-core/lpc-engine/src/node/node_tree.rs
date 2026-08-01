//! The node tree container: flat slot storage with path and sibling indices.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/01-tree.md` §NodeTree.

use alloc::vec::Vec;
use lp_collection::VecMap;
use lpc_model::{
    ChannelName, NodeId, NodeInvocation, NodeName, NodePathSegment, Revision, SlotPath, TreePath,
};
use lpc_wire::WireChildKind;

use crate::dataflow::binding::{BindingDraft, BindingEntry, BindingError, BindingRef};

use crate::node::node_binding_index::{NodeBindingIndex, binding_by_ref};
use crate::node::{RuntimeNodeEntry, TreeError};

/// The node tree container.
///
/// Generic over `N` — the payload type in `EntryState::Alive(N)`. In M3 this
/// is `()` (no Node trait yet). When the Node trait lands, this becomes
/// `Box<dyn Node>`.
#[derive(Debug)]
pub struct RuntimeNodeTree<N> {
    nodes: Vec<Option<RuntimeNodeEntry<N>>>,
    by_path: VecMap<TreePath, NodeId>,
    by_sibling: VecMap<(NodeId, NodeName), NodeId>,
    binding_index: NodeBindingIndex,
    next_id: u32,
    root: NodeId,
}

impl<N> RuntimeNodeTree<N> {
    /// Create a new tree with a root node at the given path and frame.
    pub fn new(root_path: TreePath, frame: Revision) -> Self {
        let root_id = NodeId::new(0);
        let root_entry = RuntimeNodeEntry::new(root_id, root_path.clone(), None, None, frame);

        let mut nodes = Vec::new();
        nodes.push(Some(root_entry));

        let mut by_path = VecMap::new();
        by_path.insert(root_path, root_id);

        Self {
            nodes,
            by_path,
            by_sibling: VecMap::new(),
            binding_index: NodeBindingIndex::default(),
            next_id: 1,
            root: root_id,
        }
    }

    /// Get the root node id.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Get a reference to an entry by id.
    pub fn get(&self, id: NodeId) -> Option<&RuntimeNodeEntry<N>> {
        self.nodes.get(id.0 as usize).and_then(|opt| opt.as_ref())
    }

    /// Get a mutable reference to an entry by id.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut RuntimeNodeEntry<N>> {
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
    pub fn entries(&self) -> impl Iterator<Item = &RuntimeNodeEntry<N>> {
        self.nodes.iter().filter_map(|opt| opt.as_ref())
    }

    /// Iterate over all live entries mutably (skips tombstones).
    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut RuntimeNodeEntry<N>> {
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
        _config: NodeInvocation,
        frame: Revision,
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
        let child_entry = RuntimeNodeEntry::new_spine(
            child_id,
            child_path.clone(),
            Some(parent),
            Some(child_kind),
            None,
            None,
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
            p.children.get_mut().push(child_id);
            p.children.mark_updated(frame);
        }

        Ok(child_id)
    }

    /// Remove a subtree (depth-first, children-first).
    ///
    /// Tombstones every descendant slot. Forbidden on root.
    pub fn remove_subtree(&mut self, id: NodeId, frame: Revision) -> Result<(), TreeError> {
        if id == self.root {
            return Err(TreeError::RootMutation);
        }

        // Collect the fields we need up front to avoid borrow issues
        let (children_to_remove, parent, path) = {
            let entry = self.get(id).ok_or(TreeError::UnknownNode(id))?;
            (
                entry.children.value().clone(),
                entry.parent,
                entry.path.clone(),
            )
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
                p.children.get_mut().retain(|&cid| cid != id);
                p.children.mark_updated(frame);
            }
        }

        // Also remove from by_path in case the entry was already tombstoned above
        self.by_path.remove(&path);
        self.rebuild_binding_index()
            .expect("removing bindings cannot introduce binding conflicts");

        Ok(())
    }

    pub(crate) fn subtree_ids_depth_first(&self, id: NodeId) -> Result<Vec<NodeId>, TreeError> {
        let entry = self.get(id).ok_or(TreeError::UnknownNode(id))?;
        let mut ids = Vec::new();
        for &child in entry.children.value() {
            ids.extend(self.subtree_ids_depth_first(child)?);
        }
        ids.push(id);
        Ok(ids)
    }

    /// Add one runtime binding to its owning node and update derived indexes.
    pub fn add_binding(
        &mut self,
        draft: BindingDraft,
        revision: Revision,
    ) -> Result<BindingRef, BindingError> {
        let owner = draft.owner;
        let index = self
            .get(owner)
            .ok_or(BindingError::UnknownOwner { owner })?
            .bindings
            .value()
            .len();
        let binding = BindingEntry {
            source: draft.source,
            target: draft.target,
            priority: draft.priority,
            kind: draft.kind,
            version: revision,
            owner,
        };

        let binding_ref = BindingRef::new(owner, index);
        self.binding_index.insert_binding(binding_ref, &binding)?;

        let entry = self
            .get_mut(owner)
            .expect("binding owner was validated before index insertion");
        let pushed = entry.bindings.get_mut().push(binding);
        debug_assert_eq!(pushed, index);
        entry.bindings.mark_updated(revision);

        Ok(binding_ref)
    }

    /// Remove every node-owned binding and reset the derived index. The
    /// loader's binding phase re-registers from defs afterwards, so the index
    /// always matches what a fresh load would produce (incremental binding
    /// apply, Option C).
    pub fn clear_bindings(&mut self, revision: Revision) {
        for entry in self.entries_mut() {
            if entry.bindings.value().is_empty() {
                continue;
            }
            entry.bindings.get_mut().clear();
            entry.bindings.mark_updated(revision);
        }
        self.binding_index = NodeBindingIndex::default();
    }

    /// Iterate over all node-owned bindings.
    pub fn bindings(&self) -> impl Iterator<Item = &BindingEntry> {
        self.entries()
            .flat_map(|entry| entry.bindings.value().iter())
    }

    /// Iterate over all node-owned bindings with their stable refs, in
    /// (owner, index) order.
    pub fn bindings_with_refs(&self) -> impl Iterator<Item = (BindingRef, &BindingEntry)> {
        self.entries().flat_map(|entry| {
            entry
                .bindings
                .value()
                .iter()
                .enumerate()
                .map(move |(index, binding)| (BindingRef::new(entry.id, index), binding))
        })
    }

    /// Resolve all consumers of a bus channel (bindings whose source is the
    /// channel).
    pub fn consumers_for_bus(&self, channel: &ChannelName) -> Vec<(BindingRef, &BindingEntry)> {
        self.binding_index
            .bus_sources(channel)
            .iter()
            .copied()
            .filter_map(|binding_ref| {
                binding_by_ref(&self.nodes, binding_ref).map(|entry| (binding_ref, entry))
            })
            .collect()
    }

    /// Every bus channel referenced by at least one binding, with its
    /// established kind.
    pub fn bus_channels(&self) -> impl Iterator<Item = (&ChannelName, lpc_model::Kind)> {
        self.binding_index.channels()
    }

    /// The bus scope `node` inhabits (modules.md R1). `None` for the root
    /// module — no scope contains it — and for unknown nodes. Answered
    /// from structural entry state, so it holds for `Pending`/`Failed`
    /// nodes and across payload reattach.
    pub fn scope_of(&self, node: NodeId) -> Option<crate::node::ScopeRef> {
        self.get(node).and_then(|entry| entry.scope)
    }

    /// The module scope `node` introduces around its project children,
    /// when it is a module-kinded node (the root always introduces the
    /// root scope). Playlist nodes introduce per-entry SINK scopes
    /// instead — those surface through their children's [`Self::scope_of`].
    pub fn scope_introduced_by(&self, node: NodeId) -> Option<crate::node::ScopeRef> {
        self.get(node)
            .filter(|entry| entry.introduces_scope)
            .map(|entry| crate::node::ScopeRef::Module { owner: entry.id })
    }

    /// Every scope in the tree: each introducer's module scope plus every
    /// sink scope some node inhabits. Sorted and deduplicated so callers
    /// get a stable listing.
    pub fn scopes(&self) -> Vec<crate::node::ScopeRef> {
        let mut scopes = Vec::new();
        for entry in self.entries() {
            if entry.introduces_scope {
                scopes.push(crate::node::ScopeRef::Module { owner: entry.id });
            }
            if let Some(scope @ crate::node::ScopeRef::Sink { .. }) = entry.scope {
                scopes.push(scope);
            }
        }
        scopes.sort();
        scopes.dedup();
        scopes
    }

    /// The channels listed in `scope`: every bus channel named by a
    /// binding whose owner inhabits it (R3 — a public slot's channel
    /// exists in the slot's scope; R4 — a module node's own endpoints land
    /// in its PARENT scope, which is the scope it inhabits). Listing only:
    /// resolution semantics stay unscoped until the scoped-channel phase.
    pub fn scope_channels(
        &self,
        scope: crate::node::ScopeRef,
    ) -> Vec<(ChannelName, lpc_model::Kind)> {
        let mut channels: Vec<(ChannelName, lpc_model::Kind)> = Vec::new();
        for binding in self.bindings() {
            if self.scope_of(binding.owner) != Some(scope) {
                continue;
            }
            let named = match (&binding.source, &binding.target) {
                (crate::dataflow::binding::BindingSource::BusChannel(channel), _) => Some(channel),
                (_, crate::dataflow::binding::BindingTarget::BusChannel(channel)) => Some(channel),
                _ => None,
            };
            let Some(channel) = named else { continue };
            if !channels.iter().any(|(existing, _)| existing == channel) {
                channels.push((channel.clone(), binding.kind));
            }
        }
        channels.sort_by(|(a, _), (b, _)| a.cmp(b));
        channels
    }

    /// The bus scope `node` writes into and reads from: its inhabited
    /// scope (R4 — produces write locally; module nodes reside in their
    /// PARENT scope, so their own endpoints land there). The root module
    /// inhabits nothing and operates in the scope it introduces.
    pub fn node_scope(&self, node: NodeId) -> Option<crate::node::ScopeRef> {
        let entry = self.get(node)?;
        if let Some(scope) = entry.scope {
            return Some(scope);
        }
        entry
            .introduces_scope
            .then_some(crate::node::ScopeRef::Module { owner: entry.id })
    }

    /// The scope enclosing `scope`: for a module scope, the scope its
    /// owner inhabits; for a playlist entry's sink scope, the playlist's
    /// scope. `None` at the root.
    pub fn parent_scope(&self, scope: crate::node::ScopeRef) -> Option<crate::node::ScopeRef> {
        match scope {
            crate::node::ScopeRef::Module { owner } => self.get(owner)?.scope,
            crate::node::ScopeRef::Sink { owner, .. } => self.node_scope(owner),
        }
    }

    /// Writer-shadowing provider lookup (R5): the provider set for a bus
    /// read performed from `scope` is the nearest enclosing scope with at
    /// least one provider for the channel, walking outward from `scope` to
    /// the root. Demand never walks INTO a scope, which is what keeps sink
    /// scopes invisible to enclosing readers by construction (R2). A
    /// scopeless read (`None`) answers with the flat provider set — the
    /// pre-scope behavior test fakes rely on.
    pub fn providers_for_bus_read(
        &self,
        scope: Option<crate::node::ScopeRef>,
        channel: &ChannelName,
    ) -> Vec<(BindingRef, &BindingEntry)> {
        let Some(mut scope) = scope else {
            return self.providers_for_bus(channel);
        };
        loop {
            let candidates: Vec<(BindingRef, &BindingEntry)> = self
                .binding_index
                .bus_targets(channel)
                .iter()
                .copied()
                .filter_map(|binding_ref| {
                    binding_by_ref(&self.nodes, binding_ref).map(|entry| (binding_ref, entry))
                })
                .filter(|(_, entry)| self.node_scope(entry.owner) == Some(scope))
                .collect();
            if !candidates.is_empty() {
                return candidates;
            }
            match self.parent_scope(scope) {
                Some(parent) => scope = parent,
                None => return Vec::new(),
            }
        }
    }

    /// The stable persisted identity of `scope` (`<scope-path>` — see
    /// [`crate::node::ScopeRef::persist_path`] for the stability
    /// rationale). `None` when the owner is unknown.
    pub fn scope_persist_path(
        &self,
        scope: crate::node::ScopeRef,
    ) -> Option<alloc::string::String> {
        self.get(scope.owner())
            .map(|entry| scope.persist_path(&entry.path))
    }

    /// Resolve the binding for one consumed slot, if one exists.
    ///
    /// When multiple owners bind the same consumed slot, the owner closest to
    /// the root wins. This keeps project-level defaults authoritative while
    /// leaving room for deeper node-local overrides later.
    pub fn binding_for_consumed_slot(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Option<(BindingRef, &BindingEntry)> {
        self.bindings_for_consumed_slot(node, slot)
            .into_iter()
            .next()
    }

    /// Resolve all bindings for one consumed slot at the winning owner depth.
    ///
    /// Multiple bindings owned at the same depth are meaningful for mergeable
    /// aggregate receivers. Bindings owned deeper in the tree are treated as
    /// overrides and ignored when a shallower owner binds the same consumed slot.
    pub fn bindings_for_consumed_slot(
        &self,
        node: NodeId,
        slot: &SlotPath,
    ) -> Vec<(BindingRef, &BindingEntry)> {
        let mut candidates: Vec<_> = self
            .binding_index
            .consumed_targets(node, slot)
            .iter()
            .copied()
            .filter_map(|binding_ref| {
                let depth = self
                    .get(binding_ref.owner)
                    .map(|entry| entry.path.0.len())
                    .unwrap_or(usize::MAX);
                binding_by_ref(&self.nodes, binding_ref).map(|entry| (depth, binding_ref, entry))
            })
            .collect();
        let Some(min_depth) = candidates.iter().map(|(depth, _, _)| *depth).min() else {
            return Vec::new();
        };
        candidates.retain(|(depth, _, _)| *depth == min_depth);
        candidates.sort_by_key(|(_, binding_ref, _)| *binding_ref);
        candidates
            .into_iter()
            .map(|(_, binding_ref, entry)| (binding_ref, entry))
            .collect()
    }

    /// Resolve all providers for a bus channel.
    pub fn providers_for_bus(&self, channel: &ChannelName) -> Vec<(BindingRef, &BindingEntry)> {
        self.binding_index
            .bus_targets(channel)
            .iter()
            .copied()
            .filter_map(|binding_ref| {
                binding_by_ref(&self.nodes, binding_ref).map(|entry| (binding_ref, entry))
            })
            .collect()
    }

    fn rebuild_binding_index(&mut self) -> Result<(), BindingError> {
        self.binding_index = NodeBindingIndex::rebuild(&self.nodes)?;
        Ok(())
    }

    /// A cheap summary of the tree's *shape*, for catching structural changes
    /// that forgot to invalidate resolution.
    ///
    /// Node count, binding count and the newest binding revision together move
    /// whenever the binding graph or the topology does. This is not a hash and
    /// does not need to be: it is compared against its own previous value one
    /// frame later, by a debug-only assertion, to answer "did the graph change
    /// without saying so?".
    #[cfg(debug_assertions)]
    pub fn structural_fingerprint(&self) -> (usize, usize, Revision) {
        let mut nodes = 0;
        let mut bindings = 0;
        let mut newest = Revision::default();
        for entry in self.entries() {
            nodes += 1;
            bindings += entry.bindings.value().len();
            newest = core::cmp::max(newest, entry.bindings.changed_at());
        }
        (nodes, bindings, newest)
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
    use super::RuntimeNodeTree;
    use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
    use crate::node::test_placeholder_spine;
    use alloc::string::String;
    use alloc::vec::Vec;
    use lpc_model::{ArtifactSpec, NodeInvocation};
    use lpc_model::{ChannelName, Kind, LpValue, NodeId, NodeName, Revision, SlotPath, TreePath};
    use lpc_wire::{WireChildKind, WireSlotIndex};

    fn make_tree() -> RuntimeNodeTree<()> {
        RuntimeNodeTree::new(TreePath::parse("/root.show").unwrap(), Revision::new(0))
    }

    fn spine_placeholder() -> NodeInvocation {
        test_placeholder_spine()
    }

    fn add_test_child(tree: &mut RuntimeNodeTree<()>, name: &str) -> NodeId {
        let root = tree.root();
        let cfg = spine_placeholder();
        tree.add_child(
            root,
            NodeName::parse(name).unwrap(),
            NodeName::parse("node").unwrap(),
            WireChildKind::Input {
                source: WireSlotIndex(0),
            },
            cfg,
            Revision::new(1),
        )
        .unwrap()
    }

    #[test]
    fn tree_add_child_stores_child_metadata() {
        let mut tree = make_tree();
        let root = tree.root();
        let cfg = NodeInvocation::new(ArtifactSpec::path("child.lp"));
        let child = tree
            .add_child(
                root,
                NodeName::parse("n").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg.clone(),
                Revision::new(1),
            )
            .unwrap();
        let entry = tree.get(child).unwrap();
        assert_eq!(entry.parent, Some(root));
        assert_eq!(
            entry.path,
            TreePath::parse("/root.show/n.vis").expect("child path")
        );
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
        let cfg = spine_placeholder();
        let child = tree
            .add_child(
                root,
                NodeName::parse("fluid").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg,
                Revision::new(1),
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
        let frame = Revision::new(5);
        let cfg = spine_placeholder();
        tree.add_child(
            root,
            NodeName::parse("a").unwrap(),
            NodeName::parse("vis").unwrap(),
            WireChildKind::Sidecar {
                name: NodeName::parse("a").unwrap(),
            },
            cfg,
            frame,
        )
        .unwrap();
        let root_entry = tree.get(root).unwrap();
        assert_eq!(root_entry.children_changed_at().0, 5);
    }

    #[test]
    fn tree_owns_and_indexes_runtime_bindings() {
        let mut tree = make_tree();
        let shader = add_test_child(&mut tree, "shader");
        let fixture = add_test_child(&mut tree, "fixture");
        let channel = ChannelName(String::from("visual"));
        let out = SlotPath::parse("output").unwrap();
        let input = SlotPath::parse("input").unwrap();

        tree.add_binding(
            BindingDraft {
                source: BindingSource::ProducedSlot {
                    node: shader,
                    slot: out,
                },
                target: BindingTarget::BusChannel(channel.clone()),
                priority: BindingPriority::new(0),
                kind: Kind::Color,
                owner: shader,
            },
            Revision::new(2),
        )
        .unwrap();
        tree.add_binding(
            BindingDraft {
                source: BindingSource::BusChannel(channel.clone()),
                target: BindingTarget::ConsumedSlot {
                    node: fixture,
                    slot: input.clone(),
                },
                priority: BindingPriority::new(0),
                kind: Kind::Color,
                owner: fixture,
            },
            Revision::new(3),
        )
        .unwrap();

        assert_eq!(tree.providers_for_bus(&channel).len(), 1);
        let (binding_ref, binding) = tree
            .binding_for_consumed_slot(fixture, &input)
            .expect("fixture input binding");
        assert_eq!(binding_ref.owner, fixture);
        assert!(matches!(binding.source, BindingSource::BusChannel(_)));
        assert_eq!(binding.version, Revision::new(3));
    }

    #[test]
    fn tree_allows_duplicate_bus_provider_priority_for_merge_consumers() {
        let mut tree = make_tree();
        let a = add_test_child(&mut tree, "a");
        let b = add_test_child(&mut tree, "b");
        let channel = ChannelName(String::from("visual"));

        let draft = |owner| BindingDraft {
            source: BindingSource::Literal(LpValue::F32(1.0)),
            target: BindingTarget::BusChannel(channel.clone()),
            priority: BindingPriority::new(0),
            kind: Kind::Color,
            owner,
        };

        tree.add_binding(draft(a), Revision::new(2)).unwrap();
        tree.add_binding(draft(b), Revision::new(3)).unwrap();
        assert_eq!(tree.providers_for_bus(&channel).len(), 2);
    }

    #[test]
    fn tree_sibling_name_collision_fails() {
        let mut tree = make_tree();
        let root = tree.root();
        let name = NodeName::parse("foo").unwrap();
        let ty = NodeName::parse("vis").unwrap();

        let cfg1 = spine_placeholder();
        tree.add_child(
            root,
            name.clone(),
            ty.clone(),
            WireChildKind::Sidecar { name: name.clone() },
            cfg1,
            Revision::new(1),
        )
        .unwrap();

        let cfg2 = spine_placeholder();
        let result = tree.add_child(
            root,
            name.clone(),
            ty,
            WireChildKind::Sidecar { name: name.clone() },
            cfg2,
            Revision::new(2),
        );
        assert!(result.is_err());
    }

    #[test]
    fn tree_lookup_path_finds_entry() {
        let mut tree = make_tree();
        let root = tree.root();
        let cfg = spine_placeholder();
        let child = tree
            .add_child(
                root,
                NodeName::parse("fluid").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg,
                Revision::new(1),
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
        let cfg = spine_placeholder();
        let child = tree
            .add_child(
                root,
                name.clone(),
                NodeName::parse("mod").unwrap(),
                WireChildKind::Sidecar { name: name.clone() },
                cfg,
                Revision::new(1),
            )
            .unwrap();

        let found = tree.lookup_sibling(root, name);
        assert_eq!(found, Some(child));
    }

    #[test]
    fn tree_remove_subtree_tombstones_entry() {
        let mut tree = make_tree();
        let root = tree.root();
        let cfg = spine_placeholder();
        let child = tree
            .add_child(
                root,
                NodeName::parse("temp").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg,
                Revision::new(1),
            )
            .unwrap();

        tree.remove_subtree(child, Revision::new(2)).unwrap();
        assert!(tree.get(child).is_none());
        assert_eq!(tree.len(), 1); // Only root remains
    }

    #[test]
    fn tree_remove_subtree_bumps_parent_children_ver() {
        let mut tree = make_tree();
        let root = tree.root();
        let cfg = spine_placeholder();
        let child = tree
            .add_child(
                root,
                NodeName::parse("temp").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg,
                Revision::new(1),
            )
            .unwrap();

        tree.remove_subtree(child, Revision::new(10)).unwrap();
        let root_entry = tree.get(root).unwrap();
        assert_eq!(root_entry.children_changed_at().0, 10);
        assert!(root_entry.children.value().is_empty());
    }

    #[test]
    fn tree_cannot_remove_root() {
        let mut tree = make_tree();
        let result = tree.remove_subtree(tree.root(), Revision::new(1));
        assert!(result.is_err());
    }

    #[test]
    fn tree_remove_subtree_is_depth_first() {
        let mut tree = make_tree();
        let root = tree.root();

        // Create grandchild -> child -> root chain
        let cfg_p = spine_placeholder();
        let child = tree
            .add_child(
                root,
                NodeName::parse("parent").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Sidecar {
                    name: NodeName::parse("parent").unwrap(),
                },
                cfg_p,
                Revision::new(1),
            )
            .unwrap();

        let cfg_g = spine_placeholder();
        let grandchild = tree
            .add_child(
                child,
                NodeName::parse("nested").unwrap(),
                NodeName::parse("fx").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg_g,
                Revision::new(2),
            )
            .unwrap();

        assert_eq!(tree.len(), 3);

        // Remove the middle node - should also remove grandchild
        tree.remove_subtree(child, Revision::new(3)).unwrap();

        assert!(tree.get(child).is_none());
        assert!(tree.get(grandchild).is_none());
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn tree_entries_iterator_skips_tombstones() {
        let mut tree = make_tree();
        let root = tree.root();

        let cfg_a = spine_placeholder();
        let a = tree
            .add_child(
                root,
                NodeName::parse("a").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg_a,
                Revision::new(1),
            )
            .unwrap();
        let cfg_b = spine_placeholder();
        let b = tree
            .add_child(
                root,
                NodeName::parse("b").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(1),
                },
                cfg_b,
                Revision::new(2),
            )
            .unwrap();

        tree.remove_subtree(a, Revision::new(3)).unwrap();

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

        let cfg_a = spine_placeholder();
        let a = tree
            .add_child(
                root,
                NodeName::parse("a").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg_a,
                Revision::new(1),
            )
            .unwrap();
        assert_eq!(a.0, 1);

        tree.remove_subtree(a, Revision::new(2)).unwrap();

        let cfg_b = spine_placeholder();
        let b = tree
            .add_child(
                root,
                NodeName::parse("b").unwrap(),
                NodeName::parse("vis").unwrap(),
                WireChildKind::Input {
                    source: WireSlotIndex(0),
                },
                cfg_b,
                Revision::new(3),
            )
            .unwrap();
        // b should get a new id, not reuse 1
        assert_eq!(b.0, 2);
    }
}
