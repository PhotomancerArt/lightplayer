//! What the firmware store gives an update: a released build's facts (the
//! store's `latest`, for "Other version…", DS7), a whole released build to
//! install, and an engine for a heal or a backup — every file verified
//! against its `ota-manifest.json` by `lpa-firmware-store` before it is
//! used.
//!
//! The build comes as raw `core.bin` / `engine.bin` plus encoding 1's `.z`
//! files when the release lists them; a `.z` that cannot be fetched or does
//! not describe its piece is dropped and the piece is served raw (encoding
//! 1 is an optimisation, never a requirement).

use std::rc::Rc;

use lpa_firmware_store::{FetchError, StoreError, fetch_engine_from_store};
use lpa_update::decide::StoreAnswer;
use lpa_update::{EncodedPiece, HostBuild, HostBuildFacts, HostIdentity, HostPieceFacts};
use lpc_firmware_release::{
    EncodedPieceFile, OtaManifest, ReleaseSelector, ReleaseVersion, TargetName,
};

use super::device_firmware_sources::StudioFirmwareStore;

/// Why a build could not be had from the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoreMiss {
    /// The store could not be reached (or none is installed).
    pub offline: bool,
    pub why: String,
}

/// The facts of a released build, by its `ota-manifest.json`; `None` when a
/// hash is not hex.
pub fn facts_from_ota_manifest(manifest: &OtaManifest) -> Option<HostBuildFacts> {
    let piece = |sha: &str, len: u64| -> Option<HostPieceFacts> {
        Some(HostPieceFacts {
            sha256: lpc_update::sha256_from_hex(sha)?,
            len: u32::try_from(len).ok()?,
        })
    };
    Some(HostBuildFacts::from_parts(
        identity_of(manifest),
        piece(&manifest.core.sha256, manifest.core.length)?,
        piece(&manifest.engine.sha256, manifest.engine.length)?,
    ))
}

/// The store's latest release for `target`, by its facts.
pub(crate) async fn store_latest(
    store: Rc<StudioFirmwareStore>,
    target: String,
) -> Result<Option<HostBuildFacts>, StoreMiss> {
    let target = TargetName::parse(&target).ok_or_else(|| StoreMiss {
        offline: false,
        why: format!("{target} is not a target name"),
    })?;
    let manifest = store
        .manifest(&target, &ReleaseSelector::Latest)
        .await
        .map_err(miss)?;
    Ok(manifest.as_ref().and_then(facts_from_ota_manifest))
}

/// Release `version` for `target`, whole, from the store.
pub(crate) async fn store_build(
    store: Option<Rc<StudioFirmwareStore>>,
    target: String,
    version: String,
) -> Result<HostBuild, StoreMiss> {
    let Some(store) = store else {
        return Err(StoreMiss {
            offline: true,
            why: "no firmware store".to_string(),
        });
    };
    let (Some(target_name), Some(release)) =
        (TargetName::parse(&target), ReleaseVersion::parse(&version))
    else {
        return Err(StoreMiss {
            offline: false,
            why: format!("{version} is not a release of {target}"),
        });
    };
    let manifest = store
        .manifest(&target_name, &ReleaseSelector::Version(release))
        .await
        .map_err(miss)?
        .ok_or_else(|| StoreMiss {
            offline: false,
            why: format!("the store has no {version} for {target}"),
        })?;
    let core = store
        .file(&manifest, &manifest.core.file)
        .await
        .map_err(miss)?;
    let engine = store
        .file(&manifest, &manifest.engine.file)
        .await
        .map_err(miss)?;
    let (core_z, engine_z) = match manifest.encoding1() {
        Some(e) => (
            encoded(&store, &manifest, &e.core).await,
            encoded(&store, &manifest, &e.engine).await,
        ),
        None => (None, None),
    };
    build_from_parts(&manifest, core, engine, core_z, engine_z)
}

/// The engine `sha` of `build_id` for a heal or a backup, as the engine
/// source asks for it.
pub(crate) async fn store_engine(
    store: Option<Rc<StudioFirmwareStore>>,
    target: String,
    build_id: String,
    sha: [u8; 32],
) -> StoreAnswer {
    let Some(store) = store else {
        return StoreAnswer::Offline;
    };
    let sha = lpc_update::sha256_to_hex(&sha);
    match fetch_engine_from_store(&store, &target, &build_id, &sha).await {
        Ok(Some((_, bytes))) => StoreAnswer::Found(bytes),
        Ok(None) => StoreAnswer::NotFound,
        Err(StoreError::Fetch(FetchError::Offline(_))) => StoreAnswer::Offline,
        Err(error) => {
            log::warn!("firmware store: engine for {build_id} refused: {error}");
            StoreAnswer::NotFound
        }
    }
}

/// A build from a manifest and its verified pieces, the `.z` streams kept
/// only when they describe their pieces.
fn build_from_parts(
    manifest: &OtaManifest,
    core: Vec<u8>,
    engine: Vec<u8>,
    core_z: Option<EncodedPiece>,
    engine_z: Option<EncodedPiece>,
) -> Result<HostBuild, StoreMiss> {
    let identity = identity_of(manifest);
    let core_z = core_z.filter(|z| z.check(core.len()).is_ok());
    let engine_z = engine_z.filter(|z| z.check(engine.len()).is_ok());
    HostBuild::from_parts(identity, core, engine, core_z, engine_z).map_err(|error| StoreMiss {
        offline: false,
        why: format!("the store's build is not one this Studio can serve: {error:?}"),
    })
}

async fn encoded(
    store: &StudioFirmwareStore,
    manifest: &OtaManifest,
    file: &EncodedPieceFile,
) -> Option<EncodedPiece> {
    match store.file(manifest, &file.file).await {
        Ok(stream) => Some(EncodedPiece {
            stream,
            chunks: file.chunks.clone(),
        }),
        Err(error) => {
            log::debug!("firmware store: {} not used: {error}", file.file);
            None
        }
    }
}

fn identity_of(manifest: &OtaManifest) -> HostIdentity {
    HostIdentity {
        target: manifest.target.clone(),
        chip: manifest.chip.clone(),
        version: manifest.version.clone(),
        build_id: manifest.build_id(),
        wire_proto: manifest.wire_proto,
        layout: manifest.requires.layout,
        min_loader: manifest.requires.loader,
    }
}

fn miss(error: StoreError) -> StoreMiss {
    StoreMiss {
        offline: matches!(error, StoreError::Fetch(FetchError::Offline(_))),
        why: error.to_string(),
    }
}
