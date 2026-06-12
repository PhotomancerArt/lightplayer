//! Coarse project change summaries between effective inventories.

use lpc_model::{
    AssetChange, AssetChangeKind, AssetChangeSummary, AssetEntry, AssetState, NodeDefChange,
    NodeDefChangeKind, NodeDefEntry, NodeDefState, NodeUseChange, NodeUseChangeKind,
    NodeUseChangeSummary, ProjectChangeSummary, ProjectInventory, ProjectNode, ProjectNodeOrigin,
};

pub(crate) fn change_summary_between(
    before: &ProjectInventory,
    after: &ProjectInventory,
) -> ProjectChangeSummary {
    ProjectChangeSummary {
        defs: node_def_changes(before, after),
        assets: asset_changes(before, after),
        uses: node_use_changes(before, after),
    }
}

fn node_use_changes(before: &ProjectInventory, after: &ProjectInventory) -> NodeUseChangeSummary {
    let mut changes = NodeUseChangeSummary::default();

    for location in after.tree.nodes.keys() {
        if !before.tree.nodes.contains_key(location) {
            changes.added.push(location.clone());
        }
    }
    for location in before.tree.nodes.keys() {
        if !after.tree.nodes.contains_key(location) {
            changes.removed.push(location.clone());
        }
    }
    for (location, before_node) in &before.tree.nodes {
        let Some(after_node) = after.tree.nodes.get(location) else {
            continue;
        };
        if let Some(kind) = classify_node_use_change(before_node, after_node) {
            changes
                .changed
                .push(NodeUseChange::new(location.clone(), kind));
        }
    }

    changes
}

fn classify_node_use_change(
    before: &ProjectNode,
    after: &ProjectNode,
) -> Option<NodeUseChangeKind> {
    if before.parent != after.parent {
        return Some(NodeUseChangeKind::ParentChanged);
    }
    if before.def_location != after.def_location {
        return Some(NodeUseChangeKind::DefinitionChanged {
            from: before.def_location.clone(),
            to: after.def_location.clone(),
        });
    }
    if !same_node_use_origin(&before.origin, &after.origin) {
        return Some(NodeUseChangeKind::OriginChanged);
    }
    None
}

fn same_node_use_origin(before: &ProjectNodeOrigin, after: &ProjectNodeOrigin) -> bool {
    match (before, after) {
        (ProjectNodeOrigin::Root, ProjectNodeOrigin::Root) => true,
        (
            ProjectNodeOrigin::Invocation {
                slot: before_slot,
                role: before_role,
                ..
            },
            ProjectNodeOrigin::Invocation {
                slot: after_slot,
                role: after_role,
                ..
            },
        ) => before_slot == after_slot && before_role == after_role,
        _ => false,
    }
}

fn node_def_changes(
    before: &ProjectInventory,
    after: &ProjectInventory,
) -> lpc_model::NodeDefChangeSummary {
    let mut changes = lpc_model::NodeDefChangeSummary::default();

    for location in after.defs.keys() {
        if !before.defs.contains_key(location) {
            changes.added.push(location.clone());
        }
    }
    for location in before.defs.keys() {
        if !after.defs.contains_key(location) {
            changes.removed.push(location.clone());
        }
    }
    for (location, before_entry) in &before.defs {
        let Some(after_entry) = after.defs.get(location) else {
            continue;
        };
        if let Some(kind) = classify_node_def_change(before_entry, after_entry) {
            changes
                .changed
                .push(NodeDefChange::new(location.clone(), kind));
        }
    }

    changes
}

fn asset_changes(before: &ProjectInventory, after: &ProjectInventory) -> AssetChangeSummary {
    let mut changes = AssetChangeSummary::default();

    for source in after.assets.keys() {
        if !before.assets.contains_key(source) {
            changes.added.push(source.clone());
        }
    }
    for source in before.assets.keys() {
        if !after.assets.contains_key(source) {
            changes.removed.push(source.clone());
        }
    }
    for (source, before_entry) in &before.assets {
        let Some(after_entry) = after.assets.get(source) else {
            continue;
        };
        if let Some(kind) = classify_asset_change(before_entry, after_entry) {
            changes.changed.push(AssetChange::new(source.clone(), kind));
        }
    }

    changes
}

fn classify_node_def_change(
    before: &NodeDefEntry,
    after: &NodeDefEntry,
) -> Option<NodeDefChangeKind> {
    if before == after {
        return None;
    }

    match (&before.state, &after.state) {
        (NodeDefState::Loaded(before_def), NodeDefState::Loaded(after_def)) => {
            if before_def.kind() != after_def.kind() {
                Some(NodeDefChangeKind::KindChanged {
                    from: before_def.kind(),
                    to: after_def.kind(),
                })
            } else {
                Some(NodeDefChangeKind::Body)
            }
        }
        (NodeDefState::Loaded(_), _) => Some(NodeDefChangeKind::EnteredError),
        (_, NodeDefState::Loaded(_)) => Some(NodeDefChangeKind::LeftError),
        _ => Some(NodeDefChangeKind::EnteredError),
    }
}

fn classify_asset_change(before: &AssetEntry, after: &AssetEntry) -> Option<AssetChangeKind> {
    if before == after {
        return None;
    }

    match (&before.state, &after.state) {
        (AssetState::Available { .. }, AssetState::Available { .. }) => Some(AssetChangeKind::Body),
        (AssetState::Available { .. }, _) => Some(AssetChangeKind::EnteredError),
        (_, AssetState::Available { .. }) => Some(AssetChangeKind::LeftError),
        _ => Some(AssetChangeKind::EnteredError),
    }
}
