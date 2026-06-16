//! Node-centric project read helpers.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{NodeId, SlotAccess};
use lpc_registry::ProjectRegistry;
use lpc_wire::{
    NodeReadQuery, NodeReadResult, ReadLevel, WireSlotRootSnapshot, WireSlotRootsSnapshot,
    wire_slot_data_from_slot_access,
};

use crate::node::{NodeEntryState, tree_deltas_since};

use super::Engine;

impl Engine {
    pub(super) fn read_project_nodes(
        &self,
        registry: &ProjectRegistry,
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
            Some(self.snapshot_node_slots(registry))
        } else {
            None
        };

        NodeReadResult {
            level: query.level,
            tree_deltas,
            slots,
        }
    }

    fn snapshot_node_slots(&self, registry: &ProjectRegistry) -> WireSlotRootsSnapshot {
        let mut roots = Vec::new();

        for entry in self.tree().entries() {
            if let Some(def) = self.loaded_node_def_for_entry(registry, entry) {
                roots.push(WireSlotRootSnapshot {
                    name: node_def_root_name(entry.id),
                    shape: def.shape_id(),
                    data: wire_slot_data_from_slot_access(
                        self.slot_shapes(),
                        def.shape_id(),
                        def.data(),
                    ),
                });
            }

            if let NodeEntryState::Alive(node) = entry.state.value()
                && let Some(state) = node.runtime_state_slots()
            {
                roots.push(WireSlotRootSnapshot {
                    name: node_state_root_name(entry.id),
                    shape: state.shape_id(),
                    data: wire_slot_data_from_slot_access(
                        self.slot_shapes(),
                        state.shape_id(),
                        state.data(),
                    ),
                });
            }
        }

        WireSlotRootsSnapshot { roots }
    }
}

pub(super) fn node_def_root_name(id: NodeId) -> String {
    format!("node.{}.def", id.0)
}

pub(super) fn node_state_root_name(id: NodeId) -> String {
    format!("node.{}.state", id.0)
}
