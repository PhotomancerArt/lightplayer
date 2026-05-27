//! Parsed node definition registry driven by artifact freshness.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{NodeDef, NodeInvocation, Revision, SlotPath};
use lpfs::{FsEvent, LpFs, LpPath, LpPathBuf};

use crate::edit::{
    ArtifactEdits, ArtifactOverlay, CommitError, EditError, OverlayDelta, PendingAsset, SlotEdit,
    require_absolute_path,
};
use crate::{ArtifactLoc, ArtifactStore};

use super::def_shell::{is_container_def, shell_changed};
use super::def_walker::{collect_invocations, resolve_node_specifier};
use super::source_bridge;
use super::sync_error::SyncError;
use super::sync_op::SyncOp;
use super::sync_outcome::SyncOutcome;
use super::sync_result::{DefChangeDetail, SyncResult};
use super::{NodeDefEntry, NodeDefLoc, NodeDefState, NodeDefUpdates, ParseCtx, RegistryError};

/// Owner of parsed node definitions keyed by [`NodeDefLoc`].
///
/// Bootstrap with [`Self::load_root`], react to filesystem edits via
/// [`Self::sync`] / [`Self::sync_fs`], mutate pending state via
/// [`Self::upsert_slot_edit`] / [`Self::set_pending_asset`] / [`Self::apply_overlay_delta`],
/// then [`Self::commit`] or [`Self::discard_slot_overlay`].
/// Pending edits are address-keyed current slot/asset changes in [`ArtifactOverlay`].
/// Effective reads use [`crate::NodeDefView`].
pub struct NodeDefRegistry {
    store: ArtifactStore,
    overlay: ArtifactOverlay,
    defs: BTreeMap<NodeDefLoc, NodeDefEntry>,
    root: Option<NodeDefLoc>,
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

    /// Load all defs reachable from a root node-definition TOML file.
    ///
    /// The root kind is not enforced — `project.toml` is convention only.
    pub fn load_root(
        &mut self,
        fs: &dyn LpFs,
        root_path: &LpPath,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefLoc, RegistryError> {
        if !self.defs.is_empty() {
            return Err(RegistryError::NotEmpty);
        }
        if !root_path.is_absolute() {
            return Err(RegistryError::InvalidPath {
                message: alloc::format!("root path must be absolute: `{}`", root_path.as_str()),
            });
        }
        let path_buf = root_path.to_path_buf();
        let location = self.store.register_file(path_buf.clone(), frame);
        let root_loc = self.register_artifact_subtree(location, root_path, frame, fs, ctx)?;
        self.root = Some(root_loc.clone());
        self.register_all_asset_paths(frame)?;
        Ok(root_loc)
    }

    /// Apply incoming sync operations and return committed + pending effects.
    pub fn sync(
        &mut self,
        fs: &dyn LpFs,
        ops: &[SyncOp],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<SyncOutcome, SyncError> {
        let mut committed = SyncResult::default();
        let mut pending_changed = false;

        for op in ops {
            match op.clone() {
                SyncOp::Fs(event) => {
                    let result = self.apply_fs_sync(fs, core::slice::from_ref(&event), frame, ctx);
                    committed.merge(result);
                }
                SyncOp::UpsertSlot { path, op } => {
                    self.upsert_slot_edit(path, op, fs, ctx, frame)?;
                    pending_changed = true;
                }
                SyncOp::SetPendingAsset { path, asset } => {
                    self.set_pending_asset(path, asset)?;
                    pending_changed = true;
                }
                SyncOp::Remove { path } => {
                    pending_changed |= self.remove_pending_at(LpPath::new(path.as_str()));
                }
                SyncOp::ClearPending => {
                    if self.overlay_active() {
                        self.overlay.clear();
                        pending_changed = true;
                    }
                }
                SyncOp::Commit => {
                    let had_pending = self.overlay_active();
                    let result = commit::commit_slot_overlay(self, fs, frame, ctx)?;
                    committed.merge(result);
                    pending_changed |= had_pending;
                }
            }
        }

        Ok(SyncOutcome {
            committed,
            pending_changed,
        })
    }

    /// Convenience wrapper mapping [`FsEvent`] batches to [`SyncOp::Fs`].
    pub fn sync_fs(
        &mut self,
        fs: &dyn LpFs,
        changes: &[FsEvent],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> SyncResult {
        let ops: Vec<SyncOp> = changes.iter().cloned().map(SyncOp::Fs).collect();
        self.sync(fs, &ops, frame, ctx)
            .map(|outcome| outcome.committed)
            .unwrap_or_default()
    }

    fn apply_fs_sync(
        &mut self,
        fs: &dyn LpFs,
        changes: &[FsEvent],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> SyncResult {
        let before = self.snapshot_def_states();

        if !changes.is_empty() {
            self.store.apply_fs_changes(changes, frame);
        }

        let mut def_updates = NodeDefUpdates::default();

        let mut def_artifact_locations = Vec::new();
        for change in changes {
            if let PathChangeKind::DefArtifact(location) = self.classify_changed_path(&change.path)
            {
                def_artifact_locations.push(location);
            }
        }
        dedupe_locations(&mut def_artifact_locations);

        for location in def_artifact_locations {
            self.sync_def_artifact(location, fs, frame, ctx, &mut def_updates);
        }

        let _ = self.reconcile_artifacts();

        let change_details = build_change_details(&before, &def_updates, &self.defs);
        SyncResult {
            def_updates,
            change_details,
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
        self.apply_slot_op(path, &op, fs, ctx, frame)
    }

    /// Set pending asset state for one artifact path.
    pub fn set_pending_asset(
        &mut self,
        path: LpPathBuf,
        asset: PendingAsset,
    ) -> Result<(), EditError> {
        require_absolute_path(path.clone())?;
        let location = self.location_for_pending_path(LpPath::new(path.as_str()));
        self.overlay.ensure_pending(location).set_asset(asset);
        Ok(())
    }

    /// Merge snapshot diff pending state into the overlay.
    pub fn apply_overlay_delta(
        &mut self,
        delta: &OverlayDelta,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
        frame: Revision,
    ) -> Result<(), EditError> {
        for (path, source) in delta.iter() {
            for op in source.slot_edits() {
                self.upsert_slot_edit(path.clone(), op.clone(), fs, ctx, frame)?;
            }
            if !matches!(source.asset_pending(), PendingAsset::None) {
                self.set_pending_asset(path.clone(), source.asset_pending().clone())?;
            }
        }
        Ok(())
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
    pub fn discard_slot_overlay(&mut self) {
        self.overlay.clear();
    }

    /// Promote all pending overlay entries to committed store and entries.
    pub fn commit(
        &mut self,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<SyncResult, CommitError> {
        commit::commit_slot_overlay(self, fs, frame, ctx)
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
    pub fn slot_overlay_contains_path(&self, path: &LpPath) -> bool {
        let location = self.location_for_pending_path(path);
        self.overlay.contains(&location)
    }

    /// Pending overlay bytes for `path`, if any (asset replace-body only).
    pub fn slot_overlay_bytes(&self, path: &LpPath) -> Option<&[u8]> {
        let location = self.location_for_pending_path(path);
        let pending = self.overlay.pending_at(&location)?;
        match pending.asset_pending() {
            crate::edit::PendingAsset::ReplaceBody(bytes) => Some(bytes.as_slice()),
            _ => None,
        }
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

    fn register_artifact_subtree(
        &mut self,
        location: ArtifactLoc,
        file_path: &LpPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefLoc, RegistryError> {
        let revision = self.store.revision(&location).unwrap_or(frame);
        let state = self.read_artifact_state(&location, fs, ctx)?;
        let source = NodeDefLoc::artifact_root(location.clone());
        self.register_def_at_source(source.clone(), state.clone(), revision)?;
        if let NodeDefState::Loaded(def) = state {
            self.register_invocations(&location, file_path, def, SlotPath::root(), frame, fs, ctx)?;
        }
        Ok(source)
    }

    fn register_invocations(
        &mut self,
        location: &ArtifactLoc,
        file_path: &LpPath,
        def: NodeDef,
        base_path: SlotPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<(), RegistryError> {
        for site in collect_invocations(&def, &base_path) {
            match &site.invocation {
                NodeInvocation::Unset => {}
                NodeInvocation::Ref(path_slot) => {
                    let path_text = path_slot.value().as_str();
                    if path_text.is_empty() {
                        continue;
                    }
                    let specifier = lpc_model::ArtifactSpec::parse(path_text).map_err(|err| {
                        RegistryError::SpecifierResolution {
                            message: String::from(err),
                        }
                    })?;
                    let child_path = resolve_node_specifier(file_path, &specifier)?;
                    let child_location = self.store.register_file(child_path.clone(), frame);
                    let child_source = NodeDefLoc::artifact_root(child_location.clone());
                    if !self.defs.contains_key(&child_source) {
                        self.register_artifact_subtree(
                            child_location,
                            child_path.as_path(),
                            frame,
                            fs,
                            ctx,
                        )?;
                    }
                }
                NodeInvocation::Def(body) => {
                    let source = NodeDefLoc {
                        artifact: location.clone(),
                        path: site.path.clone(),
                    };
                    let revision = self.store.revision(&location).unwrap_or(frame);
                    self.register_def_at_source(
                        source,
                        NodeDefState::Loaded(body.value().clone()),
                        revision,
                    )?;
                    self.register_invocations(
                        location,
                        file_path,
                        body.value().clone(),
                        site.path,
                        frame,
                        fs,
                        ctx,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn sync_def_artifact(
        &mut self,
        location: ArtifactLoc,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
        updates: &mut NodeDefUpdates,
    ) {
        let Some(current) = self.store.revision(&location) else {
            return;
        };
        let Some(file_path) = location.file_path().cloned() else {
            return;
        };

        let new_inventory =
            match self.derive_inventory(location.clone(), file_path.as_path(), frame, fs, ctx) {
                Ok(inventory) => inventory,
                Err(_) => return,
            };

        let old_sources: BTreeMap<NodeDefLoc, NodeDefState> = self
            .defs
            .iter()
            .filter(|(loc, _)| loc.artifact == location)
            .map(|(loc, entry)| (loc.clone(), entry.state.clone()))
            .collect();

        for source in old_sources.keys() {
            if !new_inventory.contains_key(source) {
                updates.push_removed(source.clone());
                self.defs.remove(source);
            }
        }

        let mut affected = Vec::new();
        for (source, new_state) in &new_inventory {
            if let Some(old_state) = old_sources.get(source) {
                if state_changed(old_state, new_state) {
                    updates.push_changed(source.clone());
                    if let Some(entry) = self.defs.get_mut(source) {
                        entry.state = new_state.clone();
                        entry.revision = current;
                    }
                    affected.push(source.clone());
                }
            } else if self
                .register_def_at_source(source.clone(), new_state.clone(), current)
                .is_ok()
            {
                updates.push_added(source.clone());
                affected.push(source.clone());
            }
        }

        for loc in affected {
            let _ = self.register_asset_paths_for_entry(&loc, frame);
        }
    }

    fn derive_inventory(
        &mut self,
        location: ArtifactLoc,
        file_path: &LpPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<BTreeMap<NodeDefLoc, NodeDefState>, RegistryError> {
        let mut inventory = BTreeMap::new();
        let state = self.read_artifact_state(&location, fs, ctx)?;
        inventory.insert(NodeDefLoc::artifact_root(location.clone()), state.clone());
        if let NodeDefState::Loaded(def) = state {
            self.derive_invocations(
                &location,
                file_path,
                def,
                SlotPath::root(),
                frame,
                fs,
                ctx,
                &mut inventory,
            )?;
        }
        Ok(inventory)
    }

    fn derive_invocations(
        &mut self,
        location: &ArtifactLoc,
        file_path: &LpPath,
        def: NodeDef,
        base_path: SlotPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
        inventory: &mut BTreeMap<NodeDefLoc, NodeDefState>,
    ) -> Result<(), RegistryError> {
        for site in collect_invocations(&def, &base_path) {
            match &site.invocation {
                NodeInvocation::Unset => {}
                NodeInvocation::Ref(path_slot) => {
                    let path_text = path_slot.value().as_str();
                    if path_text.is_empty() {
                        continue;
                    }
                    let specifier = lpc_model::ArtifactSpec::parse(path_text).map_err(|err| {
                        RegistryError::SpecifierResolution {
                            message: String::from(err),
                        }
                    })?;
                    let child_path = resolve_node_specifier(file_path, &specifier)?;
                    let child_location = self.store.register_file(child_path.clone(), frame);
                    let child_inventory = self.derive_inventory(
                        child_location,
                        child_path.as_path(),
                        frame,
                        fs,
                        ctx,
                    )?;
                    for (source, state) in child_inventory {
                        if inventory.insert(source.clone(), state).is_some() {
                            return Err(RegistryError::DuplicateSource);
                        }
                    }
                }
                NodeInvocation::Def(body) => {
                    let source = NodeDefLoc {
                        artifact: location.clone(),
                        path: site.path.clone(),
                    };
                    if inventory
                        .insert(source, NodeDefState::Loaded(body.value().clone()))
                        .is_some()
                    {
                        return Err(RegistryError::DuplicateSource);
                    }
                    self.derive_invocations(
                        location,
                        file_path,
                        body.value().clone(),
                        site.path,
                        frame,
                        fs,
                        ctx,
                        inventory,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn read_artifact_state(
        &mut self,
        location: &ArtifactLoc,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefState, RegistryError> {
        match self.store.read_bytes(location, fs) {
            Ok(bytes) => Ok(effective_read::parse_toml_bytes(ctx, &bytes)),
            Err(err) => Ok(NodeDefState::ParseError(effective_read::read_error_state(
                err,
            ))),
        }
    }

    pub(crate) fn register_file_artifact(
        &mut self,
        path: LpPathBuf,
        frame: Revision,
    ) -> ArtifactLoc {
        self.store.register_file(path, frame)
    }

    fn referenced_locations(&self) -> alloc::collections::BTreeSet<ArtifactLoc> {
        let mut referenced = self
            .defs
            .keys()
            .map(|loc| loc.artifact.clone())
            .collect::<alloc::collections::BTreeSet<_>>();

        for (loc, entry) in &self.defs {
            let NodeDefState::Loaded(def) = &entry.state else {
                continue;
            };
            let Some(containing) = loc.artifact.file_path() else {
                continue;
            };
            if let Ok(paths) = source_bridge::asset_paths_for_def(def, containing.as_path()) {
                for path in paths {
                    referenced.insert(ArtifactLoc::location_for_path(path.as_path()));
                }
            }
        }

        referenced
    }

    pub(crate) fn reconcile_artifacts(&mut self) -> Result<(), RegistryError> {
        let referenced = self.referenced_locations();
        let to_unregister: Vec<ArtifactLoc> = self
            .store
            .locations()
            .filter(|location| !referenced.contains(location))
            .collect();

        for location in to_unregister {
            self.store.unregister(&location)?;
        }
        Ok(())
    }

    fn register_def_at_source(
        &mut self,
        source: NodeDefLoc,
        state: NodeDefState,
        revision: Revision,
    ) -> Result<(), RegistryError> {
        if self.defs.contains_key(&source) {
            return Err(RegistryError::DuplicateSource);
        }
        self.defs.insert(
            source.clone(),
            NodeDefEntry {
                loc: source,
                state,
                revision,
            },
        );
        Ok(())
    }

    fn register_all_asset_paths(&mut self, frame: Revision) -> Result<(), RegistryError> {
        let locs: Vec<NodeDefLoc> = self.defs.keys().cloned().collect();
        for loc in locs {
            self.register_asset_paths_for_entry(&loc, frame)?;
        }
        Ok(())
    }

    fn register_asset_paths_for_entry(
        &mut self,
        loc: &NodeDefLoc,
        frame: Revision,
    ) -> Result<(), RegistryError> {
        let Some(entry) = self.defs.get(loc) else {
            return Ok(());
        };
        let NodeDefState::Loaded(def) = entry.state.clone() else {
            return Ok(());
        };
        let containing = loc.artifact.file_path().cloned().ok_or_else(|| {
            RegistryError::SpecifierResolution {
                message: alloc::format!("missing artifact path for def {loc:?}"),
            }
        })?;

        for path in source_bridge::asset_paths_for_def(&def, containing.as_path())? {
            self.store.register_file(path, frame);
        }
        Ok(())
    }

    fn classify_changed_path(&self, path: &LpPath) -> PathChangeKind {
        let Some(location) = self.store.location_for_path(path) else {
            return PathChangeKind::SourceOnly;
        };
        let source = NodeDefLoc::artifact_root(location.clone());
        if self.defs.contains_key(&source) {
            PathChangeKind::DefArtifact(location)
        } else {
            PathChangeKind::SourceOnly
        }
    }

    pub(crate) fn snapshot_def_states(&self) -> BTreeMap<NodeDefLoc, NodeDefState> {
        self.defs
            .iter()
            .map(|(loc, entry)| (loc.clone(), entry.state.clone()))
            .collect()
    }
}

#[path = "commit.rs"]
mod commit;

#[path = "effective_read.rs"]
mod effective_read;

#[path = "projection.rs"]
mod projection;

#[path = "slot_apply.rs"]
mod slot_apply;

#[cfg(feature = "diff")]
pub(crate) use slot_apply::apply_ops_to_node_def;
pub use slot_apply::serialize_slot_draft;

enum PathChangeKind {
    DefArtifact(ArtifactLoc),
    SourceOnly,
}

fn state_changed(before: &NodeDefState, after: &NodeDefState) -> bool {
    match (before, after) {
        (NodeDefState::Loaded(b), NodeDefState::Loaded(a)) => {
            if is_container_def(b) {
                shell_changed(b, a)
            } else {
                super::def_shell::body_changed(b, a)
            }
        }
        _ => before != after,
    }
}

pub(crate) fn build_change_details(
    before: &BTreeMap<NodeDefLoc, NodeDefState>,
    updates: &NodeDefUpdates,
    entries: &BTreeMap<NodeDefLoc, NodeDefEntry>,
) -> Vec<(NodeDefLoc, DefChangeDetail)> {
    updates
        .changed
        .iter()
        .filter_map(|loc| {
            let before_state = before.get(loc)?;
            let after_state = entries.get(loc).map(|entry| &entry.state)?;
            Some((loc.clone(), classify_def_change(before_state, after_state)))
        })
        .collect()
}

fn classify_def_change(before: &NodeDefState, after: &NodeDefState) -> DefChangeDetail {
    match (before, after) {
        (_, NodeDefState::ParseError(_)) if !matches!(before, NodeDefState::ParseError(_)) => {
            DefChangeDetail::EnteredError
        }
        (NodeDefState::ParseError(_), NodeDefState::Loaded(_)) => DefChangeDetail::LeftError,
        (NodeDefState::Loaded(b), NodeDefState::Loaded(a)) if b.kind() != a.kind() => {
            DefChangeDetail::KindChanged {
                from: b.kind(),
                to: a.kind(),
            }
        }
        _ => DefChangeDetail::Content,
    }
}

pub(crate) fn dedupe_locations(locations: &mut Vec<ArtifactLoc>) {
    locations.sort_unstable();
    locations.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use lpc_model::{NodeKind, SlotShapeRegistry};
    use lpfs::{FsEventKind, LpFsMemory};

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
