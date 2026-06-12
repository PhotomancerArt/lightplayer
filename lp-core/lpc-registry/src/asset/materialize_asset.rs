//! Materialize effective project assets by [`lpc_model::AssetSource`].

use alloc::format;
use alloc::string::{String, ToString};

use lpc_model::{
    ArtifactLocation, ArtifactOverlay, AssetEntry, AssetKind, AssetOverlay, AssetSource,
    NodeDefState, ProjectInventory, ProjectOverlay, WithRevision,
};
use lpfs::LpFs;

use crate::{ArtifactError, ArtifactReadFailure, ArtifactStore};

use super::{MaterializeAssetError, MaterializedAsset, MaterializedTextAsset};

pub fn materialize_asset(
    artifacts: &mut ArtifactStore,
    overlay: &WithRevision<ProjectOverlay>,
    inventory: &ProjectInventory,
    fs: &dyn LpFs,
    source: &AssetSource,
) -> Result<MaterializedAsset, MaterializeAssetError> {
    let entry =
        inventory
            .assets
            .get(source)
            .ok_or_else(|| MaterializeAssetError::UnreferencedAsset {
                source: source.clone(),
            })?;

    match source {
        AssetSource::Artifact { location } => {
            materialize_artifact_asset(artifacts, overlay, fs, source, location, entry)
        }
        AssetSource::Inline { owner, path } => {
            materialize_inline_asset(inventory, source, owner, path, entry)
        }
    }
}

pub fn materialize_asset_text(
    artifacts: &mut ArtifactStore,
    overlay: &WithRevision<ProjectOverlay>,
    inventory: &ProjectInventory,
    fs: &dyn LpFs,
    source: &AssetSource,
) -> Result<MaterializedTextAsset, MaterializeAssetError> {
    let materialized = materialize_asset(artifacts, overlay, inventory, fs, source)?;
    let text = String::from_utf8(materialized.bytes.clone()).map_err(|err| {
        MaterializeAssetError::Utf8 {
            source: source.clone(),
            message: err.to_string(),
        }
    })?;

    Ok(MaterializedTextAsset {
        source: materialized.source,
        kind: materialized.kind,
        revision: materialized.revision,
        text,
        diagnostic_name: materialized.diagnostic_name,
    })
}

fn materialize_artifact_asset(
    artifacts: &mut ArtifactStore,
    overlay: &WithRevision<ProjectOverlay>,
    fs: &dyn LpFs,
    source: &AssetSource,
    location: &ArtifactLocation,
    entry: &AssetEntry,
) -> Result<MaterializedAsset, MaterializeAssetError> {
    match overlay.get().artifact(location) {
        Some(ArtifactOverlay::Asset {
            overlay: AssetOverlay::ReplaceBody(bytes),
        }) => {
            return Ok(MaterializedAsset {
                source: source.clone(),
                kind: entry.kind,
                revision: overlay.changed_at(),
                bytes: bytes.clone(),
                diagnostic_name: artifact_diagnostic_name(location),
            });
        }
        Some(ArtifactOverlay::Asset {
            overlay: AssetOverlay::Delete,
        }) => {
            return Err(MaterializeAssetError::Deleted {
                source: source.clone(),
            });
        }
        Some(ArtifactOverlay::Slot { .. }) => {
            return Err(MaterializeAssetError::Unsupported {
                source: source.clone(),
                message: String::from("slot overlay cannot materialize as an asset body"),
            });
        }
        None => {}
    }

    match artifacts.read_bytes(location, fs) {
        Ok(bytes) => Ok(MaterializedAsset {
            source: source.clone(),
            kind: entry.kind,
            revision: artifacts.revision(location).unwrap_or(entry.revision),
            bytes,
            diagnostic_name: artifact_diagnostic_name(location),
        }),
        Err(err) => Err(error_from_artifact(source, err)),
    }
}

fn materialize_inline_asset(
    inventory: &ProjectInventory,
    source: &AssetSource,
    owner: &lpc_model::NodeDefLocation,
    path: &lpc_model::SlotPath,
    entry: &AssetEntry,
) -> Result<MaterializedAsset, MaterializeAssetError> {
    if !matches!(
        entry.kind,
        AssetKind::ShaderSource | AssetKind::ComputeShaderSource
    ) {
        return Err(MaterializeAssetError::Unsupported {
            source: source.clone(),
            message: String::from("inline binary assets are not supported yet"),
        });
    }

    let Some(owner_entry) = inventory.defs.get(owner) else {
        return Err(MaterializeAssetError::OwnerDefUnavailable {
            source: source.clone(),
            owner: owner.clone(),
        });
    };
    let NodeDefState::Loaded(def) = &owner_entry.state else {
        return Err(MaterializeAssetError::OwnerDefUnavailable {
            source: source.clone(),
            owner: owner.clone(),
        });
    };

    let Some(text) = def.inline_asset_text(&owner.path, path) else {
        return Err(MaterializeAssetError::Unsupported {
            source: source.clone(),
            message: String::from("inline asset source is not supported by this node definition"),
        });
    };

    Ok(MaterializedAsset {
        source: source.clone(),
        kind: entry.kind,
        revision: entry.revision,
        bytes: text.text.as_bytes().to_vec(),
        diagnostic_name: format!(
            "{}:{}.{}",
            owner.artifact.file_path().as_str(),
            path,
            text.extension
        ),
    })
}

fn error_from_artifact(source: &AssetSource, err: ArtifactError) -> MaterializeAssetError {
    match err {
        ArtifactError::Read(ArtifactReadFailure::NotFound) => MaterializeAssetError::NotFound {
            source: source.clone(),
        },
        ArtifactError::Read(ArtifactReadFailure::Deleted) => MaterializeAssetError::Deleted {
            source: source.clone(),
        },
        ArtifactError::Read(ArtifactReadFailure::Io { message })
        | ArtifactError::Read(ArtifactReadFailure::InvalidPath { message })
        | ArtifactError::Resolution(message)
        | ArtifactError::Internal(message) => MaterializeAssetError::ReadError {
            source: source.clone(),
            message,
        },
        ArtifactError::UnknownArtifact { location } => MaterializeAssetError::ReadError {
            source: source.clone(),
            message: format!("unknown artifact {}", location.to_uri()),
        },
    }
}

fn artifact_diagnostic_name(location: &ArtifactLocation) -> String {
    location.file_path().as_str().to_string()
}
