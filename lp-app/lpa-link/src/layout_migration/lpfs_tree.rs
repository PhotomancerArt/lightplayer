//! Every file and directory of a LightPlayer filesystem image.
//!
//! The migration carries the **whole tree** — projects, `/hardware.json`,
//! `/.lp/device.json`, `/.lp/access.json`, empty directories — so there is
//! no list of "important paths" to drift out of date. Paths are absolute
//! device paths (`/projects/basic/project.json`), sorted, directories before
//! their contents.

use littlefs_rust::{Filesystem, RamStorage};

use super::lpfs_geometry::{LPFS_BLOCK_SIZE, LpfsGeometry};

/// One entry of a filesystem tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LpfsNode {
    Dir,
    File(Vec<u8>),
}

/// A filesystem's whole content.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LpfsTree {
    /// `(absolute path, node)`, sorted by path. The root is not listed.
    pub entries: Vec<(String, LpfsNode)>,
}

/// Why an image could not be read as a tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LpfsTreeError {
    /// The image is not the region's size.
    WrongSize { expected: u32, actual: u32 },
    /// littlefs would not mount it (named by littlefs's own error).
    Unmountable(String),
    /// A directory or file inside would not read.
    Unreadable { path: String, error: String },
}

impl core::fmt::Display for LpfsTreeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WrongSize { expected, actual } => {
                write!(f, "filesystem image is {actual} bytes, expected {expected}")
            }
            Self::Unmountable(error) => write!(f, "filesystem does not mount: {error}"),
            Self::Unreadable { path, error } => write!(f, "cannot read {path}: {error}"),
        }
    }
}

impl LpfsTree {
    /// Mount `image` (the raw bytes of a region of `geometry`) and collect
    /// every entry. Also returns the blocks littlefs reports in use.
    ///
    /// Mounts a **copy**: the caller's bytes are never written.
    pub fn from_image(image: &[u8], geometry: LpfsGeometry) -> Result<(Self, u32), LpfsTreeError> {
        if image.len() != geometry.len() as usize {
            return Err(LpfsTreeError::WrongSize {
                expected: geometry.len(),
                actual: image.len() as u32,
            });
        }
        let storage = storage_from_bytes(image, geometry.block_count);
        let fs = Filesystem::mount(storage, geometry.config())
            .map_err(|(error, _)| LpfsTreeError::Unmountable(error.to_string()))?;
        let used = fs
            .fs_size()
            .map_err(|error| LpfsTreeError::Unmountable(error.to_string()))?;
        let mut tree = Self::default();
        walk(&fs, "", &mut tree)?;
        tree.sort();
        Ok((tree, used))
    }

    /// A tree from `(path, bytes)` files; parent directories are implied and
    /// listed. For fixtures, tests and the `--dir` measurement.
    pub fn from_files(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        let mut tree = Self::default();
        for (path, bytes) in files {
            let path = normalize(&path);
            let mut prefix = String::new();
            let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
            for part in &parts[..parts.len().saturating_sub(1)] {
                prefix.push('/');
                prefix.push_str(part);
                if !tree.entries.iter().any(|(p, _)| *p == prefix) {
                    tree.entries.push((prefix.clone(), LpfsNode::Dir));
                }
            }
            tree.entries.push((path, LpfsNode::File(bytes)));
        }
        tree.sort();
        tree
    }

    /// Add an (empty) directory.
    pub fn add_dir(&mut self, path: &str) {
        let path = normalize(path);
        if !self.entries.iter().any(|(p, _)| *p == path) {
            self.entries.push((path, LpfsNode::Dir));
            self.sort();
        }
    }

    /// Number of files (directories not counted).
    pub fn file_count(&self) -> u32 {
        self.files().count() as u32
    }

    /// Sum of every file's length.
    pub fn total_bytes(&self) -> u64 {
        self.files().map(|(_, bytes)| bytes.len() as u64).sum()
    }

    /// Every file, in path order.
    pub fn files(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.entries.iter().filter_map(|(path, node)| match node {
            LpfsNode::File(bytes) => Some((path.as_str(), bytes.as_slice())),
            LpfsNode::Dir => None,
        })
    }

    /// The bytes of the file at `path`.
    pub fn file(&self, path: &str) -> Option<&[u8]> {
        self.files().find(|(p, _)| *p == path).map(|(_, b)| b)
    }

    /// The stamped device uid (`/.lp/device.json`'s `uid`), when the board
    /// has one — the identity a migration must come back with.
    pub fn device_uid(&self) -> Option<String> {
        let bytes = self.file("/.lp/device.json")?;
        let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        value.get("uid")?.as_str().map(str::to_string)
    }

    fn sort(&mut self) {
        self.entries.sort_by(|a, b| a.0.cmp(&b.0));
    }
}

/// A littlefs `RamStorage` holding a copy of `image`.
pub(crate) fn storage_from_bytes(image: &[u8], block_count: u32) -> RamStorage {
    use littlefs_rust::Storage;
    let mut storage = RamStorage::new(LPFS_BLOCK_SIZE, block_count);
    for (block, chunk) in image.chunks(LPFS_BLOCK_SIZE as usize).enumerate() {
        // An erased block is already 0xff; skipping it keeps this cheap.
        if chunk.iter().any(|b| *b != 0xFF) {
            storage
                .write(block as u32, 0, chunk)
                .expect("a RamStorage write inside its own geometry");
        }
    }
    storage
}

fn walk(fs: &Filesystem<RamStorage>, dir: &str, tree: &mut LpfsTree) -> Result<(), LpfsTreeError> {
    let unreadable = |path: &str, error: littlefs_rust::Error| LpfsTreeError::Unreadable {
        path: format!("/{path}"),
        error: error.to_string(),
    };
    // littlefs names the root "/", not "".
    let listed = if dir.is_empty() { "/" } else { dir };
    for entry in fs.list_dir(listed).map_err(|e| unreadable(dir, e))? {
        let path = if dir.is_empty() {
            entry.name.clone()
        } else {
            format!("{dir}/{}", entry.name)
        };
        match entry.file_type {
            littlefs_rust::FileType::Dir => {
                tree.entries.push((format!("/{path}"), LpfsNode::Dir));
                walk(fs, &path, tree)?;
            }
            littlefs_rust::FileType::File => {
                let bytes = fs.read_to_vec(&path).map_err(|e| unreadable(&path, e))?;
                tree.entries
                    .push((format!("/{path}"), LpfsNode::File(bytes)));
            }
        }
    }
    Ok(())
}

fn normalize(path: &str) -> String {
    format!("/{}", path.trim_start_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_migration::lpfs_repack::write_tree_image;

    fn geometry(blocks: u32) -> LpfsGeometry {
        LpfsGeometry {
            offset: 0,
            block_count: blocks,
        }
    }

    #[test]
    fn a_tree_survives_an_image_round_trip_including_empty_dirs_and_empty_files() {
        let mut tree = LpfsTree::from_files([
            ("/projects/basic/project.json".to_string(), b"{}".to_vec()),
            ("/hardware.json".to_string(), vec![1u8; 9000]),
            (
                "/.lp/device.json".to_string(),
                br#"{"uid":"dev01"}"#.to_vec(),
            ),
            ("/empty.txt".to_string(), Vec::new()),
        ]);
        tree.add_dir("/projects/empty");
        let image = write_tree_image(&tree, geometry(64)).unwrap();
        let (back, used) = LpfsTree::from_image(&image, geometry(64)).unwrap();
        assert_eq!(back, tree);
        assert!(used > 0);
        assert_eq!(back.device_uid().as_deref(), Some("dev01"));
        assert_eq!(back.file_count(), 4);
        assert_eq!(back.total_bytes(), 2 + 9000 + 15);
    }

    #[test]
    fn erased_flash_is_unmountable_and_the_wrong_size_is_refused() {
        let blank = vec![0xFFu8; 64 * 4096];
        assert!(matches!(
            LpfsTree::from_image(&blank, geometry(64)),
            Err(LpfsTreeError::Unmountable(_))
        ));
        assert!(matches!(
            LpfsTree::from_image(&blank[..4096], geometry(64)),
            Err(LpfsTreeError::WrongSize { .. })
        ));
    }
}
