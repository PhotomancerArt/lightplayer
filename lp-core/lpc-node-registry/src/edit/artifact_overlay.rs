//! Address-keyed pending artifact edits (slot upserts and asset replacements).

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use lpc_model::{NodeDef, SlotPath};

use crate::ArtifactLoc;

use super::SlotEdit;

/// In-memory map of current pending edits keyed by [`ArtifactLoc`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtifactOverlay {
    edits: BTreeMap<ArtifactLoc, ArtifactEdits>,
}

/// Pending edits for one artifact location.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtifactEdits {
    /// Pending slot ops in apply order. Same [`SlotEdit::path`] upserts in place.
    slot_edits: Vec<SlotEdit>,
    pub asset_edit: AssetEdit,
}

/// Pending asset body or deletion for one artifact.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum AssetEdit {
    #[default]
    None,
    Delete,
    ReplaceBody(Vec<u8>),
}

impl ArtifactEdits {
    /// Insert or replace the pending edit; clears asset pending.
    pub fn upsert_slot(&mut self, edit: SlotEdit) {
        self.asset_edit = AssetEdit::None;
        let target = edit.path().clone();
        let clear_scopes = structural_clear_scopes(&edit);
        let clears_ancestor_remove = matches!(
            edit,
            SlotEdit::EnsurePresent { .. } | SlotEdit::AssignValue { .. }
        );
        self.slot_edits.retain(|existing| {
            if existing.path() == &target {
                return false;
            }
            if clear_scopes
                .iter()
                .any(|scope| is_strict_ancestor(scope, existing.path()))
            {
                return false;
            }
            if clears_ancestor_remove
                && matches!(existing, SlotEdit::Remove { path } if is_strict_ancestor(path, &target))
            {
                return false;
            }
            true
        });

        if matches!(edit, SlotEdit::Remove { .. })
            && self
                .slot_edits
                .iter()
                .any(|existing| matches!(existing, SlotEdit::Remove { path } if is_strict_ancestor(path, &target)))
        {
            return;
        }
        self.slot_edits.push(edit);
    }

    /// Set asset pending state; clears all slot edits.
    pub fn set_asset(&mut self, asset: AssetEdit) {
        self.asset_edit = asset;
        self.slot_edits.clear();
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.asset_edit, AssetEdit::None) && self.slot_edits.is_empty()
    }

    pub fn slot_edits(&self) -> impl Iterator<Item = &SlotEdit> {
        self.slot_edits.iter()
    }

    pub(crate) fn slot_edits_is_empty(&self) -> bool {
        self.slot_edits.is_empty()
    }

    pub fn asset_pending(&self) -> &AssetEdit {
        &self.asset_edit
    }

    pub fn has_pending_at_path(&self, path: &SlotPath) -> bool {
        self.slot_edits.iter().any(|edit| edit.path() == path)
    }

    /// Merge pending edits from `other`, preserving upsert semantics.
    pub fn merge_from(&mut self, other: &ArtifactEdits) {
        for op in other.slot_edits() {
            self.upsert_slot(op.clone());
        }
        if !matches!(other.asset_pending(), AssetEdit::None) {
            self.set_asset(other.asset_pending().clone());
        }
    }
}

fn structural_clear_scopes(edit: &SlotEdit) -> Vec<SlotPath> {
    match edit {
        SlotEdit::Remove { path } => alloc::vec![path.clone()],
        SlotEdit::EnsurePresent { path } => {
            let mut scopes = alloc::vec![path.clone()];
            if ensure_present_clears_parent_scope(path) {
                scopes.push(parent_path(path));
            }
            scopes
        }
        SlotEdit::AssignValue { .. } => Vec::new(),
    }
}

fn ensure_present_clears_parent_scope(path: &SlotPath) -> bool {
    match path.segments() {
        [lpc_model::SlotPathSegment::Field(name)] => NodeDef::is_variant_name(name.as_str()),
        [.., lpc_model::SlotPathSegment::Field(_)] => true,
        _ => false,
    }
}

fn parent_path(path: &SlotPath) -> SlotPath {
    let Some((_, parent)) = path.segments().split_last() else {
        return SlotPath::root();
    };
    SlotPath::from_segments(parent.to_vec())
}

fn is_strict_ancestor(ancestor: &SlotPath, descendant: &SlotPath) -> bool {
    let ancestor = ancestor.segments();
    let descendant = descendant.segments();
    ancestor.len() < descendant.len() && descendant.starts_with(ancestor)
}

impl ArtifactOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub fn contains(&self, location: &ArtifactLoc) -> bool {
        self.edits.contains_key(location)
    }

    pub fn pending_at(&self, location: &ArtifactLoc) -> Option<&ArtifactEdits> {
        self.edits.get(location)
    }

    pub fn pending_at_mut(&mut self, location: &ArtifactLoc) -> Option<&mut ArtifactEdits> {
        self.edits.get_mut(location)
    }

    pub fn ensure_pending(&mut self, location: ArtifactLoc) -> &mut ArtifactEdits {
        self.edits.entry(location).or_default()
    }

    pub fn remove(&mut self, location: &ArtifactLoc) -> bool {
        self.edits.remove(location).is_some()
    }

    pub fn clear(&mut self) {
        self.edits.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ArtifactLoc, &ArtifactEdits)> + '_ {
        self.edits.iter().filter(|(_, pending)| !pending.is_empty())
    }

    /// Merge pending edits from `other` into this overlay.
    pub fn merge_from(&mut self, other: &ArtifactOverlay) {
        for (location, source) in other.iter() {
            let pending = self.ensure_pending(location.clone());
            pending.merge_from(source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{LpValue, SlotPath};

    #[test]
    fn empty_overlay() {
        let overlay = ArtifactOverlay::new();
        assert!(overlay.is_empty());
        assert!(!overlay.contains(&ArtifactLoc::file("/a.toml")));
    }

    #[test]
    fn upsert_two_slot_paths() {
        let mut pending = ArtifactEdits::default();
        pending.upsert_slot(SlotEdit::AssignValue {
            path: SlotPath::parse("controls.rate").unwrap(),
            value: LpValue::F32(1.0),
        });
        pending.upsert_slot(SlotEdit::AssignValue {
            path: SlotPath::parse("controls.phase").unwrap(),
            value: LpValue::F32(0.5),
        });
        assert_eq!(pending.slot_edits().count(), 2);
    }

    #[test]
    fn upsert_same_path_replaces() {
        let mut pending = ArtifactEdits::default();
        let path = SlotPath::parse("controls.rate").unwrap();
        pending.upsert_slot(SlotEdit::AssignValue {
            path: path.clone(),
            value: LpValue::F32(1.0),
        });
        pending.upsert_slot(SlotEdit::AssignValue {
            path,
            value: LpValue::F32(2.0),
        });
        assert_eq!(pending.slot_edits().count(), 1);
    }

    #[test]
    fn upsert_same_key_moves_to_end() {
        let mut pending = ArtifactEdits::default();
        pending.upsert_slot(SlotEdit::AssignValue {
            path: SlotPath::parse("controls.rate").unwrap(),
            value: LpValue::F32(1.0),
        });
        pending.upsert_slot(SlotEdit::AssignValue {
            path: SlotPath::parse("controls.phase").unwrap(),
            value: LpValue::F32(0.5),
        });
        pending.upsert_slot(SlotEdit::AssignValue {
            path: SlotPath::parse("controls.rate").unwrap(),
            value: LpValue::F32(2.0),
        });
        assert_eq!(pending.slot_edits().count(), 2);
        let rate = pending.slot_edits().last().and_then(|edit| match edit {
            SlotEdit::AssignValue { value, .. } => Some(value),
            _ => None,
        });
        assert_eq!(rate, Some(&LpValue::F32(2.0)));
    }

    #[test]
    fn set_asset_clears_slots() {
        let mut pending = ArtifactEdits::default();
        pending.upsert_slot(SlotEdit::EnsurePresent {
            path: SlotPath::root(),
        });
        pending.set_asset(AssetEdit::Delete);
        assert_eq!(pending.slot_edits().count(), 0);
        assert_eq!(pending.asset_edit, AssetEdit::Delete);
    }

    #[test]
    fn upsert_slot_clears_asset() {
        let mut pending = ArtifactEdits::default();
        pending.set_asset(AssetEdit::ReplaceBody(b"body".to_vec()));
        pending.upsert_slot(SlotEdit::EnsurePresent {
            path: SlotPath::root(),
        });
        assert_eq!(pending.asset_edit, AssetEdit::None);
        assert_eq!(pending.slot_edits().count(), 1);
    }

    #[test]
    fn remove_and_clear() {
        let mut overlay = ArtifactOverlay::new();
        let location = ArtifactLoc::file("/clock.toml");
        overlay.ensure_pending(location.clone());
        assert!(overlay.contains(&location));
        assert!(overlay.remove(&location));
        assert!(!overlay.contains(&location));

        overlay.ensure_pending(location.clone());
        overlay.clear();
        assert!(overlay.is_empty());
    }

    #[test]
    fn has_pending_at_path() {
        let mut pending = ArtifactEdits::default();
        let path = SlotPath::parse("controls.rate").unwrap();
        assert!(!pending.has_pending_at_path(&path));
        pending.upsert_slot(SlotEdit::AssignValue {
            path: path.clone(),
            value: LpValue::F32(1.0),
        });
        assert!(pending.has_pending_at_path(&path));
        assert!(!pending.has_pending_at_path(&SlotPath::parse("controls.phase").unwrap()));
    }

    #[test]
    fn parent_remove_clears_pending_descendants() {
        let mut pending = ArtifactEdits::default();
        pending.upsert_slot(SlotEdit::AssignValue {
            path: SlotPath::parse("entries[0].node.controls.rate").unwrap(),
            value: LpValue::F32(2.0),
        });
        pending.upsert_slot(SlotEdit::Remove {
            path: SlotPath::parse("entries[0].node").unwrap(),
        });

        assert_eq!(pending.slot_edits().count(), 1);
        assert!(matches!(
            pending.slot_edits().next(),
            Some(SlotEdit::Remove { path }) if path == &SlotPath::parse("entries[0].node").unwrap()
        ));
    }

    #[test]
    fn descendant_assign_clears_ancestor_remove() {
        let mut pending = ArtifactEdits::default();
        pending.upsert_slot(SlotEdit::Remove {
            path: SlotPath::parse("entries[0].node").unwrap(),
        });
        pending.upsert_slot(SlotEdit::AssignValue {
            path: SlotPath::parse("entries[0].node.controls.rate").unwrap(),
            value: LpValue::F32(2.0),
        });

        assert_eq!(pending.slot_edits().count(), 1);
        assert!(matches!(
            pending.slot_edits().next(),
            Some(SlotEdit::AssignValue { path, .. })
                if path == &SlotPath::parse("entries[0].node.controls.rate").unwrap()
        ));
    }

    #[test]
    fn enum_variant_ensure_clears_stale_payload_descendants() {
        let mut pending = ArtifactEdits::default();
        pending.upsert_slot(SlotEdit::AssignValue {
            path: SlotPath::parse("entries[0].node.controls.rate").unwrap(),
            value: LpValue::F32(2.0),
        });
        pending.upsert_slot(SlotEdit::EnsurePresent {
            path: SlotPath::parse("entries[0].node.Shader").unwrap(),
        });

        assert_eq!(pending.slot_edits().count(), 1);
        assert!(matches!(
            pending.slot_edits().next(),
            Some(SlotEdit::EnsurePresent { path })
                if path == &SlotPath::parse("entries[0].node.Shader").unwrap()
        ));
    }

    #[test]
    fn single_field_ensure_does_not_clear_root_variant_ensure() {
        let mut pending = ArtifactEdits::default();
        pending.upsert_slot(SlotEdit::EnsurePresent {
            path: SlotPath::parse("Output").unwrap(),
        });
        pending.upsert_slot(SlotEdit::EnsurePresent {
            path: SlotPath::parse("options").unwrap(),
        });

        assert_eq!(pending.slot_edits().count(), 2);
    }
}
