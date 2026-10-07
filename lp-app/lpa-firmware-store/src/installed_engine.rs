//! The engine a USB install just wrote, out of the package that wrote it —
//! so Studio keeps every engine it installs (plan D19).
//!
//! A split package's manifest (`manifest.json`, schemaVersion 2) places the
//! engine inside the merged image by its `split` block: a flash offset and a
//! length, with the engine's SHA-256. The merged image is written at flash
//! `0x0`, so the offset is an index into it. [`InstalledPackage::parse`]
//! reads only the fields this needs, with a local struct that ignores the
//! rest (the published `package.json` is additive-only; readers ignore
//! unknown keys). A package with no `split` block (the S3, the classic, the
//! C6's single-image local build) has no engine to keep.
//!
//! The image is checked against the package's own length and SHA-256 before
//! anything is sliced, and the slice against the split block's engine hash,
//! so the bytes a cache entry is keyed by are the bytes the board's core
//! names (its digest slot, proven at package time and by `release-check`).

use lpc_firmware_release::sha256_hex;
use serde::Deserialize;

use crate::engine_cache_entry::{EngineCacheEntry, EngineSource};

/// What a split package says about its merged image and its engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledPackage {
    /// The package's target (`firmwareId`, the build definition id).
    pub target: String,
    /// The image's version (its manifest core's `version`), when it says.
    pub version: Option<String>,
    /// `version+commit[..12]` (the split block's `buildId`).
    pub build_id: String,
    /// The merged image's file name, beside the package manifest.
    pub image_file: String,
    image_length: u64,
    image_sha256: String,
    engine_offset: u64,
    engine_length: u64,
    engine_sha256: String,
}

impl InstalledPackage {
    /// Read a package manifest. `Ok(None)` for a package with no `split`
    /// block: nothing to keep.
    pub fn parse(package_manifest: &[u8]) -> Result<Option<Self>, String> {
        let manifest: PackageManifest = serde_json::from_slice(package_manifest)
            .map_err(|e| format!("the package manifest does not parse: {e}"))?;
        let Some(split) = manifest.split else {
            return Ok(None);
        };
        let [image] = manifest.images.as_slice() else {
            return Err(format!(
                "a split package flashes one merged image, this one lists {}",
                manifest.images.len()
            ));
        };
        Ok(Some(Self {
            target: manifest.firmware_id,
            version: manifest.core.and_then(|core| core.version),
            build_id: split.build_id,
            image_file: image.path.clone(),
            image_length: image.size_bytes,
            image_sha256: image.sha256.to_ascii_lowercase(),
            engine_offset: parse_hex_offset(&split.engine.offset)?,
            engine_length: split.engine.size_bytes,
            engine_sha256: split.engine.sha256.to_ascii_lowercase(),
        }))
    }

    /// The engine's SHA-256, as the split block names it.
    pub fn engine_sha256(&self) -> &str {
        &self.engine_sha256
    }

    /// Check `image` is this package's merged image, slice the engine out,
    /// check it, and describe it as an `installed` cache entry at `now`
    /// (epoch seconds).
    pub fn engine(&self, image: &[u8], now: f64) -> Result<(EngineCacheEntry, Vec<u8>), String> {
        if image.len() as u64 != self.image_length || sha256_hex(image) != self.image_sha256 {
            return Err(format!(
                "{} is {} bytes with sha256 {}, not the package's {} bytes with sha256 {}",
                self.image_file,
                image.len(),
                sha256_hex(image),
                self.image_length,
                self.image_sha256
            ));
        }
        let start = usize::try_from(self.engine_offset).map_err(|e| e.to_string())?;
        let len = usize::try_from(self.engine_length).map_err(|e| e.to_string())?;
        let end = start
            .checked_add(len)
            .filter(|end| *end <= image.len())
            .ok_or_else(|| {
                format!(
                    "the engine at 0x{start:x} ({len} B) runs past the image's end ({} B)",
                    image.len()
                )
            })?;
        let bytes = image[start..end].to_vec();
        let sha256 = sha256_hex(&bytes);
        if sha256 != self.engine_sha256 {
            return Err(format!(
                "the engine sliced at 0x{start:x} hashes to {sha256}, the split block says {}",
                self.engine_sha256
            ));
        }
        let mut entry =
            EngineCacheEntry::new(sha256, bytes.len() as u64, EngineSource::Installed, now);
        entry.target = Some(self.target.clone());
        entry.build_id = Some(self.build_id.clone());
        entry.version = self.version.clone();
        Ok((entry, bytes))
    }
}

/// [`InstalledPackage::parse`] then [`InstalledPackage::engine`]: the
/// engine a package's merged image carries, or `Ok(None)` for a package
/// with no `split` block.
pub fn engine_from_merged_image(
    package_manifest: &[u8],
    image: &[u8],
    now: f64,
) -> Result<Option<(EngineCacheEntry, Vec<u8>)>, String> {
    match InstalledPackage::parse(package_manifest)? {
        Some(package) => package.engine(image, now).map(Some),
        None => Ok(None),
    }
}

/// The fields of `manifest.json` this reads; everything else is ignored.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageManifest {
    firmware_id: String,
    #[serde(default)]
    core: Option<ManifestCore>,
    images: Vec<PackageImage>,
    #[serde(default)]
    split: Option<SplitBlock>,
}

#[derive(Deserialize)]
struct ManifestCore {
    #[serde(default)]
    version: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageImage {
    path: String,
    size_bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SplitBlock {
    build_id: String,
    engine: SplitPiece,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SplitPiece {
    /// Flash offset, written `0x…`.
    offset: String,
    size_bytes: u64,
    sha256: String,
}

fn parse_hex_offset(offset: &str) -> Result<u64, String> {
    offset
        .strip_prefix("0x")
        .or_else(|| offset.strip_prefix("0X"))
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .ok_or_else(|| format!("split offset `{offset}` is not 0x-hex"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_split_package_yields_its_engine_as_installed() {
        let fx = SplitFixture::new();
        let (entry, bytes) = engine_from_merged_image(&fx.manifest(), &fx.image, 7.0)
            .unwrap()
            .expect("a split package has an engine");
        assert_eq!(bytes, fx.engine);
        assert_eq!(entry.sha256, sha256_hex(&fx.engine));
        assert_eq!(entry.length, fx.engine.len() as u64);
        assert_eq!(entry.source, EngineSource::Installed);
        assert_eq!(entry.target.as_deref(), Some("esp32c6-4mb"));
        assert_eq!(entry.build_id.as_deref(), Some("2026.10.05-3+103285d5d05e"));
        assert_eq!(entry.version.as_deref(), Some("2026.10.05-3"));
        assert_eq!(entry.added_at_epoch_seconds, 7.0);
        assert!(!entry.held);
    }

    #[test]
    fn a_package_without_a_split_block_has_nothing_to_keep() {
        let fx = SplitFixture::new();
        let mut manifest: serde_json::Value = serde_json::from_slice(&fx.manifest()).unwrap();
        manifest.as_object_mut().unwrap().remove("split");
        let manifest = serde_json::to_vec(&manifest).unwrap();
        assert_eq!(
            engine_from_merged_image(&manifest, &fx.image, 1.0).unwrap(),
            None
        );
    }

    #[test]
    fn an_image_that_is_not_the_packages_is_refused() {
        let fx = SplitFixture::new();
        let mut image = fx.image.clone();
        image[3] ^= 1;
        let error = engine_from_merged_image(&fx.manifest(), &image, 1.0).unwrap_err();
        assert!(error.contains("not the package's"), "{error}");
    }

    #[test]
    fn an_engine_hash_that_disagrees_with_the_split_block_is_refused() {
        let mut fx = SplitFixture::new();
        fx.engine_sha256 = sha256_hex(b"another engine");
        let error = engine_from_merged_image(&fx.manifest(), &fx.image, 1.0).unwrap_err();
        assert!(error.contains("the split block says"), "{error}");
    }

    #[test]
    fn an_engine_past_the_images_end_is_refused() {
        let mut fx = SplitFixture::new();
        fx.engine_offset = fx.image.len() as u64 - 10;
        let error = engine_from_merged_image(&fx.manifest(), &fx.image, 1.0).unwrap_err();
        assert!(error.contains("runs past the image's end"), "{error}");
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let fx = SplitFixture::new();
        let mut manifest: serde_json::Value = serde_json::from_slice(&fx.manifest()).unwrap();
        manifest["aFutureKey"] = serde_json::json!({"anything": [1, 2]});
        manifest["split"]["aFutureSplitKey"] = 3.into();
        let manifest = serde_json::to_vec(&manifest).unwrap();
        assert!(
            engine_from_merged_image(&manifest, &fx.image, 1.0)
                .unwrap()
                .is_some()
        );
    }

    /// A synthetic merged image with an engine at a flash offset, and the
    /// manifest fields that place it.
    struct SplitFixture {
        image: Vec<u8>,
        engine: Vec<u8>,
        engine_offset: u64,
        engine_sha256: String,
    }

    impl SplitFixture {
        fn new() -> Self {
            let mut image = vec![0xffu8; 0x6000];
            let engine: Vec<u8> = (0..0x1800u32).map(|i| (i * 7 + 3) as u8).collect();
            let offset = 0x4000usize;
            image[offset..offset + engine.len()].copy_from_slice(&engine);
            Self {
                engine_sha256: sha256_hex(&engine),
                image,
                engine,
                engine_offset: offset as u64,
            }
        }

        fn manifest(&self) -> Vec<u8> {
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 2,
                "firmwareId": "esp32c6-4mb",
                "core": { "version": "2026.10.05-3", "target": "esp32c6-4mb" },
                "images": [{
                    "path": "fw-esp32c6-merged.bin", "address": "0x0",
                    "sizeBytes": self.image.len(), "sha256": sha256_hex(&self.image)
                }],
                "split": {
                    "layout": 1, "page": 32768,
                    "buildId": "2026.10.05-3+103285d5d05e",
                    "loader": { "offset": "0x1000", "sizeBytes": 16, "sha256": "aa", "version": 1 },
                    "core": { "offset": "0x2000", "sizeBytes": 16, "sha256": "bb" },
                    "engine": {
                        "offset": format!("0x{:x}", self.engine_offset),
                        "sizeBytes": self.engine.len(),
                        "sha256": self.engine_sha256,
                        "header": 1
                    }
                }
            }))
            .unwrap()
        }
    }
}
