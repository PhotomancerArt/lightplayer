//! Which filesystem a region of flash holds, and which one a firmware image
//! mounts.
//!
//! Two kinds share the `lpfs` partition across the C6's history: **littlefs**
//! (every board fielded before the tree-store cutover) and the **tree store**
//! (`lp-tree-store`, plan `lp2025/2026-10-16-0848-tree-store-adoption`). Both
//! are recognised by their magic alone — the littlefs superblock's
//! `littlefs` at byte 8 of block 0 or 1, the tree store's `LTS1` sector
//! header in any sector — which decides what to read next, never whether
//! files are intact (a full mount does that).
//!
//! What an image mounts is read from the image itself: its embedded manifest
//! core lists `fs.tree` when it was built with the tree store
//! (`lpc_model::LpFeature::FsTree`), the same fact the board's hello carries
//! and the `fs-tree` core's update guard reads
//! (`fw_esp32_common::fs_tree_core_guard`).

use lp_tree_store::SectorState;

use super::lpfs_geometry::{LPFS_BLOCK_SIZE, has_superblock};

/// The tree store's sector size on the C6 (FORMAT.md "The medium").
pub const TREE_SECTOR_SIZE: u32 = 4096;
/// A tree-store sector header's length (FORMAT.md "Sector").
pub const TREE_HEADER_LEN: u32 = 24;

pub use crate::provider::fs_kind::FsKind;

/// The filesystem `merged` (a firmware package's merged image, or any build
/// artifact) mounts: [`FsKind::Tree`] when its manifest core lists `fs.tree`,
/// [`FsKind::Littlefs`] otherwise — every image built before the tree store
/// existed carries no such feature, and every one of them mounts littlefs.
pub fn image_fs_kind(merged: &[u8]) -> FsKind {
    let Some(json) = lpc_model::manifest::find_manifest_core(merged) else {
        return FsKind::Littlefs;
    };
    let Ok(core) = serde_json::from_slice::<serde_json::Value>(json) else {
        return FsKind::Littlefs;
    };
    let tree = lpc_model::LpFeature::FsTree.wire_name();
    let lists_tree = core
        .get("features")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|features| features.iter().any(|f| f.as_str() == Some(tree)));
    if lists_tree {
        FsKind::Tree
    } else {
        FsKind::Littlefs
    }
}

/// Does the sector starting at `sector_start` (at least its 24 header bytes)
/// carry a tree-store header a firmware would not format over: a trusted
/// version-3 header, or the magic in front of a newer version or an
/// unsupported layout (FORMAT.md "Sector", rules 0, 2 and 3 — a refused
/// store is a store)?
pub fn tree_header_present(sector_start: &[u8]) -> bool {
    let Some(head) = sector_start.get(..TREE_HEADER_LEN as usize) else {
        return false;
    };
    // The inspector classifies a whole sector; the header decides the state
    // and an erased tail ends its record walk at once.
    let mut sector = vec![0xFFu8; TREE_SECTOR_SIZE as usize];
    sector[..head.len()].copy_from_slice(head);
    let report = lp_tree_store::scan_sector(&sector, 0, TREE_SECTOR_SIZE);
    matches!(
        report.state,
        SectorState::Valid | SectorState::Newer { .. } | SectorState::Unsupported { .. }
    )
}

/// The sectors of `region` (whole 4 KB sectors from its start) that carry a
/// tree-store header ([`tree_header_present`]).
pub fn tree_sectors(region: &[u8]) -> Vec<u32> {
    region
        .chunks(TREE_SECTOR_SIZE as usize)
        .enumerate()
        .filter(|(_, sector)| tree_header_present(sector))
        .map(|(index, _)| index as u32)
        .collect()
}

/// The filesystem `region` holds by its magic: littlefs when its superblock
/// pair carries one (block 0 or 1), the tree store when any sector carries a
/// store header, `None` for neither. A littlefs superblock wins: a tree
/// store's sectors beside a mounting littlefs are an interrupted migration's
/// (staged in blocks littlefs does not use), and littlefs is still the
/// board's filesystem.
pub fn fs_in_region(region: &[u8]) -> Option<FsKind> {
    if region.len() >= 2 * LPFS_BLOCK_SIZE as usize && has_superblock(region) {
        return Some(FsKind::Littlefs);
    }
    (!tree_sectors(region).is_empty()).then_some(FsKind::Tree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_migration::tree_image::{RamNor, store_config};
    use lp_tree_store::{SoftSha256, TreeStore};

    fn store_region(sectors: u32) -> Vec<u8> {
        let flash = RamNor::erased(sectors);
        let mut store = TreeStore::format(flash, SoftSha256, store_config())
            .unwrap_or_else(|(e, ..)| panic!("format: {e:?}"));
        store.put("/hardware.json", b"{}").unwrap();
        store.into_flash().into_bytes()
    }

    #[test]
    fn a_formatted_store_is_a_tree_and_erased_flash_is_nothing() {
        let region = store_region(16);
        assert_eq!(fs_in_region(&region), Some(FsKind::Tree));
        assert!(!tree_sectors(&region).is_empty());
        assert_eq!(fs_in_region(&vec![0xFFu8; 16 * 4096]), None);
        assert_eq!(fs_in_region(&vec![0u8; 16 * 4096]), None, "killed headers");
    }

    #[test]
    fn a_newer_store_header_is_still_a_store() {
        let mut region = store_region(16);
        let at = tree_sectors(&region)[0] as usize * 4096;
        // Version 4: rule 0 refuses before the CRC.
        region[at + 4..at + 6].copy_from_slice(&4u16.to_le_bytes());
        assert!(tree_header_present(&region[at..]));
    }

    #[test]
    fn a_torn_header_without_its_crc_is_not_a_store() {
        let mut region = store_region(16);
        let at = tree_sectors(&region)[0] as usize * 4096;
        region[at + 20] ^= 0xFF;
        assert!(!tree_header_present(&region[at..]));
    }

    #[test]
    fn an_image_without_a_manifest_mounts_littlefs() {
        assert_eq!(image_fs_kind(&[0x5A; 4096]), FsKind::Littlefs);
    }

    #[test]
    fn an_image_whose_manifest_lists_fs_tree_mounts_the_tree_store() {
        let blob = |features: &str| {
            let mut image = vec![0x11u8; 1024];
            image.extend_from_slice(lpc_model::manifest::MANIFEST_BLOB_BEGIN.as_bytes());
            image.extend_from_slice(
                format!(r#"{{"lpManifestCore":1,"features":[{features}]}}"#).as_bytes(),
            );
            image.extend_from_slice(lpc_model::manifest::MANIFEST_BLOB_END.as_bytes());
            image.extend_from_slice(&[0xFF; 512]);
            image
        };
        assert_eq!(image_fs_kind(&blob(r#""server","fs.tree""#)), FsKind::Tree);
        assert_eq!(image_fs_kind(&blob(r#""server""#)), FsKind::Littlefs);
    }
}
