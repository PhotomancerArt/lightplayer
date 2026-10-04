//! `manifest.json` schemaVersion 2 — the distribution manifest a packaged
//! firmware directory ships beside its images.
//!
//! v2 is a **projection**: `core` is the manifest core extracted verbatim
//! from the built artifact (features, limits, wire proto, provenance), and
//! everything else is a distribution fact the artifact cannot know (which
//! variant produced it, how to flash it, where the bytes are). v1 restated
//! the feature list by hand in a `.mjs` and `sed`-ed the wire proto out of a
//! source file; both are gone.
//!
//! Consumers version + refuse: `schemaVersion` 2 only, no dual decode.
//!
//! A split build (the ESP32-C6) adds a `split` block — additive, so the
//! schema stays 2: where the loader, the core and the engine sit **inside the
//! merged image** (flash offsets), with their lengths and SHA-256s, so a
//! later Studio can slice the parts out of the bytes it already has. Once
//! published, a package manifest is read by every future Studio that
//! reinstalls that release: keys are only ever added.

use serde::Serialize;
use serde_json::Value;

/// Only `schemaVersion` this tool emits.
pub const MANIFEST_SCHEMA_VERSION: u32 = 2;

/// Flash-image format identifier for an espflash merged image.
pub const FLASH_FORMAT_MERGED_IMAGE: &str = "espflash-merged-image";

/// The packaged `manifest.json`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistributionManifest {
    /// [`MANIFEST_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Build-def id (`esp32c6-4mb`); also the directory this manifest lives in.
    pub firmware_id: String,
    /// Human label from the build def.
    pub display_name: String,
    /// Packaging timestamp (UTC, RFC 3339).
    pub generated_at: String,
    /// The manifest core extracted from the artifact, verbatim. Held as raw
    /// JSON so packaging cannot quietly reshape what the build said about
    /// itself.
    pub core: Value,
    /// How to write these images to a device.
    pub flash: FlashPolicy,
    /// The images, in flash order.
    pub images: Vec<ManifestImage>,
    /// Where a split image's pieces are inside the merged image; absent for
    /// a single linked image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<SplitBlock>,
}

/// The `split` block: the layout (an integer, the same one the board
/// reports), the MMU page it assumes, the build id the core and engine
/// share, and the three pieces.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitBlock {
    /// `1`: the layout `lp_bootctl::SplitLayout` describes.
    pub layout: u32,
    /// The MMU page the layout assumes, in bytes.
    pub page: u32,
    /// `"<version>+<commit>"`.
    pub build_id: String,
    pub loader: SplitPiece,
    pub core: SplitPiece,
    pub engine: SplitPiece,
}

/// One piece of a split image, inside the merged image.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPiece {
    /// Flash offset, hex.
    pub offset: String,
    pub size_bytes: u64,
    /// Lowercase hex SHA-256 of the piece exactly as flashed.
    pub sha256: String,
    /// The loader's version word.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u16>,
    /// The engine header's version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<u16>,
}

/// Flashing policy — the destructive-operation contract the link layer and
/// the browser flasher both read before touching a board.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashPolicy {
    /// [`FLASH_FORMAT_MERGED_IMAGE`].
    pub format: String,
    /// Base address of the merged image.
    pub address: String,
    /// Flash size the image header declares — must match the physical chip
    /// or the bootloader rejects the partition table (see the ESP32-S3 8 MB
    /// floor, `docs/adr/2026-07-30-esp32s3-partition-floor.md`).
    pub flash_size_bytes: u64,
    /// What flashing overwrites.
    pub erase_policy: String,
    /// Whether flashing can destroy device-side data (projects, settings).
    pub may_affect_device_data: bool,
    /// Whether the flasher should reset the board afterwards.
    pub reset_after_flash: bool,
    /// Free-form operator note.
    pub notes: String,
}

/// One flashable image.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestImage {
    /// File name, relative to the manifest.
    pub path: String,
    /// Flash offset, hex.
    pub address: String,
    /// Byte length.
    pub size_bytes: u64,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
}

impl FlashPolicy {
    /// The policy for an espflash merged image written at 0x0. Semantics
    /// carried over from the v1 generator.
    pub fn merged_image(flash_size_bytes: u64) -> Self {
        Self {
            format: FLASH_FORMAT_MERGED_IMAGE.to_string(),
            address: "0x0".to_string(),
            flash_size_bytes,
            erase_policy: "write-bootloader-partition-table-and-app".to_string(),
            may_affect_device_data: true,
            reset_after_flash: true,
            notes: "Merged image generated by espflash save-image --merge \
                    --skip-padding. Treat as destructive: it rewrites the \
                    bootloader, partition table and app."
                .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The emitted shape is what consumers decode: `core` verbatim under its
    /// own key, distribution facts beside it, camelCase throughout.
    #[test]
    fn serializes_the_v2_shape() {
        let core: Value = serde_json::from_str(
            r#"{"lpManifestCore":1,"package":"fw-esp32c6","commit":"abc","dirty":false}"#,
        )
        .unwrap();
        let manifest = DistributionManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            firmware_id: "esp32c6-4mb".to_string(),
            display_name: "LightPlayer ESP32-C6 server firmware".to_string(),
            generated_at: "2026-08-01T12:00:00Z".to_string(),
            core: core.clone(),
            flash: FlashPolicy::merged_image(4 * 1024 * 1024),
            images: vec![ManifestImage {
                path: "fw-esp32c6-merged.bin".to_string(),
                address: "0x0".to_string(),
                size_bytes: 3_022_960,
                sha256: "ab".repeat(32),
            }],
            split: None,
        };

        let json: Value = serde_json::from_str(&serde_json::to_string(&manifest).unwrap()).unwrap();
        assert_eq!(json["schemaVersion"], 2);
        assert_eq!(json["firmwareId"], "esp32c6-4mb");
        assert_eq!(json["core"], core);
        assert_eq!(json["core"]["commit"], "abc");
        assert_eq!(json["flash"]["format"], FLASH_FORMAT_MERGED_IMAGE);
        assert_eq!(json["flash"]["flashSizeBytes"], 4_194_304);
        assert_eq!(json["flash"]["mayAffectDeviceData"], true);
        assert_eq!(json["images"][0]["sizeBytes"], 3_022_960);
        assert_eq!(json["images"][0]["address"], "0x0");
        assert!(
            json.get("split").is_none(),
            "a single image has no split block"
        );
    }

    /// The split block's shape, as a later Studio reads it.
    #[test]
    fn serializes_the_split_block() {
        let piece = |offset: &str, size_bytes| SplitPiece {
            offset: offset.to_string(),
            size_bytes,
            sha256: "cd".repeat(32),
            version: None,
            header: None,
        };
        let block = SplitBlock {
            layout: 1,
            page: 32768,
            build_id: "2026.10.05-1+abc123456789".to_string(),
            loader: SplitPiece {
                version: Some(1),
                ..piece("0x10000", 2800)
            },
            core: piece("0x18000", 1_161_104),
            engine: SplitPiece {
                header: Some(1),
                ..piece("0x138000", 1_824_152)
            },
        };
        let json: Value = serde_json::to_value(&block).unwrap();
        assert_eq!(json["layout"], 1);
        assert_eq!(json["page"], 32768);
        assert_eq!(json["buildId"], "2026.10.05-1+abc123456789");
        assert_eq!(json["loader"]["offset"], "0x10000");
        assert_eq!(json["loader"]["version"], 1);
        assert!(json["loader"].get("header").is_none());
        assert_eq!(json["core"]["sizeBytes"], 1_161_104);
        assert!(json["core"].get("version").is_none());
        assert_eq!(json["engine"]["header"], 1);
        assert_eq!(json["engine"]["sha256"], "cd".repeat(32));
        assert!(json.get("engineDigestInCore").is_none());
    }
}
