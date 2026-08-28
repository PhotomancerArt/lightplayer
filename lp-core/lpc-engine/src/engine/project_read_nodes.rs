//! Node-centric project read helpers.

use alloc::format;
use alloc::string::String;

use lpc_model::{NodeId, Revision, SlotAccess};
use lpc_registry::ProjectRegistry;
use lpc_wire::{WireSlotRootSnapshot, wire_slot_data_from_slot_access};

use crate::node::NodeEntryState;

use super::Engine;

impl Engine {
    /// Lazily snapshot slot roots, one root at a time, gated per-root by
    /// revision (M5 G6a).
    ///
    /// A root is included only when its owning revision is newer than `since`:
    /// `.def` roots gate on the node-def entry revision
    /// ([`lpc_model::NodeDefEntry::revision`]), `.state` roots gate on the node
    /// runtime entry `changed_at`. The whole [`WireSlotRootSnapshot`] is sent
    /// when the gate passes (no sub-root patching — that is M6). The `since == 0`
    /// bulk-sync guard includes every live root so a fresh read is complete.
    ///
    /// Each call to `next()` materializes at most one entry's roots; the
    /// caller is expected to send and drop each snapshot before pulling the
    /// next, keeping peak memory at one root's deep copy.
    pub(super) fn iter_node_slot_roots<'a>(
        &'a self,
        registry: &'a ProjectRegistry,
        since: Revision,
    ) -> impl Iterator<Item = WireSlotRootSnapshot> + 'a {
        self.tree().entries().flat_map(move |entry| {
            let def_root = if let Some(location) = entry.def_location.as_ref()
                && let Some(def_entry) = registry.def(location)
                && root_changed_since(since, def_entry.revision)
                && let lpc_model::NodeDefState::Loaded(def) = &def_entry.state
            {
                Some(WireSlotRootSnapshot {
                    name: node_def_root_name(entry.id),
                    shape: def.shape_id(),
                    data: wire_slot_data_from_slot_access(
                        self.slot_shapes(),
                        def.shape_id(),
                        def.data(),
                    ),
                })
            } else {
                None
            };

            let state_root = if root_changed_since(since, entry.changed_at())
                && let NodeEntryState::Alive(node) = entry.state.value()
                && let Some(state) = node.runtime_state_slots()
            {
                Some(WireSlotRootSnapshot {
                    name: node_state_root_name(entry.id),
                    shape: state.shape_id(),
                    data: wire_slot_data_from_slot_access(
                        self.slot_shapes(),
                        state.shape_id(),
                        state.data(),
                    ),
                })
            } else {
                None
            };

            def_root.into_iter().chain(state_root)
        })
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
