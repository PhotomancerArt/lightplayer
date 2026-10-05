//! A synthetic split package for the OTA files' tests: no firmware build.
//!
//! A fake merged image with a fake loader (carrying the loader version
//! word), core and engine at `split` offsets; a fake `LPED` slot in the core
//! holding SHA-256 of the engine; a package manifest (schemaVersion 2) whose
//! `core` block is a manifest core with a `target` string, a chip word and
//! `wireProto`, and whose `split` block describes the image. The pieces are
//! pseudo-random with a 1 KiB block repeated every 10 KiB, so some chunks
//! compress only against their dictionary and the rest go raw.

use std::path::{Path, PathBuf};

use lpc_firmware_release::sha256_hex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::build_def::{BuildDef, find_repo_root, load_build_def};
use super::ota_files::{CORE_BIN, ENGINE_BIN, OtaFiles, PACKAGE_MANIFEST_FILE, write_ota_files};

/// The full commit every fixture is built from.
pub const COMMIT: &str = "0a1b2c3d4e5f60718293a4b5c6d7e8f901234567";
/// The split target.
pub const TARGET: &str = "esp32c6-4mb";
/// The packaged image's name.
pub const IMAGE: &str = "fw-esp32c6-merged.bin";

const LOADER_AT: usize = 0x1_0000;
const CORE_AT: usize = 0x1_8000;
const PAGE: usize = 0x8000;
const DIGEST_SLOT_AT: usize = 1000;

pub struct Fixture {
    _tmp: tempfile::TempDir,
    pub root: PathBuf,
    pub def: BuildDef,
    pub core: Vec<u8>,
    pub engine: Vec<u8>,
}

impl Fixture {
    /// A split package at `version`, packaged and with its parts in place,
    /// not yet with OTA files.
    pub fn new(version: &str) -> Self {
        Self::build(version, false)
    }

    /// The same, but the core's digest slot holds the wrong digest (the
    /// image, the parts and the manifest all agree with each other).
    pub fn with_wrong_digest_slot(version: &str) -> Self {
        Self::build(version, true)
    }

    fn build(version: &str, wrong_slot: bool) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let def = load_build_def(&find_repo_root().unwrap(), TARGET).unwrap();

        let mut rng = 0x9e37_79b9_7f4a_7c15u64;
        let engine = pieces_bytes(&mut rng, 80 * 1024 + 50);
        let mut core = pieces_bytes(&mut rng, 64 * 1024 + 100);
        let digest: [u8; 32] = if wrong_slot {
            [7; 32]
        } else {
            Sha256::digest(&engine).into()
        };
        let slot = lp_bootctl::engine_digest::encode(&digest);
        core[DIGEST_SLOT_AT..DIGEST_SLOT_AT + slot.len()].copy_from_slice(&slot);

        let mut loader = vec![0x5au8; 2048];
        loader[64..72].copy_from_slice(&lp_bootctl::loader_identity::loader_identity(1));

        let engine_at = (CORE_AT + core.len()).div_ceil(PAGE) * PAGE;
        let end = (engine_at + engine.len()).div_ceil(0x1000) * 0x1000;
        let mut image = vec![0xffu8; end];
        image[..16].copy_from_slice(b"fake bootloader!");
        image[LOADER_AT..LOADER_AT + loader.len()].copy_from_slice(&loader);
        image[CORE_AT..CORE_AT + core.len()].copy_from_slice(&core);
        image[engine_at..engine_at + engine.len()].copy_from_slice(&engine);

        let piece = |at: usize, bytes: &[u8]| {
            json!({ "offset": format!("{at:#x}"), "sizeBytes": bytes.len(),
                    "sha256": sha256_hex(bytes) })
        };
        let mut loader_piece = piece(LOADER_AT, &loader);
        loader_piece["version"] = 1.into();
        let mut engine_piece = piece(engine_at, &engine);
        engine_piece["header"] = 1.into();
        let manifest = json!({
            "schemaVersion": 2,
            "firmwareId": TARGET,
            "displayName": "LightPlayer ESP32-C6 server firmware",
            "generatedAt": "2026-10-05T00:00:00Z",
            "core": {
                "lpManifestCore": 3, "package": "fw-esp32c6", "version": version,
                "target": TARGET, "profile": "release-esp32", "commit": &COMMIT[..12],
                "dirty": false,
                "platform": { "family": "esp32", "chip": "esp32c6",
                              "cargoTarget": "riscv32imac-unknown-none-elf" },
                "features": [], "limits": {}, "wireProto": 36
            },
            "flash": { "format": "espflash-merged-image", "address": "0x0",
                       "flashSizeBytes": 4_194_304, "erasePolicy": "x",
                       "mayAffectDeviceData": true, "resetAfterFlash": true, "notes": "" },
            "images": [{ "path": IMAGE, "address": "0x0", "sizeBytes": image.len(),
                         "sha256": sha256_hex(&image) }],
            "split": {
                "layout": 1, "page": PAGE,
                "buildId": format!("{version}+{}", &COMMIT[..12]),
                "loader": loader_piece,
                "core": piece(CORE_AT, &core),
                "engine": engine_piece,
            }
        });

        let fx = Self {
            _tmp: tmp,
            root,
            def,
            core,
            engine,
        };
        std::fs::create_dir_all(fx.package_dir()).unwrap();
        std::fs::create_dir_all(fx.ota_dir()).unwrap();
        fx.write_package_manifest(&manifest);
        std::fs::write(fx.package_dir().join(IMAGE), &image).unwrap();
        std::fs::write(fx.ota_dir().join(CORE_BIN), &fx.core).unwrap();
        std::fs::write(fx.ota_dir().join(ENGINE_BIN), &fx.engine).unwrap();
        fx
    }

    /// Where every target's package directory is (`<root>/<target>/`).
    pub fn packages_root(&self) -> PathBuf {
        self.root.join("packages")
    }

    /// Where every target's parts/OTA directory is.
    pub fn parts_root(&self) -> PathBuf {
        self.root.join("parts")
    }

    pub fn package_dir(&self) -> PathBuf {
        self.packages_root().join(TARGET)
    }

    pub fn ota_dir(&self) -> PathBuf {
        self.parts_root().join(TARGET)
    }

    /// Run the writer, with a commit resolver that knows [`COMMIT`].
    pub fn write(&self) -> anyhow::Result<Option<OtaFiles>> {
        let resolve = |short: &str| COMMIT.starts_with(short).then(|| COMMIT.to_string());
        write_ota_files(&self.def, &self.package_dir(), &self.ota_dir(), &resolve)
    }

    /// Rewrite the package manifest through `edit`.
    pub fn edit_package(&self, edit: impl FnOnce(&mut Value)) {
        let path = self.package_dir().join(PACKAGE_MANIFEST_FILE);
        let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        edit(&mut manifest);
        self.write_package_manifest(&manifest);
    }

    /// Rebuild the fixture with a wrong digest slot in the core.
    pub fn corrupt_digest_slot(&self) {
        let version = self.package_core_version();
        let bad = Self::with_wrong_digest_slot(&version);
        for (from, to) in [
            (bad.package_dir(), self.package_dir()),
            (bad.ota_dir(), self.ota_dir()),
        ] {
            copy_dir(&from, &to);
        }
    }

    /// A non-split package for `target` (`chip`): its manifest and image.
    pub fn add_plain_package(&self, target: &str, chip: &str, version: &str) {
        let dir = self.packages_root().join(target);
        std::fs::create_dir_all(&dir).unwrap();
        let image = vec![0x33u8; 0x2000];
        let image_name = format!("fw-{chip}-merged.bin");
        let manifest = json!({
            "schemaVersion": 2, "firmwareId": target, "displayName": target,
            "generatedAt": "2026-10-05T00:00:00Z",
            "core": { "version": version, "target": target, "commit": &COMMIT[..12],
                      "platform": { "family": "esp32", "chip": chip,
                                    "cargoTarget": "x" }, "wireProto": 36 },
            "flash": { "format": "espflash-merged-image", "address": "0x0",
                       "flashSizeBytes": 8_388_608, "erasePolicy": "x",
                       "mayAffectDeviceData": true, "resetAfterFlash": true, "notes": "" },
            "images": [{ "path": image_name, "address": "0x0", "sizeBytes": image.len(),
                         "sha256": sha256_hex(&image) }]
        });
        std::fs::write(dir.join(&image_name), &image).unwrap();
        let mut bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        bytes.push(b'\n');
        std::fs::write(dir.join(PACKAGE_MANIFEST_FILE), bytes).unwrap();
    }

    fn package_core_version(&self) -> String {
        let path = self.package_dir().join(PACKAGE_MANIFEST_FILE);
        let manifest: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        manifest["core"]["version"].as_str().unwrap().to_string()
    }

    fn write_package_manifest(&self, manifest: &Value) {
        let mut bytes = serde_json::to_vec_pretty(manifest).unwrap();
        bytes.push(b'\n');
        std::fs::write(self.package_dir().join(PACKAGE_MANIFEST_FILE), bytes).unwrap();
    }
}

/// Pseudo-random bytes with one 1 KiB block repeated at every 10 KiB.
fn pieces_bytes(rng: &mut u64, len: usize) -> Vec<u8> {
    let mut next = || {
        *rng ^= *rng << 13;
        *rng ^= *rng >> 7;
        *rng ^= *rng << 17;
        (*rng >> 24) as u8
    };
    let block: Vec<u8> = (0..1024).map(|_| next()).collect();
    (0..len)
        .map(|i| {
            if i % (10 * 1024) < 1024 {
                block[i % 1024]
            } else {
                next()
            }
        })
        .collect()
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(&path, to.join(path.file_name().unwrap())).unwrap();
    }
}
