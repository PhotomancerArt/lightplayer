//! Parsed node definition registry driven by artifact freshness.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{NodeDef, NodeDefParseError, NodeDefRef, Revision, SlotPath};
use lpfs::{FsChange, LpFs, LpPath, LpPathBuf};

use crate::change::apply::apply_op;
use crate::change::{
    ArtifactChange, ArtifactTarget, ChangeError, ChangeOverlay, ChangeSet, require_absolute_path,
};
use crate::{ArtifactError, ArtifactId, ArtifactLocation, ArtifactStore};

use super::def_shell::{is_container_def, shell_changed};
use super::def_walker::{collect_invocations, resolve_node_locator};
use super::registry_change::RegistryChange;
use super::source_bridge;
use super::source_deps::SourceDep;
use super::sync_result::{DefChangeDetail, SourceRevisionBump, SyncResult};
use super::{
    DefSource, NodeDefEntry, NodeDefId, NodeDefState, NodeDefUpdates, ParseCtx, RegistryError,
};

/// Owner of parsed node definitions keyed by [`NodeDefId`].
///
/// Bootstrap with [`Self::load_root`], then apply filesystem changes via
/// [`Self::sync`] or [`Self::sync_fs`].
pub struct NodeDefRegistry {
    store: ArtifactStore,
    overlay: ChangeOverlay,
    entries: BTreeMap<NodeDefId, NodeDefEntry>,
    source_index: BTreeMap<DefSource, NodeDefId>,
    artifact_refs: BTreeMap<ArtifactId, u32>,
    artifact_root_path: BTreeMap<ArtifactId, LpPathBuf>,
    artifact_path_to_id: BTreeMap<String, ArtifactId>,
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
            overlay: ChangeOverlay::new(),
            entries: BTreeMap::new(),
            source_index: BTreeMap::new(),
            artifact_refs: BTreeMap::new(),
            artifact_root_path: BTreeMap::new(),
            artifact_path_to_id: BTreeMap::new(),
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
        let artifact_id = self.acquire_file_artifact(path_buf.clone(), frame)?;
        let root_id = self.register_artifact_subtree(artifact_id, root_path, frame, fs, ctx)?;
        self.root_id = Some(root_id);
        self.refresh_all_source_deps(fs, frame, ctx);
        Ok(root_id)
    }

    /// Apply incoming changes, update internal state, return summary.
    pub fn sync(
        &mut self,
        fs: &dyn LpFs,
        changes: &[RegistryChange],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> SyncResult {
        let before = self.snapshot_def_states();

        let fs_changes: Vec<FsChange> = changes
            .iter()
            .filter_map(|change| match change {
                RegistryChange::Fs(fs_change) => Some(fs_change.clone()),
            })
            .collect();
        if !fs_changes.is_empty() {
            self.store.apply_fs_changes(&fs_changes, frame);
        }

        let mut def_updates = NodeDefUpdates::default();
        let mut source_revisions = Vec::new();

        let mut def_artifact_ids = Vec::new();
        let mut source_paths = Vec::new();
        for change in &fs_changes {
            match self.classify_changed_path(&change.path) {
                PathChangeKind::DefArtifact(artifact_id) => def_artifact_ids.push(artifact_id),
                PathChangeKind::SourceOnly => source_paths.push(change.path.clone()),
            }
        }
        dedupe_artifact_ids(&mut def_artifact_ids);
        dedupe_paths(&mut source_paths);

        for artifact_id in def_artifact_ids {
            self.sync_def_artifact(artifact_id, fs, frame, ctx, &mut def_updates);
        }

        for path in source_paths {
            self.sync_source_path(&path, fs, frame, ctx, &mut source_revisions);
        }

        let _ = self.reconcile_artifact_refs(frame);

        let change_details = build_change_details(&before, &def_updates, &self.entries);
        SyncResult {
            def_updates,
            source_revisions,
            change_details,
        }
    }

    /// Convenience wrapper mapping [`FsChange`] batches to [`RegistryChange::Fs`].
    pub fn sync_fs(
        &mut self,
        fs: &dyn LpFs,
        changes: &[FsChange],
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> SyncResult {
        let registry_changes: Vec<RegistryChange> =
            changes.iter().cloned().map(RegistryChange::Fs).collect();
        self.sync(fs, &registry_changes, frame, ctx)
    }

    pub fn root_id(&self) -> Option<NodeDefId> {
        self.root_id
    }

    pub fn get(&self, id: &NodeDefId) -> Option<&NodeDefEntry> {
        self.entries.get(id)
    }

    pub fn get_by_source(&self, source: &DefSource) -> Option<&NodeDefEntry> {
        self.source_index
            .get(source)
            .and_then(|id| self.entries.get(id))
    }

    /// Iterate registered entries (stable order by id).
    pub fn iter_entries(&self) -> impl Iterator<Item = &NodeDefEntry> {
        self.entries.values()
    }

    /// Apply one artifact change block to the overlay. Committed state unchanged.
    pub fn apply_change(&mut self, change: &ArtifactChange) -> Result<(), ChangeError> {
        let path = self.resolve_change_target(change.target.clone())?;
        for op in &change.ops {
            apply_op(&mut self.overlay, path.clone(), op)?;
        }
        Ok(())
    }

    /// Apply an ordered changeset to the overlay. Aborts on first error.
    pub fn apply_changeset(&mut self, changeset: &ChangeSet) -> Result<(), ChangeError> {
        for change in &changeset.changes {
            self.apply_change(change)?;
        }
        Ok(())
    }

    /// Drop all pending overlay edits.
    pub fn discard_overlay(&mut self) {
        self.overlay.clear();
    }

    /// Whether any overlay entries are pending.
    pub fn overlay_active(&self) -> bool {
        !self.overlay.is_empty()
    }

    /// Whether `path` has a pending overlay entry.
    pub fn overlay_contains_path(&self, path: &LpPath) -> bool {
        self.overlay.contains_path(path)
    }

    /// Pending overlay bytes for `path`, if any.
    pub fn overlay_bytes(&self, path: &LpPath) -> Option<&[u8]> {
        self.overlay.get_bytes(path)
    }

    fn resolve_change_target(&self, target: ArtifactTarget) -> Result<LpPathBuf, ChangeError> {
        match target {
            ArtifactTarget::Path(path) => require_absolute_path(path),
            ArtifactTarget::Id(id) => {
                self.artifact_root_path
                    .get(&id)
                    .cloned()
                    .ok_or(ChangeError::UnknownArtifact {
                        artifact_id: id.handle(),
                    })
            }
        }
    }

    fn register_artifact_subtree(
        &mut self,
        artifact_id: ArtifactId,
        file_path: &LpPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefId, RegistryError> {
        let revision = self.store.revision(&artifact_id).unwrap_or(frame);
        let state = self.read_artifact_state(artifact_id, fs, ctx)?;
        let source = DefSource::artifact_root(artifact_id);
        let root_id = self.register_def_at_source(source, state.clone(), revision)?;
        if let NodeDefState::Loaded(def) = state {
            self.register_invocations(
                artifact_id,
                file_path,
                def,
                SlotPath::root(),
                frame,
                fs,
                ctx,
            )?;
        }
        Ok(root_id)
    }

    fn register_invocations(
        &mut self,
        artifact_id: ArtifactId,
        file_path: &LpPath,
        def: NodeDef,
        base_path: SlotPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<(), RegistryError> {
        for site in collect_invocations(&def, &base_path) {
            match &site.invocation.def {
                NodeDefRef::Path(locator) => {
                    let child_path = resolve_node_locator(file_path, locator)?;
                    let child_artifact = self.acquire_file_artifact(child_path.clone(), frame)?;
                    let child_source = DefSource::artifact_root(child_artifact);
                    if !self.source_index.contains_key(&child_source) {
                        self.register_artifact_subtree(
                            child_artifact,
                            child_path.as_path(),
                            frame,
                            fs,
                            ctx,
                        )?;
                    }
                }
                NodeDefRef::Inline(body) => {
                    let source = DefSource {
                        artifact_id,
                        path: site.path.clone(),
                    };
                    let revision = self.store.revision(&artifact_id).unwrap_or(frame);
                    self.register_def_at_source(
                        source,
                        NodeDefState::Loaded((**body).clone()),
                        revision,
                    )?;
                    self.register_invocations(
                        artifact_id,
                        file_path,
                        (**body).clone(),
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

    fn sync_def_artifact(
        &mut self,
        artifact_id: ArtifactId,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
        updates: &mut NodeDefUpdates,
    ) {
        let Some(current) = self.store.revision(&artifact_id) else {
            return;
        };
        let Some(file_path) = self.artifact_root_path.get(&artifact_id).cloned() else {
            return;
        };

        let new_inventory =
            match self.derive_inventory(artifact_id, file_path.as_path(), frame, fs, ctx) {
                Ok(inventory) => inventory,
                Err(_) => return,
            };

        let old_sources: BTreeMap<DefSource, NodeDefId> = self
            .entries
            .values()
            .filter(|entry| entry.source.artifact_id == artifact_id)
            .map(|entry| (entry.source.clone(), entry.id))
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
                        entry.last_seen_revision = current;
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

    fn sync_source_path(
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
            let Some(containing) = self
                .artifact_root_path
                .get(&entry.source.artifact_id)
                .cloned()
            else {
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
        artifact_id: ArtifactId,
        file_path: &LpPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<BTreeMap<DefSource, NodeDefState>, RegistryError> {
        let mut inventory = BTreeMap::new();
        let state = self.read_artifact_state(artifact_id, fs, ctx)?;
        inventory.insert(DefSource::artifact_root(artifact_id), state.clone());
        if let NodeDefState::Loaded(def) = state {
            self.derive_invocations(
                artifact_id,
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
        artifact_id: ArtifactId,
        file_path: &LpPath,
        def: NodeDef,
        base_path: SlotPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
        inventory: &mut BTreeMap<DefSource, NodeDefState>,
    ) -> Result<(), RegistryError> {
        for site in collect_invocations(&def, &base_path) {
            match &site.invocation.def {
                NodeDefRef::Path(locator) => {
                    let child_path = resolve_node_locator(file_path, locator)?;
                    let child_artifact = self.acquire_file_artifact(child_path.clone(), frame)?;
                    let child_inventory = self.derive_inventory(
                        child_artifact,
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
                NodeDefRef::Inline(body) => {
                    let source = DefSource {
                        artifact_id,
                        path: site.path.clone(),
                    };
                    if inventory
                        .insert(source, NodeDefState::Loaded((**body).clone()))
                        .is_some()
                    {
                        return Err(RegistryError::DuplicateSource);
                    }
                    self.derive_invocations(
                        artifact_id,
                        file_path,
                        (**body).clone(),
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

    fn read_artifact_state(
        &mut self,
        artifact_id: ArtifactId,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefState, RegistryError> {
        match self.store.read_bytes(&artifact_id, fs) {
            Ok(bytes) => {
                let text = core::str::from_utf8(&bytes).map_err(|err| RegistryError::Utf8 {
                    message: err.to_string(),
                })?;
                Ok(match NodeDef::read_toml(ctx.shapes, text) {
                    Ok(def) => NodeDefState::Loaded(def),
                    Err(err) => NodeDefState::ParseError(err),
                })
            }
            Err(err) => Ok(NodeDefState::ParseError(read_error_state(err))),
        }
    }

    fn acquire_file_artifact(
        &mut self,
        path: LpPathBuf,
        frame: Revision,
    ) -> Result<ArtifactId, RegistryError> {
        if let Some(id) = self.artifact_path_to_id.get(path.as_str()).copied() {
            return Ok(id);
        }
        let id = self
            .store
            .acquire_location(ArtifactLocation::file(path.clone()), frame);
        self.artifact_path_to_id
            .insert(String::from(path.as_str()), id);
        self.artifact_root_path.insert(id, path);
        *self.artifact_refs.entry(id).or_insert(0) += 1;
        Ok(id)
    }

    fn register_def_at_source(
        &mut self,
        source: DefSource,
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
                source,
                state,
                last_seen_revision: revision,
            },
        );
        Ok(id)
    }

    fn remove_entry(&mut self, id: NodeDefId) {
        self.remove_def_from_source_index(id);
        if let Some(entry) = self.entries.remove(&id) {
            self.source_index.remove(&entry.source);
        }
    }

    fn reconcile_artifact_refs(&mut self, frame: Revision) -> Result<(), RegistryError> {
        let referenced: alloc::collections::BTreeSet<ArtifactId> = self
            .entries
            .values()
            .map(|entry| entry.source.artifact_id)
            .collect();

        let to_release: Vec<ArtifactId> = self
            .artifact_refs
            .keys()
            .copied()
            .filter(|artifact_id| !referenced.contains(artifact_id))
            .collect();

        for artifact_id in to_release {
            self.artifact_refs.remove(&artifact_id);
            if let Some(path) = self.artifact_root_path.remove(&artifact_id) {
                self.artifact_path_to_id.remove(path.as_str());
            }
            self.store.release(&artifact_id, frame)?;
        }
        Ok(())
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
        let containing = self
            .artifact_root_path
            .get(&entry.source.artifact_id)
            .cloned()
            .ok_or_else(|| RegistryError::LocatorResolution {
                message: alloc::format!("missing artifact path for def {def_id:?}"),
            })?;

        let paths = source_bridge::source_paths_for_def(&def, containing.as_path())?;
        let mut deps = Vec::new();
        for resolved in paths {
            self.acquire_file_artifact(resolved.clone(), frame)?;
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
        let Some(artifact_id) = self.artifact_path_to_id.get(path.as_str()).copied() else {
            return PathChangeKind::SourceOnly;
        };
        let source = DefSource::artifact_root(artifact_id);
        if self.source_index.contains_key(&source) {
            PathChangeKind::DefArtifact(artifact_id)
        } else {
            PathChangeKind::SourceOnly
        }
    }

    fn snapshot_def_states(&self) -> BTreeMap<NodeDefId, NodeDefState> {
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

enum PathChangeKind {
    DefArtifact(ArtifactId),
    SourceOnly,
}

fn read_error_state(err: ArtifactError) -> NodeDefParseError {
    NodeDefParseError::Toml {
        error: alloc::format!("artifact read failed: {err:?}"),
    }
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

fn build_change_details(
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

fn dedupe_artifact_ids(ids: &mut Vec<ArtifactId>) {
    ids.sort_unstable();
    ids.dedup();
}

fn dedupe_paths(paths: &mut Vec<LpPathBuf>) {
    paths.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
    paths.dedup_by(|a, b| a.as_str() == b.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use lpc_model::{NodeKind, SlotShapeRegistry};
    use lpfs::{ChangeType, LpFsMemory};

    fn parse_ctx() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }

    fn fs_modify(path: &str) -> FsChange {
        FsChange {
            path: LpPathBuf::from(path),
            change_type: ChangeType::Modify,
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
            .find(|entry| !entry.source.path.is_root())
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
node = { def = { path = "./active.toml" } }
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
            .find(|entry| entry.source.path.is_root() && entry.id != root)
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
            .find(|entry| !entry.source.path.is_root())
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
