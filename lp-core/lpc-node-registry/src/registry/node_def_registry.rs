//! Parsed node definition registry driven by artifact freshness.

use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec::Vec;

use lpc_model::{NodeDef, NodeDefParseError, NodeDefRef, Revision, SlotPath};
use lpfs::{LpFs, LpPath, LpPathBuf};

use crate::{ArtifactError, ArtifactId, ArtifactLocation, ArtifactStore};

use super::def_shell::{is_container_def, shell_changed};
use super::def_walker::{collect_invocations, resolve_node_locator};
use super::{
    DefSource, NodeDefEntry, NodeDefId, NodeDefState, NodeDefUpdates, ParseCtx, RegistryError,
};

/// Owner of parsed node definitions keyed by [`NodeDefId`].
///
/// Bootstrap with [`Self::load_root`], then after the driver applies filesystem
/// changes to [`ArtifactStore`], call [`Self::sync`] for [`NodeDefUpdates`].
pub struct NodeDefRegistry {
    entries: BTreeMap<NodeDefId, NodeDefEntry>,
    source_index: BTreeMap<DefSource, NodeDefId>,
    artifact_refs: BTreeMap<ArtifactId, u32>,
    artifact_root_path: BTreeMap<ArtifactId, LpPathBuf>,
    artifact_path_to_id: BTreeMap<alloc::string::String, ArtifactId>,
    artifact_last_revision: BTreeMap<ArtifactId, Revision>,
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
            entries: BTreeMap::new(),
            source_index: BTreeMap::new(),
            artifact_refs: BTreeMap::new(),
            artifact_root_path: BTreeMap::new(),
            artifact_path_to_id: BTreeMap::new(),
            artifact_last_revision: BTreeMap::new(),
            root_id: None,
            next_id: 1,
        }
    }

    /// Load all defs reachable from a root node-definition TOML file.
    ///
    /// The root kind is not enforced — `project.toml` is convention only.
    pub fn load_root(
        &mut self,
        store: &mut ArtifactStore,
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
        let artifact_id = self.acquire_file_artifact(store, path_buf.clone(), frame)?;
        let root_id =
            self.register_artifact_subtree(store, fs, artifact_id, root_path, frame, ctx)?;
        self.root_id = Some(root_id);
        self.artifact_last_revision
            .insert(artifact_id, store.revision(&artifact_id).unwrap_or(frame));
        Ok(root_id)
    }

    /// Re-derive defs for artifacts whose store revision advanced.
    ///
    /// Call after `store.apply_fs_changes`. A kind change on a bound def requires
    /// runtime delete/recreate in the engine (M6).
    pub fn sync(
        &mut self,
        store: &mut ArtifactStore,
        fs: &dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> NodeDefUpdates {
        let mut updates = NodeDefUpdates::default();
        let artifact_ids: Vec<ArtifactId> = self.artifact_refs.keys().copied().collect();

        for artifact_id in artifact_ids {
            let Some(current) = store.revision(&artifact_id) else {
                continue;
            };
            if self.artifact_last_revision.get(&artifact_id) == Some(&current) {
                continue;
            }

            let Some(file_path) = self.artifact_root_path.get(&artifact_id).cloned() else {
                continue;
            };

            let new_inventory = match self.derive_inventory(
                store,
                fs,
                artifact_id,
                file_path.as_path(),
                frame,
                ctx,
            ) {
                Ok(inventory) => inventory,
                Err(_) => continue,
            };

            let old_sources: BTreeMap<DefSource, NodeDefId> = self
                .entries
                .values()
                .filter(|entry| entry.source.artifact_id == artifact_id)
                .map(|entry| (entry.source.clone(), entry.id))
                .collect();

            for (source, id) in &old_sources {
                if !new_inventory.contains_key(source) {
                    updates.removed.insert(*id);
                    self.remove_entry(*id);
                }
            }

            for (source, new_state) in &new_inventory {
                if let Some(id) = old_sources.get(source) {
                    let Some(entry) = self.entries.get(id) else {
                        continue;
                    };
                    if state_changed(&entry.state, new_state) {
                        updates.changed.insert(*id);
                        if let Some(entry) = self.entries.get_mut(id) {
                            entry.state = new_state.clone();
                            entry.last_seen_revision = current;
                        }
                    }
                } else {
                    match self.register_def_at_source(source.clone(), new_state.clone(), current) {
                        Ok(id) => {
                            updates.added.insert(id);
                        }
                        Err(RegistryError::DuplicateSource) => {}
                        Err(_) => {}
                    }
                }
            }

            self.artifact_last_revision.insert(artifact_id, current);
        }

        let _ = self.reconcile_artifact_refs(store, frame);
        updates
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

    fn register_artifact_subtree(
        &mut self,
        store: &mut ArtifactStore,
        fs: &dyn LpFs,
        artifact_id: ArtifactId,
        file_path: &LpPath,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefId, RegistryError> {
        let revision = store.revision(&artifact_id).unwrap_or(frame);
        let state = self.read_artifact_state(store, fs, artifact_id, ctx)?;
        let source = DefSource::artifact_root(artifact_id);
        let root_id = self.register_def_at_source(source, state.clone(), revision)?;
        if let NodeDefState::Loaded(def) = state {
            self.register_invocations(
                store,
                fs,
                artifact_id,
                file_path,
                def,
                SlotPath::root(),
                frame,
                ctx,
            )?;
        }
        Ok(root_id)
    }

    fn register_invocations(
        &mut self,
        store: &mut ArtifactStore,
        fs: &dyn LpFs,
        artifact_id: ArtifactId,
        file_path: &LpPath,
        def: NodeDef,
        base_path: SlotPath,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<(), RegistryError> {
        for site in collect_invocations(&def, &base_path) {
            match &site.invocation.def {
                NodeDefRef::Path(locator) => {
                    let child_path = resolve_node_locator(file_path, locator)?;
                    let child_artifact =
                        self.acquire_file_artifact(store, child_path.clone(), frame)?;
                    let child_source = DefSource::artifact_root(child_artifact);
                    if !self.source_index.contains_key(&child_source) {
                        self.register_artifact_subtree(
                            store,
                            fs,
                            child_artifact,
                            child_path.as_path(),
                            frame,
                            ctx,
                        )?;
                    }
                }
                NodeDefRef::Inline(body) => {
                    let source = DefSource {
                        artifact_id,
                        path: site.path.clone(),
                    };
                    let revision = store.revision(&artifact_id).unwrap_or(frame);
                    self.register_def_at_source(
                        source,
                        NodeDefState::Loaded((**body).clone()),
                        revision,
                    )?;
                    self.register_invocations(
                        store,
                        fs,
                        artifact_id,
                        file_path,
                        (**body).clone(),
                        site.path,
                        frame,
                        ctx,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn derive_inventory(
        &mut self,
        store: &mut ArtifactStore,
        fs: &dyn LpFs,
        artifact_id: ArtifactId,
        file_path: &LpPath,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<BTreeMap<DefSource, NodeDefState>, RegistryError> {
        let mut inventory = BTreeMap::new();
        let state = self.read_artifact_state(store, fs, artifact_id, ctx)?;
        inventory.insert(DefSource::artifact_root(artifact_id), state.clone());
        if let NodeDefState::Loaded(def) = state {
            self.derive_invocations(
                store,
                fs,
                artifact_id,
                file_path,
                def,
                SlotPath::root(),
                frame,
                ctx,
                &mut inventory,
            )?;
        }
        Ok(inventory)
    }

    fn derive_invocations(
        &mut self,
        store: &mut ArtifactStore,
        fs: &dyn LpFs,
        artifact_id: ArtifactId,
        file_path: &LpPath,
        def: NodeDef,
        base_path: SlotPath,
        frame: Revision,
        ctx: &ParseCtx<'_>,
        inventory: &mut BTreeMap<DefSource, NodeDefState>,
    ) -> Result<(), RegistryError> {
        for site in collect_invocations(&def, &base_path) {
            match &site.invocation.def {
                NodeDefRef::Path(locator) => {
                    let child_path = resolve_node_locator(file_path, locator)?;
                    let child_artifact =
                        self.acquire_file_artifact(store, child_path.clone(), frame)?;
                    let child_inventory = self.derive_inventory(
                        store,
                        fs,
                        child_artifact,
                        child_path.as_path(),
                        frame,
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
                        store,
                        fs,
                        artifact_id,
                        file_path,
                        (**body).clone(),
                        site.path,
                        frame,
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
        store: &mut ArtifactStore,
        fs: &dyn LpFs,
        artifact_id: ArtifactId,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefState, RegistryError> {
        match store.read_bytes(&artifact_id, fs) {
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
        store: &mut ArtifactStore,
        path: LpPathBuf,
        frame: Revision,
    ) -> Result<ArtifactId, RegistryError> {
        if let Some(id) = self.artifact_path_to_id.get(path.as_str()).copied() {
            return Ok(id);
        }
        let id = store.acquire_location(ArtifactLocation::file(path.clone()), frame);
        self.artifact_path_to_id
            .insert(alloc::string::String::from(path.as_str()), id);
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
        if let Some(entry) = self.entries.remove(&id) {
            self.source_index.remove(&entry.source);
        }
    }

    fn reconcile_artifact_refs(
        &mut self,
        store: &mut ArtifactStore,
        frame: Revision,
    ) -> Result<(), RegistryError> {
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
            self.artifact_last_revision.remove(&artifact_id);
            if let Some(path) = self.artifact_root_path.remove(&artifact_id) {
                self.artifact_path_to_id.remove(path.as_str());
            }
            store.release(&artifact_id, frame)?;
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeSet;
    use lpc_model::SlotShapeRegistry;
    use lpfs::{ChangeType, FsChange, LpFsMemory};

    fn parse_ctx() -> SlotShapeRegistry {
        SlotShapeRegistry::default()
    }

    fn write_file(fs: &mut LpFsMemory, path: &str, contents: &str) {
        fs.write_file_mut(LpPath::new(path), contents.as_bytes())
            .unwrap();
    }

    fn fs_modify(path: &str) -> FsChange {
        FsChange {
            path: LpPathBuf::from(path),
            change_type: ChangeType::Modify,
        }
    }

    #[test]
    fn load_root_registers_inline_child() {
        let mut fs = LpFsMemory::new();
        write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "a.glsl" }
"#,
        );
        let mut store = ArtifactStore::new();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        registry
            .load_root(
                &mut store,
                &fs,
                LpPath::new("/playlist.toml"),
                Revision::new(1),
                &ctx,
            )
            .unwrap();
        assert_eq!(registry.entries.len(), 2);
    }

    #[test]
    fn load_root_rejects_non_empty_registry() {
        let mut fs = LpFsMemory::new();
        write_file(&mut fs, "/clock.toml", "kind = \"Clock\"\n");
        let mut store = ArtifactStore::new();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        registry
            .load_root(
                &mut store,
                &fs,
                LpPath::new("/clock.toml"),
                Revision::new(1),
                &ctx,
            )
            .unwrap();
        let err = registry
            .load_root(
                &mut store,
                &fs,
                LpPath::new("/clock.toml"),
                Revision::new(2),
                &ctx,
            )
            .unwrap_err();
        assert!(matches!(err, RegistryError::NotEmpty));
    }

    #[test]
    fn leaf_file_edit_marks_root_changed() {
        let mut fs = LpFsMemory::new();
        write_file(
            &mut fs,
            "/clock.toml",
            r#"
kind = "Clock"

[controls]
rate = 1.0
"#,
        );
        let mut store = ArtifactStore::new();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(
                &mut store,
                &fs,
                LpPath::new("/clock.toml"),
                Revision::new(1),
                &ctx,
            )
            .unwrap();

        write_file(
            &mut fs,
            "/clock.toml",
            r#"
kind = "Clock"

[controls]
rate = 2.0
"#,
        );
        store.apply_fs_changes(&[fs_modify("/clock.toml")], Revision::new(2));
        let updates = registry.sync(&mut store, &fs, Revision::new(2), &ctx);
        assert!(updates.added.is_empty());
        assert!(updates.removed.is_empty());
        assert_eq!(updates.changed, BTreeSet::from([root]));
    }

    #[test]
    fn inline_child_edit_isolated() {
        let mut fs = LpFsMemory::new();
        write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "a.glsl" }
"#,
        );
        let mut store = ArtifactStore::new();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(
                &mut store,
                &fs,
                LpPath::new("/playlist.toml"),
                Revision::new(1),
                &ctx,
            )
            .unwrap();
        let child = registry
            .entries
            .values()
            .find(|entry| !entry.source.path.is_root())
            .expect("inline child")
            .id;

        write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
source = { path = "b.glsl" }
"#,
        );
        store.apply_fs_changes(&[fs_modify("/playlist.toml")], Revision::new(2));
        let updates = registry.sync(&mut store, &fs, Revision::new(2), &ctx);
        assert!(!updates.changed.contains(&root));
        assert_eq!(updates.changed, BTreeSet::from([child]));
    }

    #[test]
    fn playlist_entry_add_marks_parent_and_child_added() {
        let mut fs = LpFsMemory::new();
        write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
"#,
        );
        let mut store = ArtifactStore::new();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(
                &mut store,
                &fs,
                LpPath::new("/playlist.toml"),
                Revision::new(1),
                &ctx,
            )
            .unwrap();

        write_file(
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
        store.apply_fs_changes(&[fs_modify("/playlist.toml")], Revision::new(2));
        let updates = registry.sync(&mut store, &fs, Revision::new(2), &ctx);
        assert_eq!(updates.added.len(), 1);
        assert!(updates.removed.is_empty());
        assert!(updates.changed.contains(&root));
    }

    #[test]
    fn path_child_file_edit_isolated() {
        let mut fs = LpFsMemory::new();
        write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2]
node = { def = { path = "./active.toml" } }
"#,
        );
        write_file(
            &mut fs,
            "/active.toml",
            r#"
kind = "Shader"
source = { path = "a.glsl" }
"#,
        );
        let mut store = ArtifactStore::new();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(
                &mut store,
                &fs,
                LpPath::new("/playlist.toml"),
                Revision::new(1),
                &ctx,
            )
            .unwrap();
        let child = registry
            .entries
            .values()
            .find(|entry| entry.source.path.is_root() && entry.id != root)
            .expect("child file root")
            .id;

        write_file(
            &mut fs,
            "/active.toml",
            r#"
kind = "Shader"
source = { path = "b.glsl" }
"#,
        );
        store.apply_fs_changes(&[fs_modify("/active.toml")], Revision::new(2));
        let updates = registry.sync(&mut store, &fs, Revision::new(2), &ctx);
        assert!(!updates.changed.contains(&root));
        assert_eq!(updates.changed, BTreeSet::from([child]));
    }

    #[test]
    fn inline_child_kind_change_marks_child_and_parent_changed() {
        let mut fs = LpFsMemory::new();
        write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Shader"
"#,
        );
        let mut store = ArtifactStore::new();
        let mut registry = NodeDefRegistry::new();
        let shapes = parse_ctx();
        let ctx = ParseCtx { shapes: &shapes };
        let root = registry
            .load_root(
                &mut store,
                &fs,
                LpPath::new("/playlist.toml"),
                Revision::new(1),
                &ctx,
            )
            .unwrap();
        let child = registry
            .entries
            .values()
            .find(|entry| !entry.source.path.is_root())
            .expect("inline child")
            .id;

        write_file(
            &mut fs,
            "/playlist.toml",
            r#"
kind = "Playlist"

[entries.2.node.def]
kind = "Clock"
"#,
        );
        store.apply_fs_changes(&[fs_modify("/playlist.toml")], Revision::new(2));
        let updates = registry.sync(&mut store, &fs, Revision::new(2), &ctx);
        assert!(updates.changed.contains(&root));
        assert!(updates.changed.contains(&child));
    }
}
