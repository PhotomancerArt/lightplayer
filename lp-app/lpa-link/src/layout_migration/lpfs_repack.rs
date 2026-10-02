//! A tree re-packed into a fresh filesystem image at a target geometry, and
//! verified before anything is written to a board.
//!
//! A raw image cannot shrink — the old filesystem's blocks are wherever
//! littlefs put them across 960 KB — so the migration is file-level: format
//! a fresh image the size of the new partition, write every directory and
//! file into it, unmount, mount it again, and read every file back against
//! the source bytes. Only an image that passed that is ever flashed.

use littlefs_rust::{Filesystem, RamStorage};

use super::lpfs_geometry::{LPFS_BLOCK_SIZE, LpfsGeometry};
use super::lpfs_tree::{LpfsNode, LpfsTree, LpfsTreeError};
use super::migration_plan::Refusal;

/// Fewest free blocks a re-packed board may be left with (64 KB): littlefs
/// copies on write, and a Push clears then rewrites a whole project
/// directory, so a nearly full filesystem fails the next upload (plan Q2).
pub const FREE_BLOCK_FLOOR: u32 = 16;

/// A verified image ready to write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepackedImage {
    /// The whole region, `geometry.len()` bytes.
    pub image: Vec<u8>,
    /// Blocks littlefs reports in use after the re-pack.
    pub blocks_used: u32,
    /// `block_count - blocks_used`.
    pub blocks_free: u32,
    /// Fewer than a quarter of the blocks are free: allowed, but worth
    /// saying (MQ5 warns, does not refuse).
    pub tight: bool,
}

/// Re-pack `tree` into a fresh image of `geometry`, verify it, and measure
/// it — or refuse with nothing written: [`Refusal::DoesNotFit`] when littlefs
/// runs out of space, [`Refusal::TooTight`] when fewer than
/// [`FREE_BLOCK_FLOOR`] blocks would be left.
pub fn repack(tree: &LpfsTree, geometry: LpfsGeometry) -> Result<RepackedImage, Refusal> {
    let image = match write_tree_image(tree, geometry) {
        Ok(image) => image,
        Err(WriteError::NoSpace) => {
            return Err(Refusal::DoesNotFit {
                files: tree.file_count(),
                bytes: tree.total_bytes(),
                blocks_available: geometry.block_count,
            });
        }
        Err(WriteError::Other(error)) => return Err(Refusal::RepackFailed(error)),
    };

    // Verify: mount the image we built and compare every entry.
    let (back, blocks_used) = LpfsTree::from_image(&image, geometry)
        .map_err(|error: LpfsTreeError| Refusal::RepackFailed(error.to_string()))?;
    if back != *tree {
        return Err(Refusal::RepackFailed(
            "the re-packed filesystem does not read back as the source files".to_string(),
        ));
    }

    let blocks_free = geometry.block_count.saturating_sub(blocks_used);
    if blocks_free < FREE_BLOCK_FLOOR {
        return Err(Refusal::TooTight {
            files: tree.file_count(),
            bytes: tree.total_bytes(),
            blocks_used,
            blocks_available: geometry.block_count,
        });
    }
    Ok(RepackedImage {
        image,
        blocks_used,
        blocks_free,
        tight: blocks_free * 4 < geometry.block_count,
    })
}

/// Why writing a tree into an image failed.
#[derive(Debug)]
pub(crate) enum WriteError {
    NoSpace,
    Other(String),
}

/// Format a fresh image of `geometry` and write `tree` into it.
pub(crate) fn write_tree_image(
    tree: &LpfsTree,
    geometry: LpfsGeometry,
) -> Result<Vec<u8>, WriteError> {
    let other = |what: &str, path: &str, error: littlefs_rust::Error| {
        if error == littlefs_rust::Error::NoSpace {
            WriteError::NoSpace
        } else {
            WriteError::Other(format!("{what} {path}: {error}"))
        }
    };
    let mut storage = RamStorage::new(LPFS_BLOCK_SIZE, geometry.block_count);
    Filesystem::format(&mut storage, &geometry.config())
        .map_err(|error| other("format", "", error))?;
    let fs = Filesystem::mount(storage, geometry.config())
        .map_err(|(error, _)| other("mount", "", error))?;
    // Sorted by path, so every directory comes before what it holds.
    for (path, node) in &tree.entries {
        let lfs_path = path.trim_start_matches('/');
        match node {
            LpfsNode::Dir => match fs.mkdir(lfs_path) {
                Ok(()) | Err(littlefs_rust::Error::Exists) => {}
                Err(error) => return Err(other("mkdir", path, error)),
            },
            LpfsNode::File(bytes) => fs
                .write_file(lfs_path, bytes)
                .map_err(|error| other("write", path, error))?,
        }
    }
    let storage = fs.unmount().map_err(|error| other("unmount", "", error))?;
    Ok(storage.data().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_migration::legacy_layout::LEGACY_C6_V1_LPFS;

    const TARGET: LpfsGeometry = LpfsGeometry {
        offset: 0x35_0000,
        block_count: 176,
    };

    /// What a fielded C6 holds: projects, the board stamp, identity, access
    /// keys, an empty directory, a 0-byte file and a file over one block.
    pub(crate) fn board_tree() -> LpfsTree {
        let mut tree = LpfsTree::from_files([
            (
                "/projects/basic/project.json".to_string(),
                br#"{"name":"basic"}"#.to_vec(),
            ),
            (
                "/projects/basic/main.glsl".to_string(),
                b"void main() {}".to_vec(),
            ),
            ("/projects/basic/.lp/state.json".to_string(), b"{}".to_vec()),
            ("/hardware.json".to_string(), vec![b'h'; 1500]),
            (
                "/.lp/device.json".to_string(),
                br#"{"uid":"dev0000000000000042","name":"Porch"}"#.to_vec(),
            ),
            (
                "/.lp/access.json".to_string(),
                br#"{"version":2,"entries":[]}"#.to_vec(),
            ),
            ("/projects/basic/empty.bin".to_string(), Vec::new()),
            (
                "/projects/basic/big.bin".to_string(),
                (0..10_000u32).map(|i| i as u8).collect(),
            ),
        ]);
        tree.add_dir("/projects/basic/nothing-here");
        tree
    }

    #[test]
    fn a_legacy_board_re_packs_into_the_new_geometry_with_every_byte_equal() {
        let tree = board_tree();
        let legacy = write_tree_image(&tree, LEGACY_C6_V1_LPFS).unwrap();
        let (source, _) = LpfsTree::from_image(&legacy, LEGACY_C6_V1_LPFS).unwrap();
        let repacked = repack(&source, TARGET).unwrap();
        assert_eq!(repacked.image.len(), TARGET.len() as usize);
        let (back, used) = LpfsTree::from_image(&repacked.image, TARGET).unwrap();
        assert_eq!(back, tree);
        assert_eq!(used, repacked.blocks_used);
        assert_eq!(repacked.blocks_used + repacked.blocks_free, 176);
        assert!(!repacked.tight);
    }

    #[test]
    fn a_tree_that_fits_240_blocks_but_not_176_is_refused() {
        // ~190 files of one block each: littlefs needs a block per
        // non-inline file, so this fits 240 and not 176.
        let files = (0..190).map(|i| (format!("/projects/many/f{i:03}.bin"), vec![i as u8; 3000]));
        let tree = LpfsTree::from_files(files);
        assert!(write_tree_image(&tree, LEGACY_C6_V1_LPFS).is_ok());
        assert!(matches!(
            repack(&tree, TARGET),
            Err(Refusal::DoesNotFit {
                blocks_available: 176,
                ..
            })
        ));
    }

    #[test]
    fn a_tree_leaving_under_the_floor_is_too_tight_and_under_a_quarter_is_tight() {
        let make = |n: u32| {
            LpfsTree::from_files((0..n).map(|i| (format!("/p/f{i:03}.bin"), vec![1u8; 3000])))
        };
        // Find counts that land in each band rather than hardcoding
        // littlefs's metadata overhead.
        let mut saw_too_tight = false;
        let mut saw_tight = false;
        for n in (100..175).step_by(3) {
            match repack(&make(n), TARGET) {
                Err(Refusal::TooTight { blocks_used, .. }) => {
                    assert!(176 - blocks_used < FREE_BLOCK_FLOOR);
                    saw_too_tight = true;
                }
                Ok(image) if image.tight => {
                    assert!(image.blocks_free >= FREE_BLOCK_FLOOR);
                    assert!(image.blocks_free * 4 < 176);
                    saw_tight = true;
                }
                _ => {}
            }
        }
        assert!(saw_too_tight && saw_tight);
    }
}
