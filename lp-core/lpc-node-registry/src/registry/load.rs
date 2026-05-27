//! Initial registry loading and reachable child registration.

use alloc::string::{String, ToString};

use lpc_model::{NodeDef, NodeInvocation, Revision, SlotPath, resolve_artifact_specifier};
use lpfs::{LpFs, LpPath};

use super::{NodeDefLoc, NodeDefRegistry, NodeDefState, ParseCtx, RegistryError};

impl NodeDefRegistry {
    /// Load all defs reachable from a root node-definition TOML file.
    ///
    /// The root kind is not enforced; `project.toml` is convention only.
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

    pub(crate) fn register_artifact_subtree(
        &mut self,
        location: crate::ArtifactLoc,
        file_path: &LpPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<NodeDefLoc, RegistryError> {
        let revision = self.store.revision(&location).unwrap_or(frame);
        let state = self.read_artifact_state(&location, fs, ctx)?;
        let loc = NodeDefLoc::artifact_root(location.clone());
        self.register_def_at_location(loc.clone(), state.clone(), revision)?;
        if let NodeDefState::Loaded(def) = state {
            self.register_invocations(&location, file_path, def, SlotPath::root(), frame, fs, ctx)?;
        }
        Ok(loc)
    }

    pub(crate) fn register_invocations(
        &mut self,
        location: &crate::ArtifactLoc,
        file_path: &LpPath,
        def: NodeDef,
        base_path: SlotPath,
        frame: Revision,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
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
                    let child_loc = NodeDefLoc::artifact_root(child_location.clone());
                    if !self.defs.contains_key(&child_loc) {
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
                    let loc = NodeDefLoc {
                        artifact: location.clone(),
                        path: site.path.clone(),
                    };
                    let revision = self.store.revision(&location).unwrap_or(frame);
                    self.register_def_at_location(
                        loc,
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
}
