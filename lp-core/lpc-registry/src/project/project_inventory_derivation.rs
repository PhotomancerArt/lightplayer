//! Effective inventory derivation by walking loaded node definitions.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::{String, ToString};

use lpc_model::{
    ArtifactBodyEdit, ArtifactLocation, AssetBodySource, AssetEntry, AssetState, NodeDefLocation,
    NodeDefState, NodeInvocation, ProjectInventory, ProjectOverlay, Revision, SlotPath,
    WithRevision, resolve_artifact_specifier,
};
use lpfs::{LpFs, LpPath};

use crate::{
    ArtifactError, ArtifactReadFailure, ArtifactStore, ParseCtx,
    edit::{EditApplyError, parse_def_bytes, project_artifact_bytes},
};

pub(crate) fn derive_effective_inventory(
    artifacts: &mut ArtifactStore,
    root: Option<&NodeDefLocation>,
    overlay: &WithRevision<ProjectOverlay>,
    fs: &dyn LpFs,
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> ProjectInventory {
    let mut derivation = InventoryDerivation {
        artifacts,
        overlay,
        fs,
        frame,
        ctx,
        inventory: ProjectInventory::new(),
        visited_defs: BTreeSet::new(),
    };

    if let Some(root) = root {
        derivation.walk_def_location(root.clone());
    }

    derivation.inventory
}

struct InventoryDerivation<'a, 'ctx> {
    artifacts: &'a mut ArtifactStore,
    overlay: &'a WithRevision<ProjectOverlay>,
    fs: &'a dyn LpFs,
    frame: Revision,
    ctx: &'a ParseCtx<'ctx>,
    inventory: ProjectInventory,
    visited_defs: BTreeSet<NodeDefLocation>,
}

impl InventoryDerivation<'_, '_> {
    fn walk_def_location(&mut self, location: NodeDefLocation) {
        if !self.visited_defs.insert(location.clone()) {
            return;
        }

        let revision = self.revision_for_artifact(&location.artifact);
        let state = self.read_effective_def(&location.artifact);
        self.inventory.defs.insert(
            location.clone(),
            lpc_model::NodeDefEntry::new(location.clone(), state.clone(), revision),
        );

        let NodeDefState::Loaded(def) = state else {
            return;
        };

        self.walk_loaded_def(&location.artifact, &location.path, &def, revision);
    }

    fn walk_loaded_def(
        &mut self,
        artifact: &ArtifactLocation,
        base_path: &SlotPath,
        def: &lpc_model::NodeDef,
        revision: Revision,
    ) {
        match def.referenced_asset_paths(artifact.file_path().as_path()) {
            Ok(paths) => {
                for path in paths {
                    self.walk_asset(ArtifactLocation::file(path));
                }
            }
            Err(err) => {
                let location = ArtifactLocation::file(error_asset_path(artifact, base_path));
                let state = AssetState::ReadError {
                    message: err.to_string(),
                };
                self.inventory.assets.insert(
                    location.clone(),
                    AssetEntry::new(location, state, self.overlay.changed_at()),
                );
            }
        }

        for site in def.invocation_sites(base_path) {
            match &site.invocation {
                NodeInvocation::Unset => {}
                NodeInvocation::Def(body) => {
                    let child_location = NodeDefLocation {
                        artifact: artifact.clone(),
                        path: site.path,
                    };
                    let child_def = body.value().clone();
                    self.inventory.defs.insert(
                        child_location.clone(),
                        lpc_model::NodeDefEntry::new(
                            child_location.clone(),
                            NodeDefState::Loaded(child_def.clone()),
                            revision,
                        ),
                    );
                    self.walk_loaded_def(
                        &child_location.artifact,
                        &child_location.path,
                        &child_def,
                        revision,
                    );
                }
                NodeInvocation::Ref(_) => {
                    self.walk_ref_invocation(artifact.file_path().as_path(), &site.invocation);
                }
            }
        }
    }

    fn walk_ref_invocation(&mut self, containing_file: &LpPath, invocation: &NodeInvocation) {
        let Some(specifier) = invocation.ref_specifier() else {
            return;
        };
        let child_location = match resolve_artifact_specifier(containing_file, &specifier) {
            Ok(path) => {
                let artifact = self.artifacts.register_file(path, self.frame);
                NodeDefLocation::artifact_root(artifact)
            }
            Err(_) => {
                let artifact = ArtifactLocation::file(error_ref_path(containing_file, &specifier));
                NodeDefLocation::artifact_root(artifact)
            }
        };

        self.walk_def_location(child_location);
    }

    fn walk_asset(&mut self, location: ArtifactLocation) {
        self.artifacts
            .register_location(location.clone(), self.frame);
        let revision = self.revision_for_artifact(&location);
        let state = self.read_effective_asset(&location);
        self.inventory
            .assets
            .insert(location.clone(), AssetEntry::new(location, state, revision));
    }

    fn read_effective_def(&mut self, location: &ArtifactLocation) -> NodeDefState {
        let pending = self.overlay.get().artifact(location);

        if let Some(body) = pending.and_then(|overlay| overlay.as_body()) {
            return match body {
                ArtifactBodyEdit::Delete => NodeDefState::Deleted,
                ArtifactBodyEdit::ReplaceBody(bytes) => match parse_def_bytes(bytes, self.ctx) {
                    Ok(def) => NodeDefState::Loaded(def),
                    Err(err) => NodeDefState::ParseError(parse_error(err)),
                },
            };
        }

        let committed = match self.artifacts.read_bytes(location, self.fs) {
            Ok(bytes) => Some(bytes),
            Err(_) if pending.and_then(|overlay| overlay.as_slot()).is_some() => None,
            Err(err) => return node_def_state_for_read_error(err),
        };

        match project_artifact_bytes(
            committed.as_deref(),
            pending,
            self.ctx,
            self.overlay.changed_at(),
        ) {
            Ok(Some(bytes)) => match parse_def_bytes(&bytes, self.ctx) {
                Ok(def) => NodeDefState::Loaded(def),
                Err(err) => NodeDefState::ParseError(parse_error(err)),
            },
            Ok(None) => NodeDefState::Deleted,
            Err(err) => NodeDefState::ParseError(parse_error(err)),
        }
    }

    fn read_effective_asset(&mut self, location: &ArtifactLocation) -> AssetState {
        match self
            .overlay
            .get()
            .artifact(location)
            .and_then(|overlay| overlay.as_body())
        {
            Some(ArtifactBodyEdit::Delete) => return AssetState::Deleted,
            Some(ArtifactBodyEdit::ReplaceBody(_)) => {
                return AssetState::Available {
                    source: AssetBodySource::OverlayReplace,
                };
            }
            None => {}
        }

        if self
            .overlay
            .get()
            .artifact(location)
            .and_then(|overlay| overlay.as_slot())
            .is_some()
        {
            return AssetState::ReadError {
                message: String::from("slot overlay cannot apply to an asset artifact"),
            };
        }

        match self.artifacts.read_bytes(location, self.fs) {
            Ok(_) => AssetState::Available {
                source: AssetBodySource::Committed,
            },
            Err(ArtifactError::Read(ArtifactReadFailure::NotFound)) => AssetState::NotFound,
            Err(ArtifactError::Read(ArtifactReadFailure::Deleted)) => AssetState::Deleted,
            Err(err) => AssetState::ReadError {
                message: artifact_error_message(&err),
            },
        }
    }

    fn revision_for_artifact(&self, location: &ArtifactLocation) -> Revision {
        if self.overlay.get().contains_artifact(location) {
            self.overlay.changed_at()
        } else {
            self.artifacts.revision(location).unwrap_or(self.frame)
        }
    }
}

fn node_def_state_for_read_error(err: ArtifactError) -> NodeDefState {
    match err {
        ArtifactError::Read(ArtifactReadFailure::NotFound) => NodeDefState::NotFound,
        ArtifactError::Read(ArtifactReadFailure::Deleted) => NodeDefState::Deleted,
        other => NodeDefState::ReadError {
            message: artifact_error_message(&other),
        },
    }
}

fn parse_error(err: EditApplyError) -> lpc_model::NodeDefParseError {
    lpc_model::NodeDefParseError::Toml {
        error: err.to_string(),
    }
}

fn artifact_error_message(err: &ArtifactError) -> String {
    match err {
        ArtifactError::UnknownArtifact { location } => {
            format!("unknown artifact {}", location.to_uri())
        }
        ArtifactError::Resolution(message) | ArtifactError::Internal(message) => message.clone(),
        ArtifactError::Read(ArtifactReadFailure::Deleted) => String::from("artifact was deleted"),
        ArtifactError::Read(ArtifactReadFailure::NotFound) => String::from("artifact not found"),
        ArtifactError::Read(ArtifactReadFailure::Io { message })
        | ArtifactError::Read(ArtifactReadFailure::InvalidPath { message }) => message.clone(),
    }
}

fn error_ref_path(containing_file: &LpPath, specifier: &lpc_model::ArtifactSpec) -> String {
    format!("{}#unresolved-ref:{specifier}", containing_file.as_str())
}

fn error_asset_path(artifact: &ArtifactLocation, base_path: &SlotPath) -> String {
    format!(
        "{}#asset-resolution-error:{}",
        artifact.file_path().as_str(),
        base_path
    )
}
