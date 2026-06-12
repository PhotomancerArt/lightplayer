//! Effective project inventory read envelopes.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{
    AssetEntry, NodeDefLocation, NodeDefState, NodeId, NodeKind, NodeUseLocation, ProjectInventory,
    ProjectNodeOrigin, ProjectNodePlacement, Revision, SlotPath,
};

/// Wire request for the current effective project inventory.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireProjectInventoryReadRequest;

/// Wire response containing a client-facing view of the current inventory.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireProjectInventoryReadResponse {
    pub defs: Vec<WireNodeDefInventoryEntry>,
    pub assets: Vec<AssetEntry>,
    pub nodes: Vec<WireProjectNodeInventoryEntry>,
}

impl WireProjectInventoryReadResponse {
    pub fn from_inventory(inventory: &ProjectInventory) -> Self {
        Self::from_inventory_with_runtime_ids(inventory, |_| None)
    }

    pub fn from_inventory_with_runtime_ids(
        inventory: &ProjectInventory,
        mut runtime_id_for: impl FnMut(&NodeUseLocation) -> Option<NodeId>,
    ) -> Self {
        let mut defs = inventory
            .defs
            .values()
            .map(WireNodeDefInventoryEntry::from_entry)
            .collect::<Vec<_>>();
        defs.sort_by(|a, b| a.location.cmp(&b.location));

        let mut assets = inventory.assets.values().cloned().collect::<Vec<_>>();
        assets.sort_by(|a, b| a.source.cmp(&b.source));

        let mut nodes = inventory
            .tree
            .nodes
            .values()
            .map(|node| WireProjectNodeInventoryEntry::from_node(node, runtime_id_for(&node.key)))
            .collect::<Vec<_>>();
        nodes.sort_by(|a, b| a.key.cmp(&b.key));

        Self {
            defs,
            assets,
            nodes,
        }
    }
}

/// Client-facing summary of one node definition inventory entry.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireNodeDefInventoryEntry {
    pub location: NodeDefLocation,
    pub state: WireNodeDefInventoryState,
    pub revision: Revision,
}

impl WireNodeDefInventoryEntry {
    fn from_entry(entry: &lpc_model::NodeDefEntry) -> Self {
        Self {
            location: entry.location.clone(),
            state: WireNodeDefInventoryState::from_state(&entry.state),
            revision: entry.revision,
        }
    }
}

/// Serializable summary of definition state.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum WireNodeDefInventoryState {
    Loaded { kind: NodeKind },
    NotFound,
    Deleted,
    ReadError { message: String },
    ParseError { message: String },
    ValidationError { message: String },
}

impl WireNodeDefInventoryState {
    fn from_state(state: &NodeDefState) -> Self {
        match state {
            NodeDefState::Loaded(def) => Self::Loaded { kind: def.kind() },
            NodeDefState::NotFound => Self::NotFound,
            NodeDefState::Deleted => Self::Deleted,
            NodeDefState::ReadError { message } => Self::ReadError {
                message: message.clone(),
            },
            NodeDefState::ParseError(error) => Self::ParseError {
                message: error.to_string(),
            },
            NodeDefState::ValidationError(error) => Self::ValidationError {
                message: error.message.clone(),
            },
        }
    }
}

/// Client-facing summary of one effective project node use.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireProjectNodeInventoryEntry {
    pub key: NodeUseLocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_id: Option<NodeId>,
    pub parent: Option<NodeUseLocation>,
    pub def_location: NodeDefLocation,
    pub origin: WireProjectNodeOrigin,
}

impl WireProjectNodeInventoryEntry {
    fn from_node(node: &lpc_model::ProjectNode, runtime_id: Option<NodeId>) -> Self {
        Self {
            key: node.key.clone(),
            runtime_id,
            parent: node.parent.clone(),
            def_location: node.def_location.clone(),
            origin: WireProjectNodeOrigin::from_origin(&node.origin),
        }
    }
}

/// Serializable summary of project node origin.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "origin")]
pub enum WireProjectNodeOrigin {
    Root,
    Invocation {
        slot: SlotPath,
        role: ProjectNodePlacement,
    },
}

impl WireProjectNodeOrigin {
    fn from_origin(origin: &ProjectNodeOrigin) -> Self {
        match origin {
            ProjectNodeOrigin::Root => Self::Root,
            ProjectNodeOrigin::Invocation { slot, role, .. } => Self::Invocation {
                slot: slot.clone(),
                role: role.clone(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_inventory_read_response_round_trips() {
        let response = WireProjectInventoryReadResponse::from_inventory(&ProjectInventory::new());

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireProjectInventoryReadResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
    }
}
