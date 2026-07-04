//! Node-centric project read helpers.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{NodeId, Revision, SlotAccess};
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
            Some(self.snapshot_node_slots(registry, since))
        } else {
            None
        };

        NodeReadResult {
            level: query.level,
            tree_deltas,
            slots,
        }
    }

    /// Snapshot slot roots, gated per-root by revision (M5 G6a).
    ///
    /// A root is included only when its owning revision is newer than `since`:
    /// `.def` roots gate on the node-def entry revision
    /// ([`lpc_model::NodeDefEntry::revision`]), `.state` roots gate on the node
    /// runtime entry `changed_at`. The whole [`WireSlotRootSnapshot`] is sent
    /// when the gate passes (no sub-root patching — that is M6). The `since == 0`
    /// bulk-sync guard includes every live root so a fresh read is complete.
    fn snapshot_node_slots(
        &self,
        registry: &ProjectRegistry,
        since: Revision,
    ) -> WireSlotRootsSnapshot {
        let mut roots = Vec::new();

        for entry in self.tree().entries() {
            if let Some(location) = entry.def_location.as_ref()
                && let Some(def_entry) = registry.def(location)
                && root_changed_since(since, def_entry.revision)
                && let lpc_model::NodeDefState::Loaded(def) = &def_entry.state
            {
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

            if root_changed_since(since, entry.changed_at())
                && let NodeEntryState::Alive(node) = entry.state.value()
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

/// Per-root inclusion test: a root's `revision` must be strictly newer than
/// `since`. The `since == 0` bulk-sync guard force-includes every live root so
/// default-stamped (revision-0) roots are not lost on a fresh read (matches
/// `tree_deltas_since`'s `since == 0` case).
fn root_changed_since(since: Revision, revision: Revision) -> bool {
    since.0 == 0 || revision.0 > since.0
}

pub(super) fn node_def_root_name(id: NodeId) -> String {
    format!("node.{}.def", id.0)
}

pub(super) fn node_state_root_name(id: NodeId) -> String {
    format!("node.{}.state", id.0)
}
