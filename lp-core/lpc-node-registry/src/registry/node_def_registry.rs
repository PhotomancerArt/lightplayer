//! Parsed node definition registry driven by artifact freshness.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{NodeDef, NodeInvocation, Revision, SlotPath};
use lpfs::{FsEvent, LpFs, LpPath, LpPathBuf};

use crate::edit::apply::apply_asset_op;
use crate::edit::{
    ArtifactEdit, CommitError, EditBatch, EditError, EditTarget, SlotOverlay, require_absolute_path,
};
use crate::{ArtifactLocation, ArtifactStore};

use super::def_shell::{is_container_def, shell_changed};
use super::def_walker::{collect_invocations, resolve_node_specifier};
use super::source_bridge;
use super::source_deps::SourceDep;
use super::sync_error::SyncError;
use super::sync_op::SyncOp;
use super::sync_outcome::SyncOutcome;
use super::sync_result::{DefChangeDetail, SourceRevisionBump, SyncResult};
use super::{
    NodeDefEntry, NodeDefId, NodeDefLoc, NodeDefState, NodeDefUpdates, ParseCtx, RegistryError,
};

/// Owner of parsed node definitions keyed by [`NodeDefId`].
///
/// Bootstrap with [`Self::load_root`], react to filesystem edits via
/// [`Self::sync`] / [`Self::sync_fs`], and apply client edits through
/// [`Self::apply_edit_batch`] → [`Self::commit`] or [`Self::discard_slot_overlay`].
/// Effective reads use [`crate::NodeDefView`].
pub struct NodeDefRegistry {
    store: ArtifactStore,
    slot_overlay: SlotOverlay,
    entries: BTreeMap<NodeDefId, NodeDefEntry>,
    source_index: BTreeMap<NodeDefLoc, NodeDefId>,
    def_source_deps: BTreeMap<NodeDefId, Vec<SourceDep>>,
    source_path_index: BTreeMap<String, Vec<NodeDefId>>,
    root_id: Option<NodeDefId>,
    next_id: u32,
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
            slot_overlay: SlotOverlay::new(),
            entries: BTreeMap::new(),
            source_index: BTreeMap::new(),
            def_source_deps: BTreeMap::new(),
            source_path_index: BTreeMap::new(),
            root_id: None,
            next_id: 1,
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
    ) -> Result<NodeDefId, RegistryError> {
        if !self.entries.is_empty() {
            return Err(RegistryError::NotEmpty);
        }
        if !root_path.is_absolute() {
            return Err(RegistryError::InvalidPath {
                message: alloc::format!("root path must be absolute: `{}`", root_path.as_str()),
            });
        }
        let path_buf = root_path.to_path_buf();
        let location = self.store.register_file(path_buf.clone(), frame);
        let root_id = self.register_artifact_subtree(location, root_path, frame, fs, ctx)?;
        self.root_id = Some(root_id);
        self.refresh_all_source_deps(fs, frame, ctx);
        Ok(root_id)
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
                SyncOp::Apply(edit) => {
                    self.apply_artifact_edit(&edit, fs, ctx, frame)?;
                    pending_changed = true;
                }
                SyncOp::Remove(target) => {
                    pending_changed |= self.remove_pending_edit(target)?;
                }
                SyncOp::ClearPending => {
                    if self.slot_overlay_active() {
                        self.slot_overlay.clear();
                        pending_changed = true;
                    }
                }
                SyncOp::Commit => {
                    let had_pending = self.slot_overlay_active();
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
        let mut source_revisions = Vec::new();

        let mut def_artifact_locations = Vec::new();
        let mut source_paths = Vec::new();
        for change in changes {
            match self.classify_changed_path(&change.path) {
                PathChangeKind::DefArtifact(location) => def_artifact_locations.push(location),
                PathChangeKind::SourceOnly => source_paths.push(change.path.clone()),
            }
        }
        dedupe_locations(&mut def_artifact_locations);
        dedupe_paths(&mut source_paths);

        for location in def_artifact_locations {
            self.sync_def_artifact(location, fs, frame, ctx, &mut def_updates);
        }

        for path in source_paths {
            self.sync_source_path(&path, fs, frame, ctx, &mut source_revisions);
        }

        let _ = self.reconcile_artifacts();

        let change_details = build_change_details(&before, &def_updates, &self.entries);
        SyncResult {
            def_updates,
            source_revisions,
            change_details,
        }
    }

    /// Drop pending overlay entry for `target`. Returns whether an entry existed.
    pub fn remove_pending_edit(&mut self, target: EditTarget) -> Result<bool, EditError> {
        let path = self.resolve_edit_target(target)?;
        Ok(self.slot_overlay.remove_path(LpPath::new(path.as_str())))
    }

    pub fn root_id(&self) -> Option<NodeDefId> {
        self.root_id
    }

    pub fn get(&self, id: &NodeDefId) -> Option<&NodeDefEntry> {
        self.entries.get(id)
    }

    pub fn get_by_source(&self, source: &NodeDefLoc) -> Option<&NodeDefEntry> {
        self.source_index
            .get(source)
            .and_then(|id| self.entries.get(id))
    }

    /// Iterate registered entries (stable order by id).
    pub fn iter_entries(&self) -> impl Iterator<Item = &NodeDefEntry> {
        self.entries.values()
    }

    /// Apply one artifact change block to the overlay. Committed state unchanged.
    pub fn apply_artifact_edit(
        &mut self,
        change: &ArtifactEdit,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
        frame: Revision,
    ) -> Result<(), EditError> {
        let path = self.resolve_edit_target(change.target().clone())?;
        match change {
            ArtifactEdit::Asset { ops, .. } => {
                for op in ops {
                    apply_asset_op(&mut self.slot_overlay, path.clone(), op)?;
                }
            }
            ArtifactEdit::Slot { ops, .. } => {
                for op in ops {
                    self.apply_slot_op(path.clone(), op, fs, ctx, frame)?;
                }
            }
        }
        Ok(())
    }

    /// Apply an ordered batch to the overlay. Aborts on first error.
    pub fn apply_edit_batch(
        &mut self,
        batch: &EditBatch,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
        frame: Revision,
    ) -> Result<(), EditError> {
        for change in &batch.edits {
            self.apply_artifact_edit(change, fs, ctx, frame)?;
        }
        Ok(())
    }

    /// Drop all pending overlay edits.
    pub fn discard_slot_overlay(&mut self) {
        self.slot_overlay.clear();
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

    pub(crate) fn restore_entry_states(&mut self, before: &BTreeMap<NodeDefId, NodeDefState>) {
        for (id, state) in before {
            if let Some(entry) = self.entries.get_mut(id) {
                entry.state = state.clone();
            }
        }
    }

    /// Whether any overlay entries are pending.
    pub fn slot_overlay_active(&self) -> bool {
        !self.slot_overlay.is_empty()
    }

    /// Whether `path` has a pending overlay entry.
    pub fn slot_overlay_contains_path(&self, path: &LpPath) -> bool {
        self.slot_overlay.contains_path(path)
    }

    /// Pending overlay bytes for `path`, if any.
    pub fn slot_overlay_bytes(&self, path: &LpPath) -> Option<&[u8]> {
        self.slot_overlay.get_bytes(path)
    }

    pub(crate) fn artifact_location_for_path(&self, path: &LpPath) -> Option<ArtifactLocation> {
        self.store.location_for_path(path)
    }

    pub(crate) fn read_committed_artifact_bytes(
        &mut self,
        location: &ArtifactLocation,
        fs: &dyn LpFs,
    ) -> Result<Vec<u8>, crate::ArtifactError> {
        self.store.read_bytes(location, fs)
    }

    fn resolve_edit_target(&self, target: EditTarget) -> Result<LpPathBuf, EditError> {
        match target {
            EditTarget::Path(path) => require_absolute_path(path),
            EditTarget::Location(location) => location
                .file_path()
                .cloned()
                .ok_or_else(|| EditError::UnknownArtifact {
                    location: location.clone(),
                })
                .and_then(|path| {
                    if self.store.entry(&location).is_some() {
                        Ok(path)
                    } else {
                        Err(EditError::UnknownArtifact { location })
                    }
                }),
        }
    }

    fn register_artifact_subtree(
        &mut self,
        location: ArtifactLocation,
        file_path: &LpPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefId, RegistryError> {
        let revision = self.store.revision(&location).unwrap_or(frame);
        let state = self.read_artifact_state(&location, fs, ctx)?;
        let source = NodeDefLoc::artifact_root(location.clone());
        let root_id = self.register_def_at_source(source, state.clone(), revision)?;
        if let NodeDefState::Loaded(def) = state {
            self.register_invocations(&location, file_path, def, SlotPath::root(), frame, fs, ctx)?;
        }
        Ok(root_id)
    }

    fn register_invocations(
        &mut self,
        location: &ArtifactLocation,
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
                    let specifier =
                        lpc_model::ArtifactSpecifier::parse(path_text).map_err(|err| {
                            RegistryError::SpecifierResolution {
                                message: String::from(err),
                            }
                        })?;
                    let child_path = resolve_node_specifier(file_path, &specifier)?;
                    let child_location = self.store.register_file(child_path.clone(), frame);
                    let child_source = NodeDefLoc::artifact_root(child_location.clone());
                    if !self.source_index.contains_key(&child_source) {
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
        location: ArtifactLocation,
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

        let old_sources: BTreeMap<NodeDefLoc, NodeDefId> = self
            .entries
            .values()
            .filter(|entry| entry.loc.artifact == location)
            .map(|entry| (entry.loc.clone(), entry.id))
            .collect();

        for (source, id) in &old_sources {
            if !new_inventory.contains_key(source) {
                updates.push_removed(*id);
                self.remove_entry(*id);
            }
        }

        let mut affected = Vec::new();
        for (source, new_state) in &new_inventory {
            if let Some(id) = old_sources.get(source) {
                let Some(entry) = self.entries.get(id) else {
                    continue;
                };
                if state_changed(&entry.state, new_state) {
                    updates.push_changed(*id);
                    if let Some(entry) = self.entries.get_mut(id) {
                        entry.state = new_state.clone();
                        entry.revision = current;
                    }
                    affected.push(*id);
                }
            } else if let Ok(id) =
                self.register_def_at_source(source.clone(), new_state.clone(), current)
            {
                updates.push_added(id);
                affected.push(id);
            }
        }

        for def_id in affected {
            let _ = self.refresh_source_deps_for_entry(def_id, fs, frame, ctx);
        }
    }

    pub(crate) fn sync_source_path(
        &mut self,
        path: &LpPath,
        fs: &dyn LpFs,
        frame: Revision,
        _ctx: &ParseCtx<'_>,
        out: &mut Vec<SourceRevisionBump>,
    ) {
        let key = String::from(path.as_str());
        let Some(def_ids) = self.source_path_index.get(&key).cloned() else {
            return;
        };

        for def_id in def_ids {
            let Some(deps) = self.def_source_deps.get_mut(&def_id) else {
                continue;
            };
            let Some(entry) = self.entries.get(&def_id) else {
                continue;
            };
            let NodeDefState::Loaded(def) = entry.state.clone() else {
                continue;
            };
            let Some(containing) = entry.loc.artifact.file_path().cloned() else {
                continue;
            };

            for dep in deps.iter_mut() {
                if dep.resolved_path.as_str() != path.as_str() {
                    continue;
                }
                let before = dep.last_version;
                let after = match source_bridge::materialize_version_for_def_path(
                    &mut self.store,
                    fs,
                    containing.as_path(),
                    &def,
                    &dep.resolved_path,
                    frame,
                ) {
                    Ok(version) => version,
                    Err(_) => continue,
                };
                if after > before {
                    out.push(SourceRevisionBump {
                        def_id,
                        before,
                        after,
                    });
                    dep.last_version = after;
                }
            }
        }
    }

    fn derive_inventory(
        &mut self,
        location: ArtifactLocation,
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
        location: &ArtifactLocation,
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
                    let specifier =
                        lpc_model::ArtifactSpecifier::parse(path_text).map_err(|err| {
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
        location: &ArtifactLocation,
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
    ) -> ArtifactLocation {
        self.store.register_file(path, frame)
    }

    fn referenced_locations(&self) -> alloc::collections::BTreeSet<ArtifactLocation> {
        let mut referenced = self
            .entries
            .values()
            .map(|entry| entry.loc.artifact.clone())
            .collect::<alloc::collections::BTreeSet<_>>();

        for deps in self.def_source_deps.values() {
            for dep in deps {
                if let Some(location) = self.store.location_for_path(dep.resolved_path.as_path()) {
                    referenced.insert(location);
                }
            }
        }

        referenced
    }

    pub(crate) fn reconcile_artifacts(&mut self) -> Result<(), RegistryError> {
        let referenced = self.referenced_locations();
        let to_unregister: Vec<ArtifactLocation> = self
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
    ) -> Result<NodeDefId, RegistryError> {
        if self.source_index.contains_key(&source) {
            return Err(RegistryError::DuplicateSource);
        }
        let id = self.alloc_id();
        self.source_index.insert(source.clone(), id);
        self.entries.insert(
            id,
            NodeDefEntry {
                id,
                loc: source,
                state,
                revision: revision,
            },
        );
        Ok(id)
    }

    fn remove_entry(&mut self, id: NodeDefId) {
        self.remove_def_from_source_index(id);
        if let Some(entry) = self.entries.remove(&id) {
            self.source_index.remove(&entry.loc);
        }
    }

    fn refresh_all_source_deps(&mut self, fs: &dyn LpFs, frame: Revision, ctx: &ParseCtx<'_>) {
        let ids: Vec<NodeDefId> = self.entries.keys().copied().collect();
        for id in ids {
            let _ = self.refresh_source_deps_for_entry(id, fs, frame, ctx);
        }
    }

    fn refresh_source_deps_for_entry(
        &mut self,
        def_id: NodeDefId,
        fs: &dyn LpFs,
        frame: Revision,
        _ctx: &ParseCtx<'_>,
    ) -> Result<(), RegistryError> {
        self.remove_def_from_source_index(def_id);

        let Some(entry) = self.entries.get(&def_id) else {
            return Ok(());
        };
        let NodeDefState::Loaded(def) = entry.state.clone() else {
            return Ok(());
        };
        let containing = entry.loc.artifact.file_path().cloned().ok_or_else(|| {
            RegistryError::SpecifierResolution {
                message: alloc::format!("missing artifact path for def {def_id:?}"),
            }
        })?;

        let paths = source_bridge::source_paths_for_def(&def, containing.as_path())?;
        let mut deps = Vec::new();
        for resolved in paths {
            self.store.register_file(resolved.clone(), frame);
            let version = source_bridge::materialize_version_for_def_path(
                &mut self.store,
                fs,
                containing.as_path(),
                &def,
                &resolved,
                frame,
            )?;
            self.index_source_dep(def_id, &resolved);
            deps.push(SourceDep {
                resolved_path: resolved,
                last_version: version,
            });
        }
        self.def_source_deps.insert(def_id, deps);
        Ok(())
    }

    fn remove_def_from_source_index(&mut self, def_id: NodeDefId) {
        if let Some(deps) = self.def_source_deps.remove(&def_id) {
            for dep in deps {
                let key = String::from(dep.resolved_path.as_str());
                if let Some(list) = self.source_path_index.get_mut(&key) {
                    list.retain(|id| *id != def_id);
                    if list.is_empty() {
                        self.source_path_index.remove(&key);
                    }
                }
            }
        }
    }

    fn index_source_dep(&mut self, def_id: NodeDefId, path: &LpPathBuf) {
        let key = String::from(path.as_str());
        let list = self.source_path_index.entry(key).or_default();
        if !list.contains(&def_id) {
            list.push(def_id);
        }
    }

    fn classify_changed_path(&self, path: &LpPath) -> PathChangeKind {
        let Some(location) = self.store.location_for_path(path) else {
            return PathChangeKind::SourceOnly;
        };
        let source = NodeDefLoc::artifact_root(location.clone());
        if self.source_index.contains_key(&source) {
            PathChangeKind::DefArtifact(location)
        } else {
            PathChangeKind::SourceOnly
        }
    }

    pub(crate) fn snapshot_def_states(&self) -> BTreeMap<NodeDefId, NodeDefState> {
        self.entries
            .iter()
            .map(|(id, entry)| (*id, entry.state.clone()))
            .collect()
    }

    fn alloc_id(&mut self) -> NodeDefId {
        let id = NodeDefId::from_raw(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        if self.next_id == 0 {
            self.next_id = 1;
        }
        id
    }
}

#[path = "commit.rs"]
mod commit;

#[path = "effective_read.rs"]
mod effective_read;

#[path = "slot_apply.rs"]
mod slot_apply;

#[cfg(feature = "diff")]
pub(crate) use slot_apply::apply_ops_to_node_def;
pub use slot_apply::serialize_slot_draft;

enum PathChangeKind {
    DefArtifact(ArtifactLocation),
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
    before: &BTreeMap<NodeDefId, NodeDefState>,
    updates: &NodeDefUpdates,
    entries: &BTreeMap<NodeDefId, NodeDefEntry>,
) -> Vec<(NodeDefId, DefChangeDetail)> {
    updates
        .changed
        .iter()
        .filter_map(|id| {
            let before_state = before.get(id)?;
            let after_state = entries.get(id).map(|entry| &entry.state)?;
            Some((*id, classify_def_change(before_state, after_state)))
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

pub(crate) fn dedupe_locations(locations: &mut Vec<ArtifactLocation>) {
    locations.sort_unstable();
    locations.dedup();
}

pub(crate) fn dedupe_paths(paths: &mut Vec<LpPathBuf>) {
    paths.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
    paths.dedup_by(|a, b| a.as_str() == b.as_str());
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

    fn changed_set(updates: &NodeDefUpdates) -> BTreeSet<NodeDefId> {
        updates.changed.iter().copied().collect()
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
        assert_eq!(registry.entries.len(), 2);
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
        assert_eq!(changed_set(&result.def_updates), BTreeSet::from([root]));
        assert!(matches!(
            result.change_details.as_slice(),
            [(id, DefChangeDetail::Content)] if *id == root
        ));
    }

    #[test]
    fn glsl_edit_only_bumps_source_revision() {
        let mut fs = crate::harness::fixtures::load_shader_project();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let shader_id = registry
            .load_root(&fs, LpPath::new("/shader.toml"), Revision::new(1), &ctx)
            .unwrap();

        crate::harness::fixtures::write_file(
            &mut fs,
            "/shader.glsl",
            "void main() { gl_FragColor = vec4(0.0); }",
        );
        let result = registry.sync_fs(&fs, &[fs_modify("/shader.glsl")], Revision::new(2), &ctx);
        assert!(result.def_updates.is_empty());
        assert_eq!(result.source_revisions.len(), 1);
        assert_eq!(result.source_revisions[0].def_id, shader_id);
        assert!(result.source_revisions[0].after > result.source_revisions[0].before);
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
            .entries
            .values()
            .find(|entry| !entry.loc.path.is_root())
            .expect("inline child")
            .id;

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
        assert!(!result.def_updates.contains_changed(root));
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
        assert!(result.def_updates.contains_changed(root));
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
            .entries
            .values()
            .find(|entry| entry.loc.path.is_root() && entry.id != root)
            .expect("child file root")
            .id;

        crate::harness::fixtures::write_file(
            &mut fs,
            "/active.toml",
            r#"
kind = "Shader"
source = { path = "b.glsl" }
"#,
        );
        let result = registry.sync_fs(&fs, &[fs_modify("/active.toml")], Revision::new(2), &ctx);
        assert!(!result.def_updates.contains_changed(root));
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
            .entries
            .values()
            .find(|entry| !entry.loc.path.is_root())
            .expect("inline child")
            .id;

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
        assert!(result.def_updates.contains_changed(root));
        assert!(result.def_updates.contains_changed(child));
        assert!(result.change_details.iter().any(|(id, detail)| *id == child
            && matches!(
                detail,
                DefChangeDetail::KindChanged {
                    from: NodeKind::Shader,
                    to: NodeKind::Clock
                }
            )));
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
        assert!(result.def_updates.contains_changed(root));
        assert!(matches!(
            result.change_details.as_slice(),
            [(id, DefChangeDetail::EnteredError)] if *id == root
        ));
    }
}
