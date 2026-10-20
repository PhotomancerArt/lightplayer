//! Does the firmware about to be written read the tree store?
//!
//! A board whose `lpfs` holds a tree store (`tree_store_magic`) may be
//! flashed only with firmware that mounts it: a littlefs core would find no
//! littlefs there and — before this guard's firmware half — format it,
//! every file gone. The firmware says what it reads in its embedded manifest
//! core (the `lp_embed_manifest_core!` blob a merged image carries): an
//! `fs-tree` build lists the feature `fs.tree` (`LpFeature::FsTree`), the
//! same fact the board's own update guard reads in a new core
//! (`fw_esp32_common::fs_tree_core_guard`).
//!
//! A provider only finds the manifest ([`embedded_manifest`]) and hands its
//! bytes over (`LinkLayoutInspection::target_manifest`); the decision reads
//! them here ([`firmware_reads_tree_store`]). An image with no readable
//! manifest does not read the store: nothing says it keeps it.

use lpc_model::LpFeature;
use lpc_model::manifest::find_manifest_core;

/// The manifest core JSON embedded in the first of `images` (a package's
/// flash images) that carries one, copied out.
pub fn embedded_manifest<'a>(images: impl IntoIterator<Item = &'a [u8]>) -> Option<Vec<u8>> {
    images
        .into_iter()
        .find_map(find_manifest_core)
        .map(<[u8]>::to_vec)
}

/// Does the manifest core JSON `manifest` list `fs.tree` among its
/// features? `None` (no manifest) does not. Read as plain JSON, so a newer
/// build's unknown features never hide it.
pub fn firmware_reads_tree_store(manifest: Option<&[u8]>) -> bool {
    let Some(Ok(json)) = manifest.map(serde_json::from_slice::<serde_json::Value>) else {
        return false;
    };
    json.get("features")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|features| {
            features
                .iter()
                .any(|f| f.as_str() == Some(LpFeature::FsTree.wire_name()))
        })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use lpc_model::manifest::{MANIFEST_BLOB_BEGIN, MANIFEST_BLOB_END};

    /// A stand-in image carrying a manifest core with `features` (a JSON
    /// array's insides).
    pub(crate) fn image_with(features: &str) -> Vec<u8> {
        let mut image = vec![0xA5u8; 4096];
        image.extend_from_slice(MANIFEST_BLOB_BEGIN.as_bytes());
        image.extend_from_slice(
            format!("{{\"package\":\"fw-esp32c6\",\"features\":[{features}]}}").as_bytes(),
        );
        image.extend_from_slice(MANIFEST_BLOB_END.as_bytes());
        image.extend_from_slice(&[0xFF; 512]);
        image
    }

    fn reads(images: &[&[u8]]) -> bool {
        firmware_reads_tree_store(embedded_manifest(images.iter().copied()).as_deref())
    }

    #[test]
    fn a_tree_build_reads_the_store_and_a_littlefs_build_does_not() {
        assert!(reads(&[&image_with("\"gfx.lpvm\",\"fs.tree\"")]));
        assert!(!reads(&[&image_with("\"gfx.lpvm\",\"svc.button\"")]));
    }

    #[test]
    fn no_manifest_means_it_does_not() {
        assert!(!reads(&[&[0u8; 8192]]));
        assert!(!reads(&[]));
        assert!(!firmware_reads_tree_store(Some(b"not json")));
    }

    /// A feature this build has never heard of does not hide `fs.tree`.
    #[test]
    fn an_unknown_feature_beside_it_does_not_hide_it() {
        assert!(reads(&[&image_with("\"some.future\",\"fs.tree\"")]));
    }

    /// The images are searched in order: a later image's manifest counts
    /// when the first has none.
    #[test]
    fn the_manifest_may_be_in_any_image() {
        assert!(reads(&[&[0xFFu8; 1024], &image_with("\"fs.tree\"")]));
    }
}
