//! A custom build installed from files on this computer ("From a file…" in
//! "Other version…"): the files a build's over-the-air update needs — its
//! `ota-manifest.json` and the `core.bin` / `engine.bin` it names, plus
//! encoding 1's `core.z` / `engine.z` when picked — exactly what
//! `lp-cli firmware package` (and `scripts/ota/build-image.sh`) write to a
//! build's `ota/` directory.
//!
//! Checked here before anything is offered: the manifest must be one this
//! Studio reads (`lpc-firmware-release`'s own checks, format 1), built for
//! the board's target, and every piece must be the length and SHA-256 the
//! manifest says. A `.z` that does not describe its piece is dropped (the
//! piece goes raw, as from the store). Nothing is kept on disk: the build
//! lives in this Studio's memory until another is picked or the page goes.
//!
//! The picked build becomes one more version choice, marked as from a file,
//! and its install is always Lasting: a file has no order against the
//! board's version and no store vouches for it.
//!
//! The file picker itself is the web's (a browser opens one only from a real
//! click), so — as with a backup restored from a file
//! ([`super::device_backup_import`]) — the published offer
//! ([`FirmwareFileOp`], `devices/<board>/install-firmware-file`) carries no
//! bytes and is the user's to press ([`ActionMeta::needs_user_activation`]),
//! and the files ride an unpublished op ([`FirmwareFileDataOp`]) only a real
//! file read produces.

use core::any::Any;

use lpa_devices::DeviceId;
use lpa_update::{EncodedPiece, HostBuild, HostBuildFacts};
use lpc_firmware_release::OtaManifest;

use super::update_store_builds::host_build_from_parts;
use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// The manifest's file name in a build's `ota/` directory.
pub const FIRMWARE_FILE_MANIFEST: &str = "ota-manifest.json";

/// One file the person picked: its name (without any folder) and bytes.
#[derive(Clone, Eq, PartialEq)]
pub struct PickedFirmwareFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl core::fmt::Debug for PickedFirmwareFile {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "PickedFirmwareFile({}, {} bytes)",
            self.name,
            self.bytes.len()
        )
    }
}

/// A build read from picked files, checked against its manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirmwareFileBuild {
    pub build: HostBuild,
    pub facts: HostBuildFacts,
}

/// Read `files` as a build for a board of `target`, or say in plain words
/// why not. See the module docs.
pub fn read_firmware_files(
    files: &[PickedFirmwareFile],
    target: &str,
) -> Result<FirmwareFileBuild, String> {
    let named = |name: &str| files.iter().find(|file| base_name(&file.name) == name);
    let manifest = named(FIRMWARE_FILE_MANIFEST).ok_or_else(|| {
        "Pick the build's ota-manifest.json with its core.bin and engine.bin (its ota folder)."
            .to_string()
    })?;
    let manifest = OtaManifest::parse_valid(&manifest.bytes)
        .map_err(|error| format!("That ota-manifest.json is not one Studio can read: {error}."))?;
    if manifest.target != target {
        return Err(format!(
            "That build is for {}, and this board is {target}.",
            manifest.target
        ));
    }
    let piece = |name: &str| -> Result<Vec<u8>, String> {
        let file = named(name).ok_or_else(|| {
            format!(
                "{name} is missing: pick it with ota-manifest.json (they are in the same folder)."
            )
        })?;
        manifest
            .verify(name, &file.bytes)
            .map_err(|error| format!("{error}: it is not the file this manifest names."))?;
        Ok(file.bytes.clone())
    };
    let core = piece(&manifest.core.file)?;
    let engine = piece(&manifest.engine.file)?;
    let encoded = |file: &lpc_firmware_release::EncodedPieceFile| -> Option<EncodedPiece> {
        let picked = named(&file.file)?;
        manifest.verify(&file.file, &picked.bytes).ok()?;
        Some(EncodedPiece {
            stream: picked.bytes.clone(),
            chunks: file.chunks.clone(),
        })
    };
    let (core_z, engine_z) = match manifest.encoding1() {
        Some(e) => (encoded(&e.core), encoded(&e.engine)),
        None => (None, None),
    };
    let build = host_build_from_parts(&manifest, core, engine, core_z, engine_z)
        .map_err(|why| format!("That build is not one Studio can install: {why}."))?;
    Ok(FirmwareFileBuild {
        facts: build.facts(),
        build,
    })
}

/// A picked file's name without any folder (a folder pick names files by
/// their path inside it).
fn base_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// "From a file…": the offer that says a custom build can be installed
/// from files here. Discoverable, never pressable by an app agent — see the
/// module docs for why it carries no bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareFileOp {
    pub device: DeviceId,
}

impl FirmwareFileOp {
    /// Routed by `StudioController` directly, like the roster's own ops.
    pub const NODE_ID: &'static str = "studio|firmware-file";

    /// This offer as a dispatchable [`UiAction`](crate::UiAction). Pressing
    /// it for real opens a file picker, which only the web's own click
    /// handler can do.
    pub fn action_for(device: DeviceId) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), Self { device })
    }
}

impl ControllerOp for FirmwareFileOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "From a file…",
            "Pick a custom build's update files from your computer — ota-manifest.json, core.bin \
             and engine.bin, from its ota folder — to install over the air.",
            ActionPriority::Secondary,
        )
        .with_icon("upload")
        // A file picker wants a real click.
        .needs_user_activation()
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Passive {
            deadline: crate::PASSIVE_REFRESH_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

/// The files the person picked for `device`'s "From a file…": unpublished,
/// produced only by a real file read in the web (see the module docs).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareFileDataOp {
    pub device: DeviceId,
    pub files: Vec<PickedFirmwareFile>,
}

impl FirmwareFileDataOp {
    /// Routed by `StudioController` directly.
    pub const NODE_ID: &'static str = "studio|firmware-file-data";
}

/// The action that hands `files`, just read from a real file pick, to core
/// for `device`'s "From a file…".
pub fn firmware_file_action(device: DeviceId, files: Vec<PickedFirmwareFile>) -> crate::UiAction {
    crate::UiAction::from_op(
        crate::ControllerId::new(FirmwareFileDataOp::NODE_ID),
        FirmwareFileDataOp { device, files },
    )
}

impl ControllerOp for FirmwareFileDataOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Read the build's files",
            "Check the picked files against their manifest and offer the build in Other version….",
            ActionPriority::Secondary,
        )
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Foreground {
            deadline: crate::PROJECT_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_builds_ota_folder_reads_as_its_build() {
        let files = ota_folder(&sample());
        let read = read_firmware_files(&files, "esp32c6-4mb").expect("reads");
        assert_eq!(read.facts.identity.version, "2026.09.30-2");
        assert_eq!(read.build.identity.target, "esp32c6-4mb");
        assert_eq!(read.build.facts(), read.facts);
    }

    #[test]
    fn a_folder_path_is_read_by_its_file_name() {
        let files: Vec<PickedFirmwareFile> = ota_folder(&sample())
            .into_iter()
            .map(|file| PickedFirmwareFile {
                name: format!("ota/{}", file.name),
                ..file
            })
            .collect();
        assert!(read_firmware_files(&files, "esp32c6-4mb").is_ok());
    }

    #[test]
    fn what_is_wrong_with_the_files_is_said_in_words() {
        let sample = sample();
        let mut files = ota_folder(&sample);
        assert!(
            read_firmware_files(&files[1..], "esp32c6-4mb")
                .unwrap_err()
                .starts_with("Pick the build's ota-manifest.json")
        );
        assert_eq!(
            read_firmware_files(&files, "esp32s3-8mb").unwrap_err(),
            "That build is for esp32c6-4mb, and this board is esp32s3-8mb."
        );
        let engine = files.pop().unwrap();
        assert!(
            read_firmware_files(&files, "esp32c6-4mb")
                .unwrap_err()
                .starts_with("engine.bin is missing")
        );
        let mut damaged = engine.clone();
        damaged.bytes[0] ^= 1;
        files.push(damaged);
        let refusal = read_firmware_files(&files, "esp32c6-4mb").unwrap_err();
        assert!(refusal.contains("engine.bin: sha256"), "{refusal}");
        files[0].bytes = b"{}".to_vec();
        assert!(
            read_firmware_files(&files, "esp32c6-4mb")
                .unwrap_err()
                .starts_with("That ota-manifest.json is not one Studio can read")
        );
    }

    /// A build's three pieces of the sample: a core and an engine that
    /// carry the C6's image header, and its manifest.
    struct Sample {
        manifest: OtaManifest,
        core: Vec<u8>,
        engine: Vec<u8>,
    }

    fn sample() -> Sample {
        let core = super::super::device_update_fixtures::piece_bytes(0xC0, 4096 + 7);
        let engine = super::super::device_update_fixtures::piece_bytes(0xE0, 2 * 4096 + 9);
        let manifest =
            super::super::device_update_fixtures::ota_manifest_for("2026.09.30-2", &core, &engine);
        Sample {
            manifest,
            core,
            engine,
        }
    }

    fn ota_folder(sample: &Sample) -> Vec<PickedFirmwareFile> {
        vec![
            PickedFirmwareFile {
                name: FIRMWARE_FILE_MANIFEST.to_string(),
                bytes: sample.manifest.to_json_bytes(),
            },
            PickedFirmwareFile {
                name: "core.bin".to_string(),
                bytes: sample.core.clone(),
            },
            PickedFirmwareFile {
                name: "engine.bin".to_string(),
                bytes: sample.engine.clone(),
            },
        ]
    }
}
