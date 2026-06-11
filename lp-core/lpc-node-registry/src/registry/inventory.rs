//! Registry definition and referenced asset inventory.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{NodeDef, NodeInvocation, Revision, SlotPath, resolve_artifact_specifier};
use lpfs::{LpFs, LpPath, LpPathBuf};

use crate::ArtifactLoc;

use super::changes::state_changed;
use super::{NodeDefEntry, NodeDefLoc, NodeDefRegistry, NodeDefState, NodeDefUpdates};
use super::{ParseCtx, RegistryError};

impl NodeDefRegistry {
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

        let old_locs: BTreeMap<NodeDefLoc, NodeDefState> = self
            .defs
            .iter()
            .filter(|(loc, _)| loc.artifact == location)
            .map(|(loc, entry)| (loc.clone(), entry.state.clone()))
            .collect();

        for loc in old_locs.keys() {
            if !new_inventory.contains_key(loc) {
                updates.push_removed(loc.clone());
                self.defs.remove(loc);
            }
        }

        let mut affected = Vec::new();
        for (loc, new_state) in &new_inventory {
            if let Some(old_state) = old_locs.get(loc) {
                if state_changed(old_state, new_state) {
                    updates.push_changed(loc.clone());
                    if let Some(entry) = self.defs.get_mut(loc) {
                        entry.state = new_state.clone();
                        entry.revision = current;
                    }
                    affected.push(loc.clone());
                }
            } else if self
                .register_def_at_location(loc.clone(), new_state.clone(), current)
                .is_ok()
            {
                updates.push_added(loc.clone());
                affected.push(loc.clone());
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

    #[expect(
        clippy::too_many_arguments,
        reason = "recursive inventory traversal carries context"
    )]
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
        for site in def.invocation_sites(&base_path) {
            match &site.invocation {
                NodeInvocation::Unset => {}
                NodeInvocation::Ref(_) => {
                    let Some(specifier) = site.invocation.ref_specifier() else {
                        continue;
                    };
                    let child_path =
                        resolve_artifact_specifier(file_path, &specifier).map_err(|err| {
                            RegistryError::SpecifierResolution {
                                message: String::from(err.to_string()),
                            }
                        })?;
                    let child_location = self.store.register_file(child_path.clone(), frame);
                    let child_inventory = self.derive_inventory(
                        child_location,
                        child_path.as_path(),
                        frame,
                        fs,
                        ctx,
                    )?;
                    for (loc, state) in child_inventory {
                        if inventory.insert(loc.clone(), state).is_some() {
                            return Err(RegistryError::DuplicateDefLocation);
                        }
                    }
                }
                NodeInvocation::Def(body) => {
                    let loc = NodeDefLoc {
                        artifact: location.clone(),
                        path: site.path.clone(),
                    };
                    if inventory
                        .insert(loc, NodeDefState::Loaded(body.value().clone()))
                        .is_some()
                    {
                        return Err(RegistryError::DuplicateDefLocation);
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
            Ok(bytes) => Ok(super::effective_projection::parse_toml_bytes(ctx, &bytes)),
            Err(err) => Ok(NodeDefState::ParseError(
                super::effective_projection::read_error_state(err),
            )),
        }
    }

    pub(crate) fn register_file_artifact(
        &mut self,
        path: LpPathBuf,
        frame: Revision,
    ) -> ArtifactLoc {
        self.store.register_file(path, frame)
    }

    fn referenced_locations(&self) -> Result<BTreeSet<ArtifactLoc>, RegistryError> {
        let Some(root) = self.root.as_ref() else {
            return Ok(self.store.locations().collect());
        };
        let mut referenced = BTreeSet::new();
        let mut visited_defs = BTreeSet::new();
        self.collect_referenced_locations(root, &mut referenced, &mut visited_defs)?;
        Ok(referenced)
    }

    fn collect_referenced_locations(
        &self,
        loc: &NodeDefLoc,
        referenced: &mut BTreeSet<ArtifactLoc>,
        visited_defs: &mut BTreeSet<NodeDefLoc>,
    ) -> Result<(), RegistryError> {
        if !visited_defs.insert(loc.clone()) {
            return Ok(());
        }
        referenced.insert(loc.artifact.clone());

        let Some(entry) = self.defs.get(loc) else {
            return Ok(());
        };
        let NodeDefState::Loaded(def) = &entry.state else {
            return Ok(());
        };
        let Some(containing) = loc.artifact.file_path() else {
            return Ok(());
        };

        for path in def
            .referenced_asset_paths(containing.as_path())
            .map_err(|err| RegistryError::SpecifierResolution {
                message: String::from(err.to_string()),
            })?
        {
            referenced.insert(ArtifactLoc::location_for_path(path.as_path()));
        }

        for site in def.invocation_sites(&loc.path) {
            match &site.invocation {
                NodeInvocation::Unset => {}
                NodeInvocation::Ref(_) => {
                    let Some(specifier) = site.invocation.ref_specifier() else {
                        continue;
                    };
                    let child_path = resolve_artifact_specifier(containing.as_path(), &specifier)
                        .map_err(|err| RegistryError::SpecifierResolution {
                        message: String::from(err.to_string()),
                    })?;
                    let child_loc = NodeDefLoc::artifact_root(ArtifactLoc::location_for_path(
                        child_path.as_path(),
                    ));
                    self.collect_referenced_locations(&child_loc, referenced, visited_defs)?;
                }
                NodeInvocation::Def(_) => {
                    let child_loc = NodeDefLoc {
                        artifact: loc.artifact.clone(),
                        path: site.path,
                    };
                    self.collect_referenced_locations(&child_loc, referenced, visited_defs)?;
                }
            }
        }

        Ok(())
    }

    pub(crate) fn reconcile_artifacts(
        &mut self,
        updates: &mut NodeDefUpdates,
    ) -> Result<(), RegistryError> {
        let referenced = self.referenced_locations()?;
        let to_unregister: Vec<ArtifactLoc> = self
            .store
            .locations()
            .filter(|location| !referenced.contains(location))
            .collect();

        for location in to_unregister {
            self.store.unregister(&location)?;
            let removed: Vec<NodeDefLoc> = self
                .defs
                .keys()
                .filter(|loc| loc.artifact == location)
                .cloned()
                .collect();
            for loc in removed {
                updates.push_removed(loc.clone());
                self.defs.remove(&loc);
                if self.root.as_ref() == Some(&loc) {
                    self.root = None;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn register_def_at_location(
        &mut self,
        loc: NodeDefLoc,
        state: NodeDefState,
        revision: Revision,
    ) -> Result<(), RegistryError> {
        if self.defs.contains_key(&loc) {
            return Err(RegistryError::DuplicateDefLocation);
        }
        self.defs.insert(
            loc.clone(),
            NodeDefEntry {
                loc,
                state,
                revision,
            },
        );
        Ok(())
    }

    pub(crate) fn register_all_asset_paths(
        &mut self,
        frame: Revision,
    ) -> Result<(), RegistryError> {
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

        for path in def
            .referenced_asset_paths(containing.as_path())
            .map_err(|err| RegistryError::SpecifierResolution {
                message: String::from(err.to_string()),
            })?
        {
            self.store.register_file(path, frame);
        }
        Ok(())
    }

    pub(crate) fn snapshot_def_states(&self) -> BTreeMap<NodeDefLoc, NodeDefState> {
        self.defs
            .iter()
            .map(|(loc, entry)| (loc.clone(), entry.state.clone()))
            .collect()
    }
}
