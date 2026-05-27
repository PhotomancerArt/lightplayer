//! Parsed node definition registry driven by artifact freshness.

use alloc::collections::BTreeMap;

use lpc_model::{Revision, SlotPath};
use lpfs::{LpFs, LpPath, LpPathBuf};

use crate::edit::{
    ArtifactEdits, ArtifactOverlay, AssetEdit, CommitError, EditError, SlotEdit,
    require_absolute_path,
};
use crate::{ArtifactLoc, ArtifactStore};

use super::sync_result::SyncResult;
use super::{NodeDefEntry, NodeDefLoc, NodeDefState, ParseCtx};

/// Owner of parsed node definitions keyed by [`NodeDefLoc`].
///
/// Bootstrap with [`Self::load_root`], react to filesystem edits via
/// [`Self::sync`] / [`Self::sync_fs`], mutate pending state via
/// [`Self::upsert_slot_edit`] / [`Self::set_pending_asset`] / [`Self::apply_overlay`],
/// then [`Self::commit`] or [`Self::discard_overlay`].
/// Pending edits are address-keyed current slot/asset changes in [`ArtifactOverlay`].
/// Effective reads use [`crate::NodeDefView`].
pub struct NodeDefRegistry {
    pub(crate) store: ArtifactStore,
    pub(crate) overlay: ArtifactOverlay,
    pub(crate) defs: BTreeMap<NodeDefLoc, NodeDefEntry>,
    pub(crate) root: Option<NodeDefLoc>,
}

impl Default for NodeDefRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeDefRegistry {
    pub fn new() -> Self {
        Self {
            store: ArtifactStore::new(),
            overlay: ArtifactOverlay::new(),
            defs: BTreeMap::new(),
            root: None,
        }
    }

    /// Drop pending overlay entry for `path`. Returns whether an entry existed.
    pub fn remove_pending_at(&mut self, path: &LpPath) -> bool {
        let location = self.location_for_pending_path(path);
        self.overlay.remove(&location)
    }

    /// Upsert one slot edit into the overlay for a `.toml` artifact path.
    pub fn upsert_slot_edit(
        &mut self,
        path: LpPathBuf,
        op: SlotEdit,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
        frame: Revision,
    ) -> Result<(), EditError> {
        self.queue_slot_edit(path, &op, fs, ctx, frame)
    }

    /// Set pending asset state for one artifact path.
    pub fn set_pending_asset(
        &mut self,
        path: LpPathBuf,
        asset: AssetEdit,
    ) -> Result<(), EditError> {
        require_absolute_path(path.clone())?;
        let location = self.location_for_pending_path(LpPath::new(path.as_str()));
        self.overlay.ensure_pending(location).set_asset(asset);
        Ok(())
    }

    /// Merge pending overlay edits into the registry overlay.
    pub fn apply_overlay(&mut self, overlay: &ArtifactOverlay) {
        self.overlay.merge_from(overlay);
    }

    pub fn root_loc(&self) -> Option<&NodeDefLoc> {
        self.root.as_ref()
    }

    pub fn get(&self, loc: &NodeDefLoc) -> Option<&NodeDefEntry> {
        self.defs.get(loc)
    }

    /// Iterate registered entries (stable order by location).
    pub fn iter_entries(&self) -> impl Iterator<Item = &NodeDefEntry> {
        self.defs.values()
    }

    /// Drop all pending overlay edits.
    pub fn discard_overlay(&mut self) {
        self.overlay.clear();
    }

    /// Drop all pending overlay edits.
    #[deprecated(note = "renamed to discard_overlay")]
    pub fn discard_slot_overlay(&mut self) {
        self.discard_overlay();
    }

    /// Promote all pending overlay entries to committed store and entries.
    pub fn commit(
        &mut self,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<SyncResult, CommitError> {
        super::commit::commit_slot_overlay(self, fs, frame, ctx)
    }

    pub(crate) fn restore_entry_states(&mut self, before: &BTreeMap<NodeDefLoc, NodeDefState>) {
        for (loc, state) in before {
            if let Some(entry) = self.defs.get_mut(loc) {
                entry.state = state.clone();
            }
        }
    }

    /// Whether any artifact has pending edits.
    pub fn overlay_active(&self) -> bool {
        !self.overlay.is_empty()
    }

    /// Pending edits for one artifact, if any.
    pub fn pending_at(&self, location: &ArtifactLoc) -> Option<&ArtifactEdits> {
        self.overlay.pending_at(location)
    }

    /// Iterate artifacts with pending edits (stable order).
    pub fn iter_pending(&self) -> impl Iterator<Item = (&ArtifactLoc, &ArtifactEdits)> + '_ {
        self.overlay.iter()
    }

    /// Whether a specific slot path has a pending edit within an artifact.
    pub fn has_pending_slot(&self, location: &ArtifactLoc, path: &SlotPath) -> bool {
        self.overlay
            .pending_at(location)
            .is_some_and(|pending| pending.has_pending_at_path(path))
    }

    /// Whether any overlay entries are pending.
    #[deprecated(note = "renamed to overlay_active")]
    pub fn slot_overlay_active(&self) -> bool {
        self.overlay_active()
    }

    /// Whether `path` has a pending overlay entry.
    pub fn overlay_contains_path(&self, path: &LpPath) -> bool {
        let location = self.location_for_pending_path(path);
        self.overlay.contains(&location)
    }

    /// Whether `path` has a pending overlay entry.
    #[deprecated(note = "renamed to overlay_contains_path")]
    pub fn slot_overlay_contains_path(&self, path: &LpPath) -> bool {
        self.overlay_contains_path(path)
    }

    /// Pending overlay bytes for `path`, if any (asset replace-body only).
    pub fn pending_asset_bytes(&self, path: &LpPath) -> Option<&[u8]> {
        let location = self.location_for_pending_path(path);
        let pending = self.overlay.pending_at(&location)?;
        match pending.asset_pending() {
            crate::edit::AssetEdit::ReplaceBody(bytes) => Some(bytes.as_slice()),
            _ => None,
        }
    }

    /// Pending overlay bytes for `path`, if any (asset replace-body only).
    #[deprecated(note = "renamed to pending_asset_bytes")]
    pub fn slot_overlay_bytes(&self, path: &LpPath) -> Option<&[u8]> {
        self.pending_asset_bytes(path)
    }

    pub(crate) fn artifact_location_for_path(&self, path: &LpPath) -> Option<ArtifactLoc> {
        self.store.location_for_path(path)
    }

    /// Committed [`ArtifactStore`] revision for a registered file path.
    pub fn artifact_revision_for_path(&self, path: &LpPath) -> Option<Revision> {
        self.store
            .location_for_path(path)
            .and_then(|location| self.store.revision(&location))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use lpc_model::{NodeKind, SlotShapeRegistry};
    use lpfs::{FsEvent, FsEventKind, LpFsMemory};

    use super::super::{DefChangeDetail, NodeDefUpdates, RegistryError};

    fn parse_ctx() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }

    fn fs_modify(path: &str) -> FsEvent {
        FsEvent {
            path: LpPathBuf::from(path),
            kind: FsEventKind::Modify,
        }
    }

    fn changed_set(updates: &NodeDefUpdates) -> BTreeSet<NodeDefLoc> {
        updates.changed.iter().cloned().collect()
    }

    #[test]
    fn load_root_registers_inline_child() {
        let mut fs = LpFsMemory::new();
        crate::harness::fixtures::write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "a.glsl" }
"#,
        );
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        registry
            .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
            .unwrap();
        assert_eq!(registry.defs.len(), 2);
    }

    #[test]
    fn load_root_rejects_non_empty_registry() {
        let mut fs = LpFsMemory::new();
        crate::harness::fixtures::write_file(&mut fs, "/clock.toml", "kind = \"Clock\"\n");
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        registry
            .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
            .unwrap();
        let err = registry
            .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(2), &ctx)
            .unwrap_err();
        assert!(matches!(err, RegistryError::NotEmpty));
    }

    #[test]
    fn leaf_file_edit_marks_root_changed() {
        let mut fs = crate::harness::fixtures::load_clock();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
            .unwrap();

        crate::harness::fixtures::write_file(
            &mut fs,
            "/clock.toml",
            r#"
kind = "Clock"

[controls]
rate = 2.0
"#,
        );
        let result = registry.sync_fs(&fs, &[fs_modify("/clock.toml")], Revision::new(2), &ctx);
        assert!(result.def_updates.added.is_empty());
        assert!(result.def_updates.removed.is_empty());
        assert_eq!(
            changed_set(&result.def_updates),
            BTreeSet::from([root.clone()])
        );
        assert!(matches!(
            result.change_details.as_slice(),
            [(loc, DefChangeDetail::Content)] if *loc == root
        ));
    }

    #[test]
    fn glsl_edit_only_bumps_artifact_store_revision() {
        let mut fs = crate::harness::fixtures::load_shader_project();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        registry
            .load_root(&fs, LpPath::new("/shader.toml"), Revision::new(1), &ctx)
            .unwrap();

        crate::harness::fixtures::write_file(
            &mut fs,
            "/shader.glsl",
            "void main() { gl_FragColor = vec4(0.0); }",
        );
        let result = registry.sync_fs(&fs, &[fs_modify("/shader.glsl")], Revision::new(2), &ctx);
        assert!(result.def_updates.is_empty());
        assert_eq!(
            registry.artifact_revision_for_path(LpPath::new("/shader.glsl")),
            Some(Revision::new(2))
        );
    }

    #[test]
    fn inline_child_edit_isolated() {
        let mut fs = crate::harness::fixtures::load_playlist_with_inline_child();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
            .unwrap();
        let child = registry
            .defs
            .keys()
            .find(|loc| !loc.path.is_root())
            .expect("inline child")
            .clone();

        crate::harness::fixtures::write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "b.glsl" }
"#,
        );
        let result = registry.sync_fs(&fs, &[fs_modify("/playlist.toml")], Revision::new(2), &ctx);
        assert!(!result.def_updates.contains_changed(&root));
        assert_eq!(changed_set(&result.def_updates), BTreeSet::from([child]));
    }

    #[test]
    fn playlist_entry_add_marks_parent_and_child_added() {
        let mut fs = LpFsMemory::new();
        crate::harness::fixtures::write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
"#,
        );
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
            .unwrap();

        crate::harness::fixtures::write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"

[entries.3.node.def]
kind = "Clock"
"#,
        );
        let result = registry.sync_fs(&fs, &[fs_modify("/playlist.toml")], Revision::new(2), &ctx);
        assert_eq!(result.def_updates.added.len(), 1);
        assert!(result.def_updates.removed.is_empty());
        assert!(result.def_updates.contains_changed(&root));
    }

    #[test]
    fn path_child_file_edit_isolated() {
        let mut fs = LpFsMemory::new();
        crate::harness::fixtures::write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2]
node = { ref = "./active.toml" }
"#,
        );
        crate::harness::fixtures::write_file(
            &mut fs,
            "/active.toml",
            r#"
kind = "Shader"
source = { path = "a.glsl" }
"#,
        );
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
            .unwrap();
        let child = registry
            .defs
            .keys()
            .find(|loc| loc.path.is_root() && **loc != root)
            .expect("child file root")
            .clone();

        crate::harness::fixtures::write_file(
            &mut fs,
            "/active.toml",
            r#"
kind = "Shader"
source = { path = "b.glsl" }
"#,
        );
        let result = registry.sync_fs(&fs, &[fs_modify("/active.toml")], Revision::new(2), &ctx);
        assert!(!result.def_updates.contains_changed(&root));
        assert_eq!(changed_set(&result.def_updates), BTreeSet::from([child]));
    }

    #[test]
    fn inline_child_kind_change_marks_child_and_parent_changed() {
        let mut fs = LpFsMemory::new();
        crate::harness::fixtures::write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
"#,
        );
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(&fs, LpPath::new("/playlist.toml"), Revision::new(1), &ctx)
            .unwrap();
        let child = registry
            .defs
            .keys()
            .find(|loc| !loc.path.is_root())
            .expect("inline child")
            .clone();

        crate::harness::fixtures::write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Clock"
"#,
        );
        let result = registry.sync_fs(&fs, &[fs_modify("/playlist.toml")], Revision::new(2), &ctx);
        assert!(result.def_updates.contains_changed(&root));
        assert!(result.def_updates.contains_changed(&child));
        assert!(
            result
                .change_details
                .iter()
                .any(|(loc, detail)| *loc == child
                    && matches!(
                        detail,
                        DefChangeDetail::KindChanged {
                            from: NodeKind::Shader,
                            to: NodeKind::Clock
                        }
                    ))
        );
    }

    #[test]
    fn leaf_parse_error_reports_entered_error() {
        let mut fs = crate::harness::fixtures::load_clock();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(&fs, LpPath::new("/clock.toml"), Revision::new(1), &ctx)
            .unwrap();

        crate::harness::fixtures::write_file(&mut fs, "/clock.toml", "kind = \"Clock\"\nrate = ");
        let result = registry.sync_fs(&fs, &[fs_modify("/clock.toml")], Revision::new(2), &ctx);
        assert!(result.def_updates.contains_changed(&root));
        assert!(matches!(
            result.change_details.as_slice(),
            [(loc, DefChangeDetail::EnteredError)] if *loc == root
        ));
    }
}
