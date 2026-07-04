//! Materialize effective project assets by [`lpc_model::AssetLocation`].

use alloc::format;
use alloc::string::{String, ToString};

use lpc_model::{
    ArtifactLocation, ArtifactOverlay, AssetBodyOverlay, AssetEntry, AssetLocation,
    ProjectInventory, ProjectOverlay, WithRevision,
};
use lpfs::LpFs;

use crate::{ArtifactError, ArtifactReadFailure, ArtifactStore};

use super::{AssetBytes, AssetReadError, AssetText};

pub fn materialize_asset(
    artifacts: &mut ArtifactStore,
    overlay: &WithRevision<ProjectOverlay>,
    inventory: &ProjectInventory,
    fs: &dyn LpFs,
    source: &AssetLocation,
) -> Result<AssetBytes, AssetReadError> {
    let entry = inventory
        .assets
        .get(source)
        .ok_or_else(|| AssetReadError::UnreferencedAsset {
            location: source.clone(),
        })?;

    match source {
        AssetLocation::Artifact { location } => {
            materialize_artifact_asset(artifacts, overlay, fs, source, location, entry)
        }
    }
}

pub fn materialize_asset_text(
    artifacts: &mut ArtifactStore,
    overlay: &WithRevision<ProjectOverlay>,
    inventory: &ProjectInventory,
    fs: &dyn LpFs,
    source: &AssetLocation,
) -> Result<AssetText, AssetReadError> {
    let materialized = materialize_asset(artifacts, overlay, inventory, fs, source)?;
    let text =
        String::from_utf8(materialized.bytes.clone()).map_err(|err| AssetReadError::Utf8 {
            location: source.clone(),
            message: err.to_string(),
        })?;

    Ok(AssetText {
        location: materialized.location,
        content_type: materialized.content_type,
        revision: materialized.revision,
        text,
        diagnostic_name: materialized.diagnostic_name,
    })
}

fn materialize_artifact_asset(
    artifacts: &mut ArtifactStore,
    overlay: &WithRevision<ProjectOverlay>,
    fs: &dyn LpFs,
    source: &AssetLocation,
    location: &ArtifactLocation,
    entry: &AssetEntry,
) -> Result<AssetBytes, AssetReadError> {
    match overlay.get().artifact(location) {
        Some(ArtifactOverlay::Asset {
            overlay: AssetBodyOverlay::ReplaceBody(bytes),
        }) => {
            return Ok(AssetBytes {
                location: source.clone(),
                content_type: entry.content_type,
                revision: overlay.changed_at(),
                bytes: bytes.clone(),
                diagnostic_name: artifact_diagnostic_name(location),
            });
        }
        Some(ArtifactOverlay::Asset {
            overlay: AssetBodyOverlay::Delete,
        }) => {
            return Err(AssetReadError::Deleted {
                location: source.clone(),
            });
        }
        Some(ArtifactOverlay::Slot { .. }) => {
            return Err(AssetReadError::Unsupported {
                location: source.clone(),
                message: String::from("slot overlay cannot materialize as an asset body"),
            });
        }
        None => {}
    }

    match artifacts.read_bytes(location, fs) {
        Ok(bytes) => Ok(AssetBytes {
            location: source.clone(),
            content_type: entry.content_type,
            revision: artifacts.revision(location).unwrap_or(entry.revision),
            bytes,
            diagnostic_name: artifact_diagnostic_name(location),
        }),
        Err(err) => Err(error_from_artifact(source, err)),
    }
}

fn error_from_artifact(source: &AssetLocation, err: ArtifactError) -> AssetReadError {
    match err {
        ArtifactError::Read(ArtifactReadFailure::NotFound) => AssetReadError::NotFound {
            location: source.clone(),
        },
        ArtifactError::Read(ArtifactReadFailure::Deleted) => AssetReadError::Deleted {
            location: source.clone(),
        },
        ArtifactError::Read(ArtifactReadFailure::Io { message })
        | ArtifactError::Read(ArtifactReadFailure::InvalidPath { message })
        | ArtifactError::Resolution(message)
        | ArtifactError::Internal(message) => AssetReadError::ReadError {
            location: source.clone(),
            message,
        },
        ArtifactError::UnknownArtifact { location } => AssetReadError::ReadError {
            location: source.clone(),
            message: format!("unknown artifact {}", location.to_uri()),
        },
    }
}

fn artifact_diagnostic_name(location: &ArtifactLocation) -> String {
    location.file_path().as_str().to_string()
}
