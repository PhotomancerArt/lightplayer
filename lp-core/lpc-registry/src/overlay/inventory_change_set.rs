//! Coarse project change sets between effective inventories.

use lpc_model::{
    AssetChange, AssetChangeKind, AssetChangeSet, AssetEntry, AssetState, NodeDefChange,
    NodeDefChangeKind, NodeDefEntry, NodeDefState, ProjectChangeSet, ProjectInventory,
};

pub(crate) fn change_set_between(
    before: &ProjectInventory,
    after: &ProjectInventory,
) -> ProjectChangeSet {
    ProjectChangeSet {
        defs: node_def_changes(before, after),
        assets: asset_changes(before, after),
    }
}

fn node_def_changes(
    before: &ProjectInventory,
    after: &ProjectInventory,
) -> lpc_model::NodeDefChangeSet {
    let mut changes = lpc_model::NodeDefChangeSet::default();

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

fn asset_changes(before: &ProjectInventory, after: &ProjectInventory) -> AssetChangeSet {
    let mut changes = AssetChangeSet::default();

    for location in after.assets.keys() {
        if !before.assets.contains_key(location) {
            changes.added.push(location.clone());
        }
    }
    for location in before.assets.keys() {
        if !after.assets.contains_key(location) {
            changes.removed.push(location.clone());
        }
    }
    for (location, before_entry) in &before.assets {
        let Some(after_entry) = after.assets.get(location) else {
            continue;
        };
        if let Some(kind) = classify_asset_change(before_entry, after_entry) {
            changes
                .changed
                .push(AssetChange::new(location.clone(), kind));
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
