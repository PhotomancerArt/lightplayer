//! Is a tree store here? The layout probe's cheap, read-only answer.
//!
//! The tree store (`lp-base/lp-tree-store`, the filesystem an `fs-tree`
//! core writes) begins every sector it uses with a 24-byte header whose
//! first four bytes are the magic `"LTS1"` — in every format version, past
//! and future (its FORMAT.md, "Sector"). Its free sectors are erased, and it
//! levels its wear, so its headers can be anywhere in the partition: the
//! probe looks at the start of every sector it read. Only the magic is
//! looked at, never a CRC or a version: a newer store may have moved them,
//! and it must be recognised all the same.
//!
//! What it decides (ADR `2026-10-10-fs-tree-refused-state-and-update-guard`,
//! amended 2026-10-16): a board holding one is never written a littlefs
//! filesystem, and is flashed only with firmware that reads it.

/// The tree store's sector magic. A test pins it to `lp_tree_store`'s own.
pub const TREE_STORE_SECTOR_MAGIC: [u8; 4] = *b"LTS1";

/// The tree store's sector: one 4 KB flash sector.
const SECTOR: usize = 4096;

/// Does any 4 KB sector of `region` (bytes read from a sector boundary)
/// begin with [`TREE_STORE_SECTOR_MAGIC`]?
pub fn holds_tree_store_sector(region: &[u8]) -> bool {
    region
        .chunks(SECTOR)
        .any(|sector| sector.starts_with(&TREE_STORE_SECTOR_MAGIC))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn the_magic_is_the_stores() {
        assert_eq!(
            TREE_STORE_SECTOR_MAGIC,
            lp_tree_store::SECTOR_MAGIC.to_le_bytes()
        );
    }

    #[test]
    fn a_real_store_is_recognised_and_blank_flash_is_not() {
        let store = tree_store_image(16);
        assert!(holds_tree_store_sector(&store));
        let mut moved = store.clone();
        moved.rotate_right(9 * SECTOR);
        assert!(!holds_tree_store_sector(&moved[..2 * SECTOR]));
        assert!(holds_tree_store_sector(&moved));
        assert!(!holds_tree_store_sector(&[0xFF; 16 * SECTOR]));
        assert!(!holds_tree_store_sector(&[0x00; 16 * SECTOR]));
    }

    /// Only a sector's first bytes count: the magic inside a sector (a
    /// file's bytes) is not a header.
    #[test]
    fn the_magic_off_a_sector_boundary_is_not_a_header() {
        let mut flash = vec![0xFF; 4 * SECTOR];
        flash[100..104].copy_from_slice(&TREE_STORE_SECTOR_MAGIC);
        flash[SECTOR + 1..SECTOR + 5].copy_from_slice(&TREE_STORE_SECTOR_MAGIC);
        assert!(!holds_tree_store_sector(&flash));
        flash[3 * SECTOR..3 * SECTOR + 4].copy_from_slice(&TREE_STORE_SECTOR_MAGIC);
        assert!(holds_tree_store_sector(&flash));
    }

    /// A `sectors`-sector store holding a file, as `lp-tree-store` writes it.
    pub(crate) fn tree_store_image(sectors: u32) -> Vec<u8> {
        tree_store_image_with(sectors, &[("/hardware.json", b"{\"board\":\"kept\"}")])
    }

    /// The same, holding `files`.
    pub(crate) fn tree_store_image_with(sectors: u32, files: &[(&str, &[u8])]) -> Vec<u8> {
        use lp_nor_sim::{NorFlashSim, NorGeometry};
        use lp_tree_store::{SoftSha256, StoreConfig, TreeStore};
        let mut st = match TreeStore::format(
            NorFlashSim::new(NorGeometry::c6(sectors)),
            SoftSha256,
            StoreConfig::default(),
        ) {
            Ok(st) => st,
            Err((e, ..)) => panic!("format: {e:?}"),
        };
        for (path, bytes) in files {
            st.put(path, bytes).expect("put");
        }
        let mut out = vec![0u8; sectors as usize * SECTOR];
        st.flash().peek(0, &mut out);
        out
    }
}
