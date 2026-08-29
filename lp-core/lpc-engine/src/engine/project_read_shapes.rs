//! Shape registry project read helpers.
//!
//! Shape entries are never materialized as a whole-registry snapshot: the
//! stream layer pulls them from [`Engine::iter_project_shape_entries`] one
//! entry at a time (sent and dropped before the next is cloned), so peak
//! memory is one `SlotShapeEntry`, not the whole shape forest (the classic's
//! project-read OOM faulted inside `SlotShape::clone` —
//! `docs/defects/2026-08-26-project-read-assembly-oom-resets-classic.md`).

use alloc::vec::Vec;

use lpc_model::{Revision, SlotShapeEntry, SlotShapeId};

use super::Engine;

impl Engine {
    /// Registry ids revision, for membership-sync gating.
    pub(super) fn project_shape_ids_revision(&self) -> Revision {
        self.slot_shapes().revision()
    }

    /// Lazily snapshot shape entries in merged static+dynamic id order, one
    /// entry cloned per step.
    ///
    /// Gating matches the old whole-snapshot `retain`: `since == 0` is a
    /// fresh/bulk read including every live entry (static catalog included);
    /// for `since > 0` inclusion is strictly `changed_at > since` (which
    /// excludes static-catalog entries — they report `Revision::default()`).
    pub(super) fn iter_project_shape_entries(
        &self,
        since: Revision,
    ) -> impl Iterator<Item = (SlotShapeId, SlotShapeEntry)> + '_ {
        let registry = self.slot_shapes();
        registry.ids_with_static_catalog().filter_map(move |id| {
            if since != Revision::default() {
                let changed_at = registry.entry_changed_at_with_static_catalog(id)?;
                if changed_at <= since {
                    return None;
                }
            }
            registry
                .entry_with_static_catalog(id)
                .map(|entry| (id, entry))
        })
    }

    /// Full current shape id set for membership sync — ids only, no entry
    /// clones.
    ///
    /// The stream emits this list (as `ProjectReadShapeEvent::Membership`) only
    /// when the registry's `ids_revision` is newer than the request `since`, so a
    /// client can prune any local shape whose id is absent. The list is the full
    /// live membership, including the static catalog, so it is authoritative.
    pub(super) fn project_shape_membership_ids(&self) -> Vec<SlotShapeId> {
        self.slot_shapes().ids_with_static_catalog().collect()
    }
}
