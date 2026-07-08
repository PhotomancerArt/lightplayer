//! Join inputs for the dirty/edit state of config-slot DTOs.

use std::collections::{BTreeMap, BTreeSet};

use lpc_model::LpValue;
// The overlay mirror's op vocabulary (AssignValue/EnsurePresent/Remove) —
// distinct from the client's `crate::SlotEditOp` action enum.
use lpc_model::SlotEditOp;
use lpc_model::slot::SlotPersistence;
use lpc_model::{ArtifactLocation, AssetBodyOverlay};

use crate::{
    DirtySummary, PendingAssetEdit, PendingEdit, PendingEditPhase, ProjectNodeAddress,
    ProjectSlotAddress,
};

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
///
/// Value leaves consult their exact address only. Composite slots
/// additionally consult [`Self::state_under`], the **prefix-aware join**
/// (D4): edits strictly under their path surface on them, which is what
/// makes a *removed* map entry visible — the entry row is gone from the
/// effective def, but the parent map row reads dirty.
///
/// The join is also the single home of [`DirtySummary`] counting
/// ([`Self::dirty_summary_for_node`]): counts are per **edit entry**, never
/// per slot row, so they stay structurally correct when rows disappear.
pub(in crate::app::project) struct SlotEditJoin<'a> {
    /// Un-acked local edits keyed by address (`ProjectController` buffer).
    buffer: Option<&'a BTreeMap<ProjectSlotAddress, PendingEdit>>,
    /// Server-acked pending edits from the overlay mirror, keyed by address
    /// and carrying the mirrored op (`AssignValue` doubles as the display
    /// shadow; the structural ops shadow nothing).
    overlay: BTreeMap<ProjectSlotAddress, SlotEditOp>,
    /// Persistence classification for every buffer/overlay address, resolved
    /// by `ProjectController` through the shape-only policy walk
    /// (`lpc_model::resolve_slot_policy`), which works on data-less paths —
    /// so a removed entry with no surviving row still classifies correctly.
    persistence: BTreeMap<ProjectSlotAddress, SlotPersistence>,
    /// Asset body edits (buffer + overlay `ArtifactOverlay::Asset` mirror),
    /// keyed per owning node so [`Self::dirty_summary_for_node`] counts them
    /// alongside slot entries. See [`AssetEditKey`].
    assets: BTreeMap<AssetEditKey, AssetEditState<'a>>,
    /// Saved (base) display strings for overlay entries, reverse-mapped from
    /// the mirror's `(artifact, path)` base-value map exactly like `overlay`
    /// itself — so a key here always has an overlay entry, never a
    /// buffer-only one (the mirror only learns base values from acks and
    /// overlay reads). Feeds `UiPendingEdit::old_value` and
    /// `UiConfigSlot::old_value`.
    base_values: BTreeMap<ProjectSlotAddress, String>,
}

/// Join key for one asset body edit entry: the owning node when the edit's
/// artifact reverse-maps to one through the def-artifact map (an artifact
/// shared by several node uses joins once per use, like slot overlay edits),
/// else `None`. Unmapped entries — asset files that are not themselves a def
/// artifact, e.g. a shader's `.glsl` — have no node to count under, so they
/// aggregate at the project level instead
/// ([`SlotEditJoin::unmapped_asset_dirty_summary`]).
pub(in crate::app::project) type AssetEditKey = (Option<ProjectNodeAddress>, ArtifactLocation);

/// Join state for one [`AssetEditKey`]: the un-acked buffered edit and/or the
/// overlay mirror's acked body edit (the buffered state wins classification
/// on overlap, matching [`DirtySummary::for_asset`]'s join order).
#[derive(Default)]
pub(in crate::app::project) struct AssetEditState<'a> {
    /// The buffered (un-acked) asset edit, if any — carries the failure
    /// reason for `Failed` entries.
    pub pending: Option<&'a PendingAssetEdit>,
    /// The server-acked body edit from the overlay mirror, if any.
    pub acked: Option<&'a AssetBodyOverlay>,
}

/// One asset edit entry of the join ([`SlotEditJoin::asset_entries`]): the
/// unit asset [`DirtySummary`] counting and the save panel's asset rows are
/// built from, mirroring [`SlotEditEntry`] for slots.
pub(in crate::app::project) struct AssetEditEntry<'a> {
    /// The owning node, when the artifact reverse-maps to one.
    pub node: Option<&'a ProjectNodeAddress>,
    /// The artifact whose body is edited.
    pub artifact: &'a ArtifactLocation,
    /// The buffered (un-acked) edit at the artifact, if any.
    pub pending: Option<&'a PendingAssetEdit>,
    /// The server-acked body edit from the overlay mirror, if any.
    pub acked: Option<&'a AssetBodyOverlay>,
    /// The entry's [`DirtySummary`] classification (exactly one bucket).
    pub summary: DirtySummary,
}

impl AssetEditEntry<'_> {
    /// The replacement body's byte length for display: the buffered bytes
    /// when an entry is buffered, else the acked `ReplaceBody` bytes. `None`
    /// for an acked `Delete`, which carries no body.
    pub fn body_len(&self) -> Option<usize> {
        if let Some(pending) = self.pending {
            return Some(pending.bytes.len());
        }
        match self.acked? {
            AssetBodyOverlay::ReplaceBody(bytes) => Some(bytes.len()),
            AssetBodyOverlay::Delete => None,
        }
    }
}

/// Aggregate state of the edits strictly under a composite slot's path,
/// reported by [`SlotEditJoin::state_under`]. Attention-first precedence:
/// a failed descendant edit outranks an in-flight one outranks an acked one.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::app::project) enum PrefixEditState {
    /// A buffered descendant edit failed; the reason feeds the composite
    /// row's `invalid` (the dispatching row for gestures on paths that have
    /// no row of their own, e.g. a rejected map-entry add).
    Failed { reason: String },
    /// A buffered descendant edit is pending or in flight.
    Saving,
    /// The overlay mirror holds a descendant edit.
    Dirty,
}

/// One edit entry of the join ([`SlotEditJoin::entries`]): the unit both
/// [`DirtySummary`] counting and the save panel's change list are built from.
pub(in crate::app::project) struct SlotEditEntry<'a> {
    /// The entry's slot address.
    pub address: &'a ProjectSlotAddress,
    /// The buffered (un-acked) edit at the address, if any — carries the
    /// failure reason for `Failed` entries.
    pub pending: Option<&'a PendingEdit>,
    /// The entry's op for display, from the source that classifies it.
    pub op: SlotEditEntrySource<'a>,
    /// The entry's [`DirtySummary`] classification (exactly one bucket).
    pub summary: DirtySummary,
}

/// Where a [`SlotEditEntry`]'s op comes from: the buffered op wins over the
/// overlay mirror when an address is in both, matching
/// [`DirtySummary::for_slot`]'s join order.
pub(in crate::app::project) enum SlotEditEntrySource<'a> {
    /// An un-acked local edit (`Pending`/`InFlight`/`Failed`).
    Buffered(&'a crate::PendingEditOp),
    /// A server-acked edit from the overlay mirror.
    Acked(&'a SlotEditOp),
}

impl<'a> SlotEditJoin<'a> {
    /// A join with no edit state: every slot reads `Clean`.
    pub(in crate::app::project) fn empty() -> Self {
        Self {
            buffer: None,
            overlay: BTreeMap::new(),
            persistence: BTreeMap::new(),
            assets: BTreeMap::new(),
            base_values: BTreeMap::new(),
        }
    }

    pub(in crate::app::project) fn new(
        buffer: &'a BTreeMap<ProjectSlotAddress, PendingEdit>,
        overlay: BTreeMap<ProjectSlotAddress, SlotEditOp>,
        persistence: BTreeMap<ProjectSlotAddress, SlotPersistence>,
    ) -> Self {
        Self {
            buffer: Some(buffer),
            overlay,
            persistence,
            assets: BTreeMap::new(),
            base_values: BTreeMap::new(),
        }
    }

    /// Attach the asset body edit side of the join (buffer + overlay asset
    /// mirror, keyed per owning node by `ProjectController`).
    pub(in crate::app::project) fn with_assets(
        mut self,
        assets: BTreeMap<AssetEditKey, AssetEditState<'a>>,
    ) -> Self {
        self.assets = assets;
        self
    }

    /// Attach the saved (base) display strings for overlay entries.
    pub(in crate::app::project) fn with_base_values(
        mut self,
        base_values: BTreeMap<ProjectSlotAddress, String>,
    ) -> Self {
        self.base_values = base_values;
        self
    }

    /// Display string of the saved (base) value the edit entry at `address`
    /// replaces, when the mirror knows it. `None` degrades to the entry's
    /// kind-only display.
    pub(in crate::app::project) fn base_display(
        &self,
        address: &ProjectSlotAddress,
    ) -> Option<&str> {
        self.base_values.get(address).map(String::as_str)
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
    /// mirror's assigned value. Structural edits shadow nothing.
    pub(in crate::app::project) fn value_shadow(
        &self,
        address: &ProjectSlotAddress,
    ) -> Option<&LpValue> {
        if let Some(value) = self.pending(address).and_then(PendingEdit::value) {
            return Some(value);
        }
        match self.overlay.get(address)? {
            SlotEditOp::AssignValue(value) => Some(value),
            SlotEditOp::EnsurePresent | SlotEditOp::Remove => None,
        }
    }

    /// Prefix-aware join (D4) for composite slots: the aggregate state of
    /// every buffer/overlay edit whose address is **strictly under**
    /// `address` (the exact address is the caller's own join). `None` when
    /// nothing under the path is edited.
    pub(in crate::app::project) fn state_under(
        &self,
        address: &ProjectSlotAddress,
    ) -> Option<PrefixEditState> {
        let mut saving = false;
        if let Some(buffer) = self.buffer {
            for (entry_address, edit) in buffer {
                if !entry_address.is_strictly_under(address) {
                    continue;
                }
                match &edit.phase {
                    PendingEditPhase::Failed { reason } => {
                        return Some(PrefixEditState::Failed {
                            reason: reason.clone(),
                        });
                    }
                    // `AwaitingRefresh` counts as saving: the normalized
                    // gesture is acked but the synced view (and thus any
                    // surviving row) lags until the next applied read.
                    PendingEditPhase::Pending
                    | PendingEditPhase::InFlight { .. }
                    | PendingEditPhase::AwaitingRefresh => saving = true,
                }
            }
        }
        if saving {
            return Some(PrefixEditState::Saving);
        }
        self.overlay
            .keys()
            .any(|entry| entry.is_strictly_under(address))
            .then_some(PrefixEditState::Dirty)
    }

    /// Enumerate every edit entry in the join — the **single enumeration**
    /// both [`DirtySummary`] counting ([`Self::dirty_summary_for_node`]) and
    /// the save panel's change list (`ProjectController::pending_edits`)
    /// consume, so the list agrees with the counts by construction.
    ///
    /// One entry per address in the union of buffer and overlay keys, in
    /// address order (node, then root, then path). Each entry carries its op
    /// source (the buffered op wins when an address is in both — matching
    /// [`DirtySummary::for_slot`]'s join order) and its per-entry summary,
    /// which lands in exactly one bucket.
    pub(in crate::app::project) fn entries(&self) -> Vec<SlotEditEntry<'_>> {
        let addresses: BTreeSet<&ProjectSlotAddress> = self
            .buffer
            .map(|buffer| buffer.keys())
            .into_iter()
            .flatten()
            .chain(self.overlay.keys())
            .collect();
        addresses
            .into_iter()
            .map(|address| {
                let pending = self.pending(address);
                let op = match pending {
                    Some(edit) => SlotEditEntrySource::Buffered(&edit.op),
                    None => SlotEditEntrySource::Acked(
                        self.overlay
                            .get(address)
                            .expect("entry addresses come from the buffer or the overlay"),
                    ),
                };
                SlotEditEntry {
                    address,
                    pending,
                    op,
                    summary: DirtySummary::for_slot(
                        pending,
                        self.overlay_dirty(address),
                        self.entry_persistence(address),
                    ),
                }
            })
            .collect()
    }

    /// The [`DirtySummary`] of every edit entry addressed to `node` — the
    /// **single counting rule** for dirty aggregation (node headers, tree
    /// items, project totals, and the save panel all derive from it).
    ///
    /// Counts are per edit entry ([`Self::entries`]), classified by
    /// [`DirtySummary::for_slot`] exactly like the per-field affordances: a
    /// failed buffer entry → `failed`, anything else → its resolved
    /// persistence bucket. Each entry counts **once** regardless of whether
    /// a slot row survives at its path (a removed map entry still counts) —
    /// prefix-dirty on ancestor composites is display state, never an
    /// additional count.
    pub(in crate::app::project) fn dirty_summary_for_node(
        &self,
        node: &ProjectNodeAddress,
    ) -> DirtySummary {
        let slots: DirtySummary = self
            .entries()
            .into_iter()
            .filter(|entry| entry.address.node == *node)
            .map(|entry| entry.summary)
            .sum();
        let assets: DirtySummary = self
            .asset_entries()
            .into_iter()
            .filter(|entry| entry.node == Some(node))
            .map(|entry| entry.summary)
            .sum();
        slots + assets
    }

    /// Enumerate every asset body edit entry in the join — the single
    /// enumeration asset [`DirtySummary`] counting and the save panel's
    /// asset rows consume, mirroring [`Self::entries`] for slots.
    ///
    /// Stable order: node-mapped entries first (by node, then artifact),
    /// then unmapped entries (by artifact) — matching the save panel's
    /// convention of appending artifact-labeled rows after node rows.
    pub(in crate::app::project) fn asset_entries(&self) -> Vec<AssetEditEntry<'_>> {
        let mut entries: Vec<AssetEditEntry<'_>> = self
            .assets
            .iter()
            .map(|((node, artifact), state)| AssetEditEntry {
                node: node.as_ref(),
                artifact,
                pending: state.pending,
                acked: state.acked,
                summary: DirtySummary::for_asset(state.pending, state.acked.is_some()),
            })
            .collect();
        // `Option` orders `None` first; a stable sort moves unmapped entries
        // to the back while keeping the map's (node, artifact) order intact.
        entries.sort_by_key(|entry| entry.node.is_none());
        entries
    }

    /// The [`DirtySummary`] of every asset edit entry whose artifact maps to
    /// **no** synced node. These entries appear in the save panel with the
    /// artifact path as their label and must still count toward the project
    /// totals (an asset edit is persisted-class: it enables Save), so
    /// project-level aggregation adds this to the per-node sums.
    pub(in crate::app::project) fn unmapped_asset_dirty_summary(&self) -> DirtySummary {
        self.asset_entries()
            .into_iter()
            .filter(|entry| entry.node.is_none())
            .map(|entry| entry.summary)
            .sum()
    }

    /// Resolved persistence for an edit entry; unresolved addresses fall back
    /// to the default policy's bucket (persisted), the save-relevant default.
    fn entry_persistence(&self, address: &ProjectSlotAddress) -> SlotPersistence {
        self.persistence
            .get(address)
            .copied()
            .unwrap_or(SlotPersistence::Persisted)
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::{MutationCmdId, SlotPath};

    use crate::{PendingEditOp, PendingEditPhase, ProjectSlotRoot};

    use super::*;

    fn node() -> ProjectNodeAddress {
        ProjectNodeAddress::parse("/demo.project/pixels.fixture").unwrap()
    }

    fn at(path: &str) -> ProjectSlotAddress {
        ProjectSlotAddress::new(
            node(),
            ProjectSlotRoot::def(),
            SlotPath::parse(path).unwrap(),
        )
    }

    fn failed(op: PendingEditOp, reason: &str) -> PendingEdit {
        PendingEdit {
            op,
            phase: PendingEditPhase::Failed {
                reason: reason.to_string(),
            },
        }
    }

    #[test]
    fn state_under_sees_only_strict_descendants() {
        let buffer = BTreeMap::new();
        let overlay = BTreeMap::from([(at("entries[a]"), SlotEditOp::Remove)]);
        let join = SlotEditJoin::new(&buffer, overlay, BTreeMap::new());

        assert_eq!(
            join.state_under(&at("entries")),
            Some(PrefixEditState::Dirty)
        );
        assert_eq!(
            join.state_under(&ProjectSlotAddress::root(node(), ProjectSlotRoot::def())),
            Some(PrefixEditState::Dirty),
            "the def root is an ancestor of every def edit"
        );
        assert_eq!(
            join.state_under(&at("entries[a]")),
            None,
            "the exact address is the caller's own join, not a prefix hit"
        );
        assert_eq!(join.state_under(&at("other")), None);
    }

    #[test]
    fn state_under_prefers_failed_over_saving_over_dirty() {
        let buffer = BTreeMap::from([
            (
                at("entries[a]"),
                PendingEdit::pending_op(PendingEditOp::EnsurePresent),
            ),
            (
                at("entries[b]"),
                failed(PendingEditOp::EnsurePresent, "no such key shape"),
            ),
        ]);
        let overlay = BTreeMap::from([(at("entries[c]"), SlotEditOp::EnsurePresent)]);
        let join = SlotEditJoin::new(&buffer, overlay, BTreeMap::new());

        assert_eq!(
            join.state_under(&at("entries")),
            Some(PrefixEditState::Failed {
                reason: "no such key shape".to_string()
            }),
            "a failed descendant outranks in-flight and acked descendants"
        );

        let buffer = BTreeMap::from([(
            at("entries[a]"),
            PendingEdit {
                op: PendingEditOp::RemoveValue,
                phase: PendingEditPhase::InFlight {
                    cmd_id: MutationCmdId::new(1),
                },
            },
        )]);
        let overlay = BTreeMap::from([(at("entries[c]"), SlotEditOp::EnsurePresent)]);
        let join = SlotEditJoin::new(&buffer, overlay, BTreeMap::new());
        assert_eq!(
            join.state_under(&at("entries")),
            Some(PrefixEditState::Saving),
            "an in-flight descendant outranks an acked one"
        );
    }

    #[test]
    fn dirty_summary_counts_entries_once_including_rowless_removals() {
        // One overlay removal at a path with no surviving row, one buffered
        // failed edit, one address present in both buffer and overlay: three
        // entries, three counts — the buffer classification wins on overlap.
        let buffer = BTreeMap::from([
            (
                at("entries[b]"),
                failed(PendingEditOp::EnsurePresent, "rejected"),
            ),
            (at("brightness"), PendingEdit::pending(LpValue::F32(0.9))),
        ]);
        let overlay = BTreeMap::from([
            (at("entries[a]"), SlotEditOp::Remove),
            (at("brightness"), SlotEditOp::AssignValue(LpValue::F32(0.5))),
        ]);
        let persistence = BTreeMap::from([
            (at("entries[a]"), SlotPersistence::Persisted),
            (at("brightness"), SlotPersistence::Transient),
        ]);
        let join = SlotEditJoin::new(&buffer, overlay, persistence);

        assert_eq!(
            join.dirty_summary_for_node(&node()),
            DirtySummary {
                persisted: 1,
                transient: 1,
                failed: 1,
            }
        );
        assert!(
            join.dirty_summary_for_node(
                &ProjectNodeAddress::parse("/demo.project/clock.clock").unwrap()
            )
            .is_clean(),
            "entries only count for their own node"
        );
    }

    #[test]
    fn empty_join_reads_clean_everywhere() {
        let join = SlotEditJoin::empty();

        assert!(join.pending(&at("entries[a]")).is_none());
        assert!(!join.overlay_dirty(&at("entries[a]")));
        assert_eq!(join.state_under(&at("entries")), None);
        assert!(join.dirty_summary_for_node(&node()).is_clean());
        assert!(join.asset_entries().is_empty());
        assert!(join.unmapped_asset_dirty_summary().is_clean());
    }

    #[test]
    fn mapped_asset_edits_count_for_their_node_and_unmapped_for_the_project() {
        let buffer = BTreeMap::new();
        let glsl = ArtifactLocation::file("/shader.glsl");
        let def = ArtifactLocation::file("/orbit.shader.json");
        let mapped_body = AssetBodyOverlay::ReplaceBody(b"void main() {}".to_vec());
        let unmapped_body = AssetBodyOverlay::ReplaceBody(b"vec3 c;".to_vec());
        let assets = BTreeMap::from([
            (
                (Some(node()), def.clone()),
                AssetEditState {
                    pending: None,
                    acked: Some(&mapped_body),
                },
            ),
            (
                (None, glsl.clone()),
                AssetEditState {
                    pending: None,
                    acked: Some(&unmapped_body),
                },
            ),
        ]);
        let join = SlotEditJoin::new(&buffer, BTreeMap::new(), BTreeMap::new()).with_assets(assets);

        let one_persisted = DirtySummary {
            persisted: 1,
            transient: 0,
            failed: 0,
        };
        assert_eq!(join.dirty_summary_for_node(&node()), one_persisted);
        assert_eq!(join.unmapped_asset_dirty_summary(), one_persisted);
        assert!(
            join.dirty_summary_for_node(
                &ProjectNodeAddress::parse("/demo.project/clock.clock").unwrap()
            )
            .is_clean(),
            "asset entries only count for their owning node"
        );

        // Stable order: mapped entries first, unmapped appended.
        let entries = join.asset_entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].artifact, &def);
        assert_eq!(entries[0].node, Some(&node()));
        assert_eq!(entries[0].body_len(), Some(14));
        assert_eq!(entries[1].artifact, &glsl);
        assert_eq!(entries[1].node, None);
        assert_eq!(entries[1].body_len(), Some(7));
    }

    #[test]
    fn failed_buffered_asset_edit_outranks_the_acked_overlay_state() {
        let buffer = BTreeMap::new();
        let glsl = ArtifactLocation::file("/shader.glsl");
        let acked = AssetBodyOverlay::ReplaceBody(b"old".to_vec());
        let failed = PendingAssetEdit::failed(b"too big".to_vec(), "shader too large");
        let assets = BTreeMap::from([(
            (Some(node()), glsl.clone()),
            AssetEditState {
                pending: Some(&failed),
                acked: Some(&acked),
            },
        )]);
        let join = SlotEditJoin::new(&buffer, BTreeMap::new(), BTreeMap::new()).with_assets(assets);

        assert_eq!(
            join.dirty_summary_for_node(&node()),
            DirtySummary {
                persisted: 0,
                transient: 0,
                failed: 1,
            }
        );
        let entries = join.asset_entries();
        assert_eq!(
            entries[0].body_len(),
            Some(7),
            "the buffered bytes win the display length on overlap"
        );
        assert_eq!(
            entries[0].pending.unwrap().failure_reason(),
            Some("shader too large")
        );
    }

    #[test]
    fn base_display_answers_only_annotated_addresses() {
        let buffer = BTreeMap::new();
        let overlay = BTreeMap::from([
            (at("brightness"), SlotEditOp::AssignValue(LpValue::F32(0.9))),
            (at("entries[a]"), SlotEditOp::Remove),
        ]);
        let join = SlotEditJoin::new(&buffer, overlay, BTreeMap::new())
            .with_base_values(BTreeMap::from([(at("brightness"), "0.75".to_string())]));

        assert_eq!(join.base_display(&at("brightness")), Some("0.75"));
        assert_eq!(
            join.base_display(&at("entries[a]")),
            None,
            "unannotated entries degrade to kind-only display"
        );
        assert_eq!(SlotEditJoin::empty().base_display(&at("brightness")), None);
    }
}
