//! Address-keyed pending artifact edits (slot upserts and asset replacements).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::SlotPath;

use crate::ArtifactLoc;

use super::SlotEdit;
use super::pending_slot_key::{slot_edit_key, slot_path_key};

/// In-memory map of current pending edits keyed by [`ArtifactLoc`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtifactOverlay {
    edits: BTreeMap<ArtifactLoc, ArtifactEdits>,
}

/// Pending edits for one artifact location.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtifactEdits {
    /// Pending slot ops in apply order. Key is [`slot_edit_key`]; same key upserts in place.
    pub slot_edits: Vec<(String, SlotEdit)>,
    pub asset_edit: PendingAsset,
}

/// Pending asset body or deletion for one artifact.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum PendingAsset {
    #[default]
    None,
    Delete,
    ReplaceBody(Vec<u8>),
}

impl ArtifactEdits {
    /// Insert or replace the pending edit; clears asset pending.
    pub fn upsert_slot(&mut self, edit: SlotEdit) {
        self.asset_edit = PendingAsset::None;
        let key = slot_edit_key(&edit);
        if let Some(pos) = self
            .slot_edits
            .iter()
            .position(|(existing, _)| existing == &key)
        {
            self.slot_edits.remove(pos);
        }
        self.slot_edits.push((key, edit));
    }

    /// Set asset pending state; clears all slot edits.
    pub fn set_asset(&mut self, asset: PendingAsset) {
        self.asset_edit = asset;
        self.slot_edits.clear();
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.asset_edit, PendingAsset::None) && self.slot_edits.is_empty()
    }

    pub fn slot_edits(&self) -> impl Iterator<Item = (&str, &SlotEdit)> {
        self.slot_edits
            .iter()
            .map(|(key, edit)| (key.as_str(), edit))
    }

    pub(crate) fn slot_edits_in_apply_order(&self) -> impl Iterator<Item = &SlotEdit> {
        self.slot_edits.iter().map(|(_, edit)| edit)
    }

    pub fn asset_pending(&self) -> &PendingAsset {
        &self.asset_edit
    }

    pub fn has_pending_at_path(&self, path: &SlotPath) -> bool {
        let path_key = slot_path_key(path);
        self.slot_edits
            .iter()
            .any(|(key, _)| key == &path_key || key.starts_with(&alloc::format!("{path_key}#")))
    }
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
        assert_eq!(pending.slot_edits.len(), 2);
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
        assert_eq!(pending.slot_edits.len(), 1);
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
        assert_eq!(pending.slot_edits.len(), 2);
        let rate = pending.slot_edits.last().and_then(|(_, edit)| match edit {
            SlotEdit::AssignValue { value, .. } => Some(value),
            _ => None,
        });
        assert_eq!(rate, Some(&LpValue::F32(2.0)));
    }

    #[test]
    fn set_asset_clears_slots() {
        let mut pending = ArtifactEdits::default();
        pending.upsert_slot(SlotEdit::UseEnumVariant {
            path: SlotPath::root(),
            variant: "Clock".into(),
        });
        pending.set_asset(PendingAsset::Delete);
        assert!(pending.slot_edits.is_empty());
        assert_eq!(pending.asset_edit, PendingAsset::Delete);
    }

    #[test]
    fn upsert_slot_clears_asset() {
        let mut pending = ArtifactEdits::default();
        pending.set_asset(PendingAsset::ReplaceBody(b"body".to_vec()));
        pending.upsert_slot(SlotEdit::UseEnumVariant {
            path: SlotPath::root(),
            variant: "Clock".into(),
        });
        assert_eq!(pending.asset_edit, PendingAsset::None);
        assert_eq!(pending.slot_edits.len(), 1);
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
}
