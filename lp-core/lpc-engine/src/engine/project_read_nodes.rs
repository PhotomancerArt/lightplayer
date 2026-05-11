//! Node-centric project read helpers.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{NodeId, SlotAccess};
use lpc_wire::{NodeReadQuery, NodeReadResult, WireSlotFullSync, WireSlotRootSnapshot};
use lpc_wire::{ReadLevel, snapshot_slot_root};

use crate::artifact::ArtifactState;
use crate::node::{NodeEntryState, tree_deltas_since};

use super::Engine;

impl Engine {
    pub(super) fn read_project_nodes(
        &self,
        since: Option<lpc_model::Revision>,
        query: NodeReadQuery,
    ) -> NodeReadResult {
        let since = since.unwrap_or_default();
        let tree_deltas = match query.level {
            ReadLevel::Ids | ReadLevel::Summary | ReadLevel::Detail => {
                tree_deltas_since(self.tree(), since)
            }
        };
        let slots = if query.include_slots && query.level == ReadLevel::Detail {
            Some(self.snapshot_node_slots())
        } else {
            None
        };

        NodeReadResult {
            level: query.level,
            tree_deltas,
            slots,
        }
    }

    fn snapshot_node_slots(&self) -> WireSlotFullSync {
        let mut roots = Vec::new();

        for entry in self.tree().entries() {
            if let Some(def) = self.loaded_node_def(entry.artifact()) {
                roots.push(WireSlotRootSnapshot {
                    name: node_def_root_name(entry.id),
                    shape: def.shape_id(),
                    data: snapshot_slot_root(&def.shape_id(), def.data(), self.slot_shapes()),
                });
            }

            if let NodeEntryState::Alive(node) = entry.state.value() {
                let state = node.runtime_state_slots();
                roots.push(WireSlotRootSnapshot {
                    name: node_state_root_name(entry.id),
                    shape: state.shape_id(),
                    data: snapshot_slot_root(&state.shape_id(), state.data(), self.slot_shapes()),
                });
            }
        }

        WireSlotFullSync {
            registry: self.slot_shapes().snapshot(),
            roots,
        }
    }

    fn loaded_node_def(
        &self,
        artifact: crate::artifact::ArtifactId,
    ) -> Option<&lpc_model::NodeDef> {
        let entry = self.artifacts().entry(&artifact)?;
        match &entry.state {
            ArtifactState::Loaded(def)
            | ArtifactState::Prepared(def)
            | ArtifactState::Idle(def) => Some(def),
            ArtifactState::Resolved
            | ArtifactState::ResolutionError(_)
            | ArtifactState::LoadError(_)
            | ArtifactState::PrepareError(_) => None,
        }
    }
}

fn node_def_root_name(id: NodeId) -> String {
    format!("node.{}.def", id.0)
}

fn node_state_root_name(id: NodeId) -> String {
    format!("node.{}.state", id.0)
}
