//! Join inputs for the dirty/edit state of config-slot DTOs.

use std::collections::BTreeMap;

use lpc_model::LpValue;

use crate::{PendingEdit, ProjectSlotAddress};

/// Per-address edit state consulted while projecting config-slot DTOs.
///
/// Built once per snapshot by `ProjectController` from its two edit-state
/// sources, consulted in order:
///
/// 1. the **edit buffer** (un-acked local edits) → value shadow plus
///    `Saving`/`Error` and `invalid` from a failed entry;
/// 2. the **overlay mirror** (server-acked pending edits, reverse-mapped from
///    `(artifact, path)` to slot addresses) → `Dirty` plus the assigned-value
///    shadow until the next project read reflects the edit;
/// 3. neither → `Clean`.
pub(in crate::app::project) struct SlotEditJoin<'a> {
    /// Un-acked local edits keyed by address (`ProjectController` buffer).
    buffer: Option<&'a BTreeMap<ProjectSlotAddress, PendingEdit>>,
    /// Server-acked pending edits from the overlay mirror. `Some(value)` for
    /// an assigned value (display shadow), `None` for other edit ops.
    overlay: BTreeMap<ProjectSlotAddress, Option<LpValue>>,
}

impl<'a> SlotEditJoin<'a> {
    /// A join with no edit state: every slot reads `Clean`.
    pub(in crate::app::project) fn empty() -> Self {
        Self {
            buffer: None,
            overlay: BTreeMap::new(),
        }
    }

    pub(in crate::app::project) fn new(
        buffer: &'a BTreeMap<ProjectSlotAddress, PendingEdit>,
        overlay: BTreeMap<ProjectSlotAddress, Option<LpValue>>,
    ) -> Self {
        Self {
            buffer: Some(buffer),
            overlay,
        }
    }

    /// The buffered (un-acked) edit for `address`, if any.
    pub(in crate::app::project) fn pending(
        &self,
        address: &ProjectSlotAddress,
    ) -> Option<&PendingEdit> {
        self.buffer?.get(address)
    }

    /// True when the overlay mirror holds a pending edit for `address`.
    pub(in crate::app::project) fn overlay_dirty(&self, address: &ProjectSlotAddress) -> bool {
        self.overlay.contains_key(address)
    }

    /// The value the DTO should display for `address`, if the edit state
    /// shadows the synced value: the buffered value first, else the overlay
    /// mirror's assigned value.
    pub(in crate::app::project) fn value_shadow(
        &self,
        address: &ProjectSlotAddress,
    ) -> Option<&LpValue> {
        if let Some(edit) = self.pending(address) {
            return Some(&edit.value);
        }
        self.overlay.get(address)?.as_ref()
    }
}
