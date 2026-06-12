//! Incremental runtime projection from registry project changes.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use lpc_model::{
    NodeDefChangeKind, NodeKind, NodeUseChangeKind, NodeUseLocation, ProjectChangeSummary,
};
use lpc_registry::ProjectRegistry;
use lpfs::LpFs;

use crate::nodes::CorePlaceholderNode;

use super::{Engine, ProjectLoadError, ProjectLoader};

/// Summary of runtime lifecycle work performed for one project apply.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeApplyResult {
    /// Runtime subtrees removed by node-use location.
    pub removed_nodes: Vec<NodeUseLocation>,
    /// Runtime nodes added by node-use location.
    pub added_nodes: Vec<NodeUseLocation>,
    /// Existing use locations rebuilt by remove/reproject.
    pub reattached_nodes: Vec<NodeUseLocation>,
    /// Node uses that could not be applied.
    pub failed_nodes: Vec<NodeUseLocation>,
}

impl RuntimeApplyResult {
    pub fn is_empty(&self) -> bool {
        self.removed_nodes.is_empty()
            && self.added_nodes.is_empty()
            && self.reattached_nodes.is_empty()
            && self.failed_nodes.is_empty()
    }
}

impl Engine {
    /// Apply registry changes to the current runtime projection.
    ///
    /// This is intentionally a lifecycle/topology operation. Same-kind
    /// definition body changes and asset body changes are value changes owned by
    /// runtime nodes through resolver/revision-aware reads.
    pub fn apply_project_changes(
        &mut self,
        fs: &dyn LpFs,
        registry: &mut ProjectRegistry,
        changes: &ProjectChangeSummary,
    ) -> Result<RuntimeApplyResult, ProjectLoadError> {
        if changes.is_empty() {
            self.resolver_mut().clear_frame_cache();
            self.project_runtime_index_mut()
                .rebuild_asset_consumers(&registry.inventory().tree);
            return Ok(RuntimeApplyResult::default());
        }

        let frame = lpc_model::current_revision();
        let mut remove_roots = BTreeSet::new();
        let mut add_targets = BTreeSet::new();
        let mut reattach_roots = BTreeSet::new();

        for removed in &changes.uses.removed {
            if let Some(parent) = playlist_parent_for_changed_child(registry, removed) {
                reattach_roots.insert(parent);
            } else {
                remove_roots.insert(removed.clone());
            }
        }

        for added in &changes.uses.added {
            if let Some(parent) = playlist_parent_for_changed_child(registry, added) {
                reattach_roots.insert(parent);
            } else {
                add_targets.insert(added.clone());
            }
        }

        for changed in &changes.uses.changed {
            match changed.kind {
                NodeUseChangeKind::DefinitionChanged { .. }
                | NodeUseChangeKind::ParentChanged
                | NodeUseChangeKind::OriginChanged => {
                    if let Some(parent) =
                        playlist_parent_for_changed_child(registry, &changed.location)
                    {
                        reattach_roots.insert(parent);
                    } else {
                        reattach_roots.insert(changed.location.clone());
                    }
                }
            }
        }

        for changed in &changes.defs.changed {
            match changed.kind {
                NodeDefChangeKind::KindChanged { .. }
                | NodeDefChangeKind::EnteredError
                | NodeDefChangeKind::LeftError => {
                    for node_id in self
                        .project_runtime_index()
                        .runtime_nodes_for_def(&changed.location)
                    {
                        if let Some(use_location) =
                            self.project_runtime_index().use_location(*node_id)
                        {
                            reattach_roots.insert(use_location.clone());
                        }
                    }
                }
                NodeDefChangeKind::Body => {}
            }
        }

        for root in &reattach_roots {
            remove_roots.insert(root.clone());
            add_subtree_targets(registry, root, &mut add_targets);
        }

        let mut result = RuntimeApplyResult::default();
        let mut removals = remove_roots.into_iter().collect::<Vec<_>>();
        removals.sort_by(|a, b| {
            b.segments
                .len()
                .cmp(&a.segments.len())
                .then_with(|| b.cmp(a))
        });
        for location in removals {
            if location.is_root() {
                if reattach_roots.contains(&location) {
                    self.reattach_runtime_node(
                        self.tree().root(),
                        alloc::boxed::Box::new(CorePlaceholderNode::new_leaf(NodeKind::Project)),
                        frame,
                    )
                    .map_err(|e| ProjectLoadError::InvalidSourcePath {
                        path: format_node_use(&location),
                        reason: format!("reattach runtime root: {e}"),
                    })?;
                    result.reattached_nodes.push(location);
                }
                continue;
            }
            let Some(node_id) = self.project_runtime_index().node_id(&location) else {
                continue;
            };
            self.remove_runtime_subtree(node_id, frame).map_err(|e| {
                ProjectLoadError::InvalidSourcePath {
                    path: format_node_use(&location),
                    reason: format!("remove runtime subtree: {e}"),
                }
            })?;
            if reattach_roots.contains(&location) {
                result.reattached_nodes.push(location);
            } else {
                result.removed_nodes.push(location);
            }
        }

        if !add_targets.is_empty() {
            let projected_nodes = ProjectLoader::ensure_runtime_spine(registry, self, frame)?;
            ProjectLoader::attach_selected_projected_nodes(
                fs,
                registry,
                self,
                &projected_nodes,
                &add_targets,
                frame,
            )?;
            for location in add_targets {
                if reattach_roots.contains(&location) {
                    if !result.reattached_nodes.contains(&location) {
                        result.reattached_nodes.push(location);
                    }
                } else {
                    result.added_nodes.push(location);
                }
            }
        }

        self.project_runtime_index_mut()
            .rebuild_asset_consumers(&registry.inventory().tree);
        self.resolver_mut().clear_frame_cache();
        Ok(result)
    }
}

fn playlist_parent_for_changed_child(
    registry: &ProjectRegistry,
    location: &NodeUseLocation,
) -> Option<NodeUseLocation> {
    let parent = parent_location(location)?;
    (node_kind_for_use(registry, &parent) == Some(NodeKind::Playlist)).then_some(parent)
}

fn parent_location(location: &NodeUseLocation) -> Option<NodeUseLocation> {
    let mut parent = location.clone();
    parent.segments.pop()?;
    Some(parent)
}

fn node_kind_for_use(registry: &ProjectRegistry, location: &NodeUseLocation) -> Option<NodeKind> {
    let node = registry.inventory().tree.nodes.get(location)?;
    registry.def(&node.def_location)?.state.kind()
}

fn add_subtree_targets(
    registry: &ProjectRegistry,
    root: &NodeUseLocation,
    targets: &mut BTreeSet<NodeUseLocation>,
) {
    for location in registry.inventory().tree.nodes.keys() {
        if is_same_or_descendant(root, location) {
            targets.insert(location.clone());
        }
    }
}

fn is_same_or_descendant(root: &NodeUseLocation, candidate: &NodeUseLocation) -> bool {
    candidate.segments.len() >= root.segments.len()
        && candidate
            .segments
            .iter()
            .zip(root.segments.iter())
            .all(|(candidate, root)| candidate == root)
}

fn format_node_use(location: &NodeUseLocation) -> alloc::string::String {
    if location.is_root() {
        return alloc::string::String::from("<root>");
    }
    location
        .segments
        .iter()
        .map(|segment| segment.slot.to_string())
        .collect::<Vec<_>>()
        .join("/")
}
