//! [`HostBuild::from_ota_manifest`]: a build from a release's
//! `ota-manifest.json` (the firmware distribution's format 1,
//! `lpc_firmware_release::OtaManifest`) and the files it names.
//!
//! This is the one place a host turns published files into something it can
//! serve (one-way-doors §4): `lp-cli`'s `--ota-offer` and Studio (M7) both
//! come through here. It never reads a package's `split` block.
//!
//! - `core.file` / `engine.file` are read and checked against the manifest
//!   (length, SHA-256) with its own `verify`;
//! - **encoding `id: 1`** is picked by id alone, if listed: its `.z` files
//!   are read, checked, and taken with the manifest's `chunks` index as each
//!   piece's encoded form (`0` = raw). An entry with any other id is skipped;
//!   no encoding at all is fine (raw only);
//! - the identity is the manifest's: `target`, `chip`, `version`, the build
//!   id it derives (`version + "+" + commit[..12]`), `wireProto`, and
//!   `requires` as the offer's `layout` / `min_loader`.
//!
//! The reader is injected (`no_std` + `alloc`): `lp-cli` reads a directory,
//! Studio its fetched files.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_firmware_release::{EncodedPieceFile, OtaManifest};
use lpc_update::PieceKind;

use crate::encoded_piece::EncodedPiece;
use crate::host_build::{HostBuild, HostBuildError, HostIdentity};

impl HostBuild {
    /// The build `manifest` describes, its files read with `read(name)`.
    /// See the module docs.
    pub fn from_ota_manifest(
        manifest: &OtaManifest,
        read: impl Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<Self, HostBuildError> {
        let file = |name: &str| -> Result<Vec<u8>, HostBuildError> {
            let bytes = read(name).ok_or_else(|| HostBuildError::MissingFile(name.into()))?;
            manifest
                .verify(name, &bytes)
                .map_err(|e| HostBuildError::FileCheck(alloc::format!("{e}")))?;
            Ok(bytes)
        };
        let core = file(&manifest.core.file)?;
        let engine = file(&manifest.engine.file)?;
        let (core_z, engine_z) = match manifest.encoding1() {
            Some(e) => (
                Some(encoded(&e.core, &file)?),
                Some(encoded(&e.engine, &file)?),
            ),
            None => (None, None),
        };
        let identity = HostIdentity {
            target: manifest.target.clone(),
            chip: manifest.chip.clone(),
            version: manifest.version.clone(),
            build_id: manifest.build_id(),
            wire_proto: manifest.wire_proto,
            layout: manifest.requires.layout,
            min_loader: manifest.requires.loader,
        };
        Self::from_parts(identity, core, engine, core_z, engine_z)
    }
}

/// One piece's `.z` file, read and checked, with its index.
fn encoded(
    z: &EncodedPieceFile,
    file: &impl Fn(&str) -> Result<Vec<u8>, HostBuildError>,
) -> Result<EncodedPiece, HostBuildError> {
    Ok(EncodedPiece {
        stream: file(&z.file)?,
        chunks: z.chunks.clone(),
    })
}

/// The piece a `.z` file is for, by which slot of encoding 1 lists it.
#[must_use]
pub fn encoded_piece_kind(manifest: &OtaManifest, file: &str) -> Option<PieceKind> {
    let e = manifest.encoding1()?;
    if e.core.file == file {
        Some(PieceKind::Core)
    } else if e.engine.file == file {
        Some(PieceKind::Engine)
    } else {
        None
    }
}

/// The file names `manifest` asks [`HostBuild::from_ota_manifest`] to read.
#[must_use]
pub fn files_to_read(manifest: &OtaManifest) -> Vec<String> {
    let mut out = alloc::vec![manifest.core.file.clone(), manifest.engine.file.clone()];
    if let Some(e) = manifest.encoding1() {
        out.push(e.core.file.clone());
        out.push(e.engine.file.clone());
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;
    use alloc::vec;
    use lpc_firmware_release::{
        Encoding1, EncodingEntry, PackageRef, PieceFile, Requires, sha256_hex,
    };

    #[test]
    fn a_raw_only_manifest_makes_a_build() {
        let (m, files) = release(false);
        let b = HostBuild::from_ota_manifest(&m, |n| files.get(n).cloned()).unwrap();
        assert_eq!(b.identity.build_id, "2026.10.05-3+abc123456789");
        assert_eq!(b.identity.target, "esp32c6-4mb");
        assert_eq!((b.identity.layout, b.identity.min_loader), (1, 1));
        assert_eq!(b.identity.wire_proto, 37);
        assert_eq!(b.core.len, 9000);
        assert!(b.core.encoded.is_none() && b.engine.encoded.is_none());
    }

    #[test]
    fn encoding_one_is_taken_by_its_index() {
        let (m, files) = release(true);
        let b = HostBuild::from_ota_manifest(&m, |n| files.get(n).cloned()).unwrap();
        let e = b.core.encoded.as_ref().unwrap();
        assert_eq!(e.chunks, vec![3, 0, 2]);
        assert_eq!(b.core.encoded_chunk(0), Some(&[1, 2, 3][..]));
        assert_eq!(b.core.encoded_chunk(1), None, "0 = send raw");
        assert_eq!(encoded_piece_kind(&m, "engine.z"), Some(PieceKind::Engine));
        assert_eq!(files_to_read(&m).len(), 4);
    }

    #[test]
    fn an_unknown_encoding_is_skipped() {
        let (mut m, files) = release(false);
        let json = serde_json::json!({ "id": 9, "codec": "zstd", "whatever": [1, 2] });
        let entry: EncodingEntry = serde_json::from_value(json).unwrap();
        m.encodings.push(entry);
        let b = HostBuild::from_ota_manifest(&m, |n| files.get(n).cloned()).unwrap();
        assert!(b.core.encoded.is_none());
    }

    #[test]
    fn a_file_that_does_not_match_its_manifest_is_refused() {
        let (m, mut files) = release(true);
        files.get_mut("engine.bin").unwrap()[7] ^= 1;
        assert!(matches!(
            HostBuild::from_ota_manifest(&m, |n| files.get(n).cloned()),
            Err(HostBuildError::FileCheck(_))
        ));
        let (m, mut files) = release(true);
        files.remove("core.z");
        assert_eq!(
            HostBuild::from_ota_manifest(&m, |n| files.get(n).cloned()),
            Err(HostBuildError::MissingFile("core.z".to_string()))
        );
    }

    /// A synthetic release: a 9000-byte core, a 5000-byte engine, and (with
    /// `z`) an encoding 1 whose streams are placeholders of the right shape.
    pub(crate) fn release(z: bool) -> (OtaManifest, BTreeMap<String, Vec<u8>>) {
        let mut files = BTreeMap::new();
        let core = vec![0x11u8; 9000];
        let engine = vec![0x22u8; 5000];
        let piece = |name: &str, bytes: &[u8]| PieceFile {
            file: name.into(),
            length: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        };
        let mut encodings = Vec::new();
        if z {
            let core_z = vec![1u8, 2, 3, 4, 5];
            let engine_z = vec![9u8; 4];
            let z_file = |name: &str, bytes: &[u8], chunks: Vec<u32>| EncodedPieceFile {
                file: name.into(),
                length: bytes.len() as u64,
                sha256: sha256_hex(bytes),
                chunks,
            };
            encodings.push(EncodingEntry::deflate1(Encoding1 {
                id: 1,
                codec: "deflate-raw".into(),
                chunk_bytes: 4096,
                window_bytes: 32768,
                core: z_file("core.z", &core_z, vec![3, 0, 2]),
                engine: z_file("engine.z", &engine_z, vec![4, 0]),
            }));
            files.insert("core.z".to_string(), core_z);
            files.insert("engine.z".to_string(), engine_z);
        }
        let m = OtaManifest {
            format: 1,
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: "2026.10.05-3".into(),
            commit: "abc1234567890123456789012345678901234567".into(),
            wire_proto: 37,
            requires: Requires {
                layout: 1,
                loader: 1,
            },
            core: piece("core.bin", &core),
            engine: piece("engine.bin", &engine),
            encodings,
            package: PackageRef {
                file: "package.json".into(),
                length: 1,
                sha256: sha256_hex(b"x"),
                image: piece("fw-esp32c6-merged.bin", b"y"),
            },
        };
        m.validate().unwrap();
        files.insert("core.bin".to_string(), core);
        files.insert("engine.bin".to_string(), engine);
        (m, files)
    }
}
