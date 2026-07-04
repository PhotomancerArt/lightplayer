//! Shape registry project read helpers.

use alloc::vec::Vec;

use lpc_model::{Revision, SlotShapeId};
use lpc_wire::{ReadLevel, ShapeReadQuery, ShapeReadResult};

use super::Engine;

impl Engine {
    pub(super) fn read_project_shapes(
        &self,
        query: ShapeReadQuery,
        since: Option<Revision>,
    ) -> ShapeReadResult {
        let since = since.unwrap_or_default();
        let registry = match query.level {
            ReadLevel::Ids | ReadLevel::Summary | ReadLevel::Detail => {
                let mut snapshot = self
                    .slot_shapes()
                    .snapshot_page_with_static_catalog(None, usize::MAX)
                    .0;
                // Gate entries by their per-entry `changed_at`. `since == 0` is a
                // fresh/bulk read, so every live entry is included (matches the
                // tree's `since==0` bulk-sync guard); for `since > 0` inclusion is
                // strictly `changed_at > since`.
                if since != Revision::default() {
                    snapshot.shapes.retain(|_, entry| entry.changed_at > since);
                }
                Some(snapshot)
            }
        };
        ShapeReadResult {
            level: query.level,
            registry,
        }
    }

    /// Full current shape id set for membership sync.
    ///
    /// The stream emits this list (as `ProjectReadShapeEvent::Membership`) only
    /// when the registry's `ids_revision` is newer than the request `since`, so a
    /// client can prune any local shape whose id is absent. The list is the full
    /// live membership, including the static catalog, so it is authoritative.
    pub(super) fn project_shape_membership_ids(&self) -> Vec<SlotShapeId> {
        self.slot_shapes()
            .snapshot_page_with_static_catalog(None, usize::MAX)
            .0
            .shapes
            .keys()
            .copied()
            .collect()
    }
}
