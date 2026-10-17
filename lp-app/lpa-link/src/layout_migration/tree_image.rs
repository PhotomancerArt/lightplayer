//! A board's files packed into a fresh tree store, and a tree store's files
//! read back — both over flash held in memory, on the host.
//!
//! The pack is file-level, as the littlefs re-pack is
//! ([`super::lpfs_repack`]): format a store the size of the target
//! partition, write every file into ONE transaction (so the image holds one
//! root and no intermediate ones), deflated on the host exactly as a push
//! sends it (`lp_tree_store::host_deflate_chunks` into
//! `put_chunk_deflated`), commit, mount the result again and read every file
//! back against the source bytes. Only an image that passed that is ever
//! flashed. It refuses — nothing written — when the files do not fit, or
//! would leave fewer than [`FREE_BLOCK_FLOOR`] sectors free.
//!
//! The reader mounts with `TreeStore::mount`, the firmware's own mount, over
//! a **copy** of the bytes.
//!
//! **Directories are implicit in the tree store** (FORMAT.md "Files and
//! paths"): a directory exists while a file is under it. An empty littlefs
//! directory is not carried; every file is.

use lp_tree_store::{
    Flash, MountSummary, SoftSha256, StoreConfig, StoreError, StoreImage, TreeStore,
    host_deflate_chunks, is_snapshot_path, valid_path,
};

use super::fs_kind::TREE_SECTOR_SIZE;
use super::lpfs_geometry::LpfsGeometry;
use super::lpfs_repack::FREE_BLOCK_FLOOR;
use super::lpfs_tree::{LpfsTree, LpfsTreeError};
use super::migration_plan::Refusal;

/// The store's dials for a pack: the firmware's (`StoreConfig::default()`,
/// `lp-fw/fw-esp32c6/src/main.rs`) with one RAM-only difference — the
/// transaction's directory delta is never spilled to flash mid-pack, so the
/// image carries no pending directories that the first GC would throw away.
/// `record_max`, the one dial that shapes what is written, is the firmware's.
pub fn store_config() -> StoreConfig {
    StoreConfig {
        txn_delta_max: u32::MAX,
        ..StoreConfig::default()
    }
}

/// The most deflated chunks one file is written as (see [`pack_tree`]); a
/// longer file is written whole. 64 chunks is at least 64 KB of logical
/// bytes and at most ~16 KB of spine rewrites.
pub const MAX_DEFLATED_CHUNKS: usize = 64;

/// NOR flash in memory: program clears bits, erase sets a sector to `0xFF`.
#[derive(Clone, Debug)]
pub struct RamNor {
    bytes: Vec<u8>,
}

/// An access outside the flash.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutOfRange;

impl RamNor {
    /// `sectors` erased 4 KB sectors.
    pub fn erased(sectors: u32) -> Self {
        Self {
            bytes: vec![0xFF; (sectors * TREE_SECTOR_SIZE) as usize],
        }
    }

    /// A flash holding a copy of `bytes` (a whole number of sectors).
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    fn span(&self, addr: u32, len: usize) -> Result<core::ops::Range<usize>, OutOfRange> {
        let start = addr as usize;
        let end = start.checked_add(len).ok_or(OutOfRange)?;
        (end <= self.bytes.len())
            .then_some(start..end)
            .ok_or(OutOfRange)
    }
}

impl Flash for RamNor {
    type Error = OutOfRange;

    fn sector_count(&self) -> u32 {
        self.bytes.len() as u32 / TREE_SECTOR_SIZE
    }

    fn sector_size(&self) -> u32 {
        TREE_SECTOR_SIZE
    }

    fn read(&mut self, addr: u32, buf: &mut [u8]) -> Result<(), OutOfRange> {
        let span = self.span(addr, buf.len())?;
        buf.copy_from_slice(&self.bytes[span]);
        Ok(())
    }

    fn program(&mut self, addr: u32, data: &[u8]) -> Result<(), OutOfRange> {
        let span = self.span(addr, data.len())?;
        for (cell, bit) in self.bytes[span].iter_mut().zip(data) {
            *cell &= *bit;
        }
        Ok(())
    }

    fn erase_sector(&mut self, sector: u32) -> Result<(), OutOfRange> {
        let span = self.span(sector * TREE_SECTOR_SIZE, TREE_SECTOR_SIZE as usize)?;
        self.bytes[span].fill(0xFF);
        Ok(())
    }
}

/// A verified tree-store image ready to write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackedTree {
    /// The whole region, `geometry.len()` bytes.
    pub image: Vec<u8>,
    /// Sectors that hold anything (not all `0xFF`).
    pub sectors_used: u32,
    /// `block_count - sectors_used`.
    pub sectors_free: u32,
    /// Fewer than a quarter of the sectors are free.
    pub tight: bool,
    /// The sector holding the committed root — the one a writer puts on
    /// flash last.
    pub root_sector: u32,
}

impl PackedTree {
    /// The sectors that hold anything, ascending.
    pub fn used_sectors(&self) -> Vec<u32> {
        used_sectors(&self.image)
    }

    /// Sector `index`'s bytes.
    pub fn sector(&self, index: u32) -> &[u8] {
        let at = (index * TREE_SECTOR_SIZE) as usize;
        &self.image[at..at + TREE_SECTOR_SIZE as usize]
    }
}

/// The sectors of `image` that are not all `0xFF`, ascending.
pub fn used_sectors(image: &[u8]) -> Vec<u32> {
    image
        .chunks(TREE_SECTOR_SIZE as usize)
        .enumerate()
        .filter(|(_, sector)| sector.iter().any(|b| *b != 0xFF))
        .map(|(index, _)| index as u32)
        .collect()
}

/// Pack every file of `tree` into a fresh tree store of `geometry`, verify
/// it, and measure it — or refuse with nothing written.
pub fn pack_tree(tree: &LpfsTree, geometry: LpfsGeometry) -> Result<PackedTree, Refusal> {
    // The reserved snapshot namespace is the snapshot feature's alone
    // (FORMAT.md "The snapshot namespace"; `LpFsTree` refuses writes there
    // too): a littlefs file under it is refused by name, never carried.
    if let Some((path, _)) = tree
        .files()
        .find(|(path, _)| !valid_path(path) || is_snapshot_path(path))
    {
        return Err(Refusal::UnstorablePath(path.to_string()));
    }
    let does_not_fit = || Refusal::DoesNotFit {
        files: tree.file_count(),
        bytes: tree.total_bytes(),
        blocks_available: geometry.block_count,
    };
    let failed = |what: &str, error: StoreError<OutOfRange>| match error {
        StoreError::NoSpace => does_not_fit(),
        other => Refusal::RepackFailed(format!("{what}: {other:?}")),
    };

    let mut store = TreeStore::format(
        RamNor::erased(geometry.block_count),
        SoftSha256,
        store_config(),
    )
    .map_err(|(error, ..)| failed("format", error))?;
    let record_max = store.config().record_max;
    store.begin().map_err(|e| failed("begin", e))?;
    for (path, bytes) in tree.files() {
        // A file and a folder never share a name (FORMAT.md "Files and
        // paths"). littlefs cannot hold both either, so this never fires on
        // a board's own files; a backup made elsewhere could.
        if store.name_clash(path).map_err(|e| failed(path, e))? {
            return Err(Refusal::UnstorablePath(path.to_string()));
        }
        if bytes.is_empty() {
            store.put(path, &[]).map_err(|e| failed(path, e))?;
            continue;
        }
        let chunks = host_deflate_chunks(&mut SoftSha256, bytes, record_max);
        // Each deflated chunk after the first is an append, which rewrites
        // the file's multi spine (8 bytes a chunk): garbage that grows with
        // the square of the chunk count. A file that deflate does not shrink
        // by a fifth, or that would take more than MAX_DEFLATED_CHUNKS, is
        // written whole instead — one spine, stored chunks.
        let deflated: usize = chunks.iter().map(|c| c.deflated.len()).sum();
        let worth_it = chunks.len() == 1
            || (chunks.len() <= MAX_DEFLATED_CHUNKS && deflated * 5 < bytes.len() * 4);
        if !worth_it {
            store.put(path, bytes).map_err(|e| failed(path, e))?;
            continue;
        }
        let mut offset = 0u32;
        for chunk in chunks {
            store
                .put_chunk_deflated(
                    path,
                    offset,
                    chunk.logical_len,
                    Some(chunk.id),
                    &chunk.deflated,
                )
                .map_err(|e| failed(path, e))?;
            offset += chunk.logical_len;
        }
    }
    store.commit().map_err(|e| failed("commit", e))?;
    let image = store.into_flash().into_bytes();

    // Verify: mount what was built and compare every file.
    let back = read_tree(&image, geometry)
        .map_err(|error| Refusal::RepackFailed(format!("the packed store: {error}")))?;
    if !same_files(&back, tree) {
        return Err(Refusal::RepackFailed(
            "the packed store does not read back as the source files".to_string(),
        ));
    }
    let root_sector = StoreImage::open(&image, Some(TREE_SECTOR_SIZE))
        .ok()
        .and_then(|inspected| inspected.report().chosen_root().map(|root| root.sector))
        .ok_or_else(|| Refusal::RepackFailed("the packed store names no root".to_string()))?;

    let sectors_used = used_sectors(&image).len() as u32;
    let sectors_free = geometry.block_count.saturating_sub(sectors_used);
    if sectors_free < FREE_BLOCK_FLOOR {
        return Err(Refusal::TooTight {
            files: tree.file_count(),
            bytes: tree.total_bytes(),
            blocks_used: sectors_used,
            blocks_available: geometry.block_count,
        });
    }
    Ok(PackedTree {
        image,
        sectors_used,
        sectors_free,
        tight: sectors_free * 4 < geometry.block_count,
        root_sector,
    })
}

/// Mount `image` (a region of `geometry` holding a tree store) with the
/// firmware's mount, over a copy, and collect every file.
pub fn read_tree(image: &[u8], geometry: LpfsGeometry) -> Result<LpfsTree, LpfsTreeError> {
    read_tree_summary(image, geometry).map(|(tree, _)| tree)
}

/// [`read_tree`], with the mount's summary.
pub fn read_tree_summary(
    image: &[u8],
    geometry: LpfsGeometry,
) -> Result<(LpfsTree, MountSummary), LpfsTreeError> {
    if image.len() != geometry.len() as usize {
        return Err(LpfsTreeError::WrongSize {
            expected: geometry.len(),
            actual: image.len() as u32,
        });
    }
    let mut store = TreeStore::mount(RamNor::from_bytes(image), SoftSha256, store_config())
        .map_err(|(error, ..)| LpfsTreeError::Unmountable(mount_words(&error).to_string()))?;
    let summary = store.summary();
    let unreadable = |path: &str, error: StoreError<OutOfRange>| LpfsTreeError::Unreadable {
        path: path.to_string(),
        error: format!("{error:?}"),
    };
    let paths = store.list("/").map_err(|e| unreadable("/", e))?;
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = store
            .get(&path)
            .map_err(|e| unreadable(&path, e))?
            .ok_or_else(|| LpfsTreeError::Unreadable {
                path: path.clone(),
                error: "listed but not found".to_string(),
            })?;
        files.push((path, bytes));
    }
    Ok((LpfsTree::from_files(files), summary))
}

/// Do `a` and `b` hold the same files, byte for byte (directories aside —
/// the tree store keeps none of its own)?
pub fn same_files(a: &LpfsTree, b: &LpfsTree) -> bool {
    a.files().eq(b.files())
}

/// What a mount that did not adopt a store says.
fn mount_words(error: &StoreError<OutOfRange>) -> &'static str {
    match error {
        StoreError::NoStore => "no tree store here",
        StoreError::Damaged(why) | StoreError::Unsupported(why) | StoreError::Corrupt(why) => why,
        StoreError::BadConfig(why) => why,
        _ => "the tree store did not mount",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_migration::lpfs_repack::tests::board_tree;

    const T2: LpfsGeometry = LpfsGeometry {
        offset: 0x38_0000,
        block_count: 128,
    };

    #[test]
    fn a_board_packs_into_128_sectors_with_every_file_equal() {
        let tree = board_tree();
        let packed = pack_tree(&tree, T2).unwrap();
        assert_eq!(packed.image.len(), 128 * 4096);
        let back = read_tree(&packed.image, T2).unwrap();
        assert!(same_files(&back, &tree));
        assert_eq!(back.file_count(), tree.file_count());
        assert_eq!(packed.sectors_used + packed.sectors_free, 128);
        assert!(packed.used_sectors().contains(&packed.root_sector));
        assert!(!packed.tight);
    }

    #[test]
    fn the_pack_is_deterministic() {
        let tree = board_tree();
        assert_eq!(
            pack_tree(&tree, T2).unwrap().image,
            pack_tree(&tree, T2).unwrap().image
        );
    }

    #[test]
    fn an_empty_directory_is_not_carried_and_an_empty_file_is() {
        let tree = board_tree();
        let back = read_tree(&pack_tree(&tree, T2).unwrap().image, T2).unwrap();
        assert_eq!(back.file("/projects/basic/empty.bin"), Some(&[][..]));
        assert!(
            !back
                .entries
                .iter()
                .any(|(path, _)| path == "/projects/basic/nothing-here")
        );
    }

    #[test]
    fn files_that_do_not_compress_and_do_not_fit_are_refused() {
        // ~600 KB of noise: does not fit 512 KB however it is deflated.
        let mut state = 0x1234_5678u32;
        let noise: Vec<u8> = (0..600 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        let tree = LpfsTree::from_files([("/projects/big/blob.bin".to_string(), noise)]);
        assert!(matches!(
            pack_tree(&tree, T2),
            Err(Refusal::DoesNotFit {
                blocks_available: 128,
                ..
            })
        ));
    }

    #[test]
    fn a_pack_leaving_under_the_floor_is_too_tight() {
        let mut state = 7u32;
        let mut noise = |len: usize| -> Vec<u8> {
            (0..len)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as u8
                })
                .collect()
        };
        // Grow until the pack crosses the floor; it must say TooTight there
        // before it says DoesNotFit.
        let mut saw_too_tight = false;
        for kb in (280..420).step_by(4) {
            let tree = LpfsTree::from_files([("/p/blob.bin".to_string(), noise(kb * 1024))]);
            let got = pack_tree(&tree, T2);
            match got {
                Err(Refusal::TooTight { blocks_used, .. }) => {
                    assert!(128 - blocks_used < FREE_BLOCK_FLOOR);
                    saw_too_tight = true;
                    break;
                }
                Err(Refusal::DoesNotFit { .. }) => break,
                _ => {}
            }
        }
        assert!(saw_too_tight);
    }

    #[test]
    fn a_path_the_store_cannot_hold_is_refused_by_name() {
        let deep = format!("/{}", vec!["d"; 40].join("/"));
        let tree = LpfsTree::from_files([(deep.clone(), b"x".to_vec())]);
        assert_eq!(pack_tree(&tree, T2), Err(Refusal::UnstorablePath(deep)));
    }

    #[test]
    fn a_file_in_the_snapshot_namespace_is_refused_by_name() {
        let tree = LpfsTree::from_files([
            ("/projects/a/project.json".to_string(), b"{}".to_vec()),
            ("/.lp/snapshots/a/one.json".to_string(), b"{}".to_vec()),
        ]);
        assert_eq!(
            pack_tree(&tree, T2),
            Err(Refusal::UnstorablePath(
                "/.lp/snapshots/a/one.json".to_string()
            ))
        );
    }

    #[test]
    fn a_file_named_like_a_folder_is_refused_by_name() {
        let tree = LpfsTree::from_files([
            ("/projects/a".to_string(), b"file".to_vec()),
            ("/projects/a/project.json".to_string(), b"{}".to_vec()),
        ]);
        assert!(matches!(
            pack_tree(&tree, T2),
            Err(Refusal::UnstorablePath(path)) if path.starts_with("/projects/a")
        ));
    }

    #[test]
    fn erased_flash_does_not_mount_as_a_store() {
        assert!(matches!(
            read_tree(&vec![0xFFu8; 128 * 4096], T2),
            Err(LpfsTreeError::Unmountable(_))
        ));
    }

    #[test]
    fn a_hot_file_round_trips() {
        let tree = LpfsTree::from_files([
            (
                "/projects/a/.lp/panel.json".to_string(),
                b"{\"k\":1}".to_vec(),
            ),
            ("/projects/a/project.json".to_string(), b"{}".to_vec()),
        ]);
        let back = read_tree(&pack_tree(&tree, T2).unwrap().image, T2).unwrap();
        assert!(same_files(&back, &tree));
    }
}
