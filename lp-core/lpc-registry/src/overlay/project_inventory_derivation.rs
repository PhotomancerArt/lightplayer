//! Effective inventory derivation by walking loaded node definitions.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{
    ArtifactLocation, AssetBodySource, AssetEntry, AssetKind, AssetOverlay, AssetSource,
    AssetState, NodeDefEntry, NodeDefLocation, NodeDefState, NodeInvocation, ProjectInventory,
    ProjectNodeEntry, ProjectNodeKey, ProjectNodeOrigin, ProjectOverlay, ReferencedAsset, Revision,
    SlotPath, WithRevision, resolve_artifact_specifier,
};
use lpfs::{LpFs, LpPath};

use crate::{
    ArtifactError, ArtifactReadFailure, ArtifactStore, ParseCtx,
    overlay::{EditApplyError, apply_slot_overlay_to_def, parse_def_bytes},
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
    };

    if let Some(root) = root {
        let root_key = ProjectNodeKey::root();
        derivation.inventory.graph.root = root_key.clone();
        derivation.walk_graph_node(
            root_key.clone(),
            None,
            root.clone(),
            ProjectNodeOrigin::Root,
            &mut Vec::new(),
        );
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
}

impl InventoryDerivation<'_, '_> {
    fn walk_graph_node(
        &mut self,
        key: ProjectNodeKey,
        parent: Option<ProjectNodeKey>,
        location: NodeDefLocation,
        origin: ProjectNodeOrigin,
        ancestry: &mut Vec<NodeDefLocation>,
    ) {
        let (state, revision) = self.ensure_def_entry(location.clone());
        self.inventory.graph.insert_node(ProjectNodeEntry {
            key: key.clone(),
            parent,
            def_location: location.clone(),
            origin,
        });

        let NodeDefState::Loaded(def) = state else {
            return;
        };

        if ancestry.contains(&location) {
            return;
        }

        ancestry.push(location.clone());
        self.walk_loaded_def(&key, &location, &def, revision, ancestry);
        ancestry.pop();
    }

    fn ensure_def_entry(&mut self, location: NodeDefLocation) -> (NodeDefState, Revision) {
        if let Some(entry) = self.inventory.defs.get(&location) {
            return (entry.state.clone(), entry.revision);
        }

        let revision = self.revision_for_artifact(&location.artifact);
        let state = self.read_effective_def(&location.artifact);
        self.inventory.defs.insert(
            location.clone(),
            NodeDefEntry::new(location, state.clone(), revision),
        );
        (state, revision)
    }

    fn walk_loaded_def(
        &mut self,
        key: &ProjectNodeKey,
        location: &NodeDefLocation,
        def: &lpc_model::NodeDef,
        revision: Revision,
        ancestry: &mut Vec<NodeDefLocation>,
    ) {
        match def.referenced_assets(
            location.artifact.file_path().as_path(),
            location,
            &location.path,
        ) {
            Ok(assets) => {
                for asset in assets {
                    self.walk_asset(asset, revision, key);
                }
            }
            Err(err) => {
                let source = AssetSource::artifact(ArtifactLocation::file(error_asset_path(
                    &location.artifact,
                    &location.path,
                )));
                let state = AssetState::ReadError {
                    message: err.to_string(),
                };
                self.inventory.assets.insert(
                    source.clone(),
                    AssetEntry::new(source, AssetKind::Binary, state, self.overlay.changed_at()),
                );
            }
        }

        for site in def.invocation_sites(&location.path) {
            match &site.invocation {
                NodeInvocation::Unset => {}
                NodeInvocation::Def(body) => {
                    let child_location = NodeDefLocation {
                        artifact: location.artifact.clone(),
                        path: site.path.clone(),
                    };
                    let child_def = body.value().clone();
                    self.inventory.defs.insert(
                        child_location.clone(),
                        NodeDefEntry::new(
                            child_location.clone(),
                            NodeDefState::Loaded(child_def.clone()),
                            revision,
                        ),
                    );
                    self.walk_graph_node(
                        key.child(site.path.clone()),
                        Some(key.clone()),
                        child_location,
                        ProjectNodeOrigin::Invocation {
                            slot: site.path,
                            role: site.role,
                            invocation: site.invocation,
                        },
                        ancestry,
                    );
                }
                NodeInvocation::Ref(_) => {
                    let child_location = self.resolve_ref_invocation(
                        location.artifact.file_path().as_path(),
                        &site.invocation,
                    );
                    self.walk_graph_node(
                        key.child(site.path.clone()),
                        Some(key.clone()),
                        child_location,
                        ProjectNodeOrigin::Invocation {
                            slot: site.path,
                            role: site.role,
                            invocation: site.invocation,
                        },
                        ancestry,
                    );
                }
            }
        }
    }

    fn resolve_ref_invocation(
        &mut self,
        containing_file: &LpPath,
        invocation: &NodeInvocation,
    ) -> NodeDefLocation {
        let Some(specifier) = invocation.ref_specifier() else {
            let artifact = ArtifactLocation::file(format!(
                "{}#unresolved-ref:<empty>",
                containing_file.as_str()
            ));
            return NodeDefLocation::artifact_root(artifact);
        };
        match resolve_artifact_specifier(containing_file, &specifier) {
            Ok(path) => {
                let artifact = self.artifacts.register_file(path, self.frame);
                NodeDefLocation::artifact_root(artifact)
            }
            Err(_) => {
                let artifact = ArtifactLocation::file(error_ref_path(containing_file, &specifier));
                NodeDefLocation::artifact_root(artifact)
            }
        }
    }

    fn walk_asset(
        &mut self,
        asset: ReferencedAsset,
        owner_revision: Revision,
        consumer: &ProjectNodeKey,
    ) {
        let revision = self.revision_for_asset(&asset.source, owner_revision);
        let state = self.read_effective_asset(&asset.source);
        self.inventory
            .graph
            .add_asset_consumer(asset.source.clone(), consumer.clone());
        self.inventory.assets.insert(
            asset.source.clone(),
            AssetEntry::new(asset.source, asset.kind, state, revision),
        );
    }

    fn read_effective_def(&mut self, location: &ArtifactLocation) -> NodeDefState {
        let pending = self.overlay.get().artifact(location);

        if let Some(body) = pending.and_then(|overlay| overlay.as_body()) {
            return match body {
                AssetOverlay::Delete => NodeDefState::Deleted,
                AssetOverlay::ReplaceBody(bytes) => match parse_def_bytes(bytes, self.ctx) {
                    Ok(def) => NodeDefState::Loaded(def),
                    Err(err) => NodeDefState::ParseError(parse_error(err)),
                },
            };
        }

        let mut def = match self.artifacts.read_bytes(location, self.fs) {
            Ok(bytes) => match parse_def_bytes(&bytes, self.ctx) {
                Ok(def) => def,
                Err(err) => return NodeDefState::ParseError(parse_error(err)),
            },
            Err(_) if pending.and_then(|overlay| overlay.as_slot()).is_some() => {
                lpc_model::NodeDef::default()
            }
            Err(err) => return node_def_state_for_read_error(err),
        };

        if let Some(slot_overlay) = pending.and_then(|overlay| overlay.as_slot()) {
            if let Err(err) = apply_slot_overlay_to_def(
                &mut def,
                slot_overlay,
                self.ctx,
                self.overlay.changed_at(),
            ) {
                return NodeDefState::ParseError(parse_error(err));
            }
        }

        NodeDefState::Loaded(def)
    }

    fn read_effective_asset(&mut self, source: &AssetSource) -> AssetState {
        let location = match source {
            AssetSource::Artifact { location } => location,
            AssetSource::Inline { .. } => {
                return AssetState::Available {
                    source: AssetBodySource::Inline,
                };
            }
            AssetSource::Url { .. } => {
                return AssetState::ReadError {
                    message: String::from("URL assets are not supported yet"),
                };
            }
        };

        self.artifacts
            .register_location(location.clone(), self.frame);

        match self
            .overlay
            .get()
            .artifact(location)
            .and_then(|overlay| overlay.as_body())
        {
            Some(AssetOverlay::Delete) => return AssetState::Deleted,
            Some(AssetOverlay::ReplaceBody(_)) => {
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

    fn revision_for_asset(&self, source: &AssetSource, owner_revision: Revision) -> Revision {
        match source {
            AssetSource::Artifact { location } => self.revision_for_artifact(location),
            AssetSource::Inline { .. } | AssetSource::Url { .. } => owner_revision,
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
