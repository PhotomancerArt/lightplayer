//! Mount a raw `lpfs` image in memory and walk it into files.
//!
//! The image arrives as the partition's bytes verbatim from
//! `LinkManagementRequest::ReadRawFilesystem`. Mounting it here — in Rust,
//! compiled to wasm — is what makes the backup possible at all: the device
//! this came off may not boot, so nothing on the device can be asked to list
//! its own files.
//!
//! The geometry must match the firmware's (`fw-esp32c6/src/flash_storage.rs`:
//! 4 KB blocks, 512 B cache, 64 B lookahead) or the mount fails or misreads.
//! Block COUNT is derived from the image length rather than pinned, because
//! it differs per board — 240 blocks on the C6, 384 on the S3.

use littlefs_rust::{Config, Error as LfsError, FileType, Filesystem, Storage};

/// littlefs geometry, matching `lpfs_config()` in the firmware.
const BLOCK_SIZE: u32 = 4096;
const CACHE_SIZE: u32 = 512;
const LOOKAHEAD_SIZE: u32 = 64;

/// How deep the walk will follow directories before giving up.
///
/// A bound, not a limit anyone should reach: real device storage is
/// `/projects/<name>/…` and `/.lp/…`. It exists because the input is a raw
/// image off a possibly-damaged board, and a corrupted directory entry that
/// points at itself must not hang the browser tab.
const MAX_DEPTH: usize = 16;

/// One file recovered from the image: absolute device path, and its bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// Mount `image` and read every file in it, sorted by path.
///
/// Sorted because the archive should be byte-reproducible for the same
/// device state: littlefs directory order is an implementation detail, and a
/// backup that reshuffles itself between captures is hard to diff.
pub fn read_image_files(image: &[u8]) -> Result<Vec<BackupFile>, LfsError> {
    let block_count = (image.len() / BLOCK_SIZE as usize) as u32;
    let mut config = Config::new(BLOCK_SIZE, block_count);
    config.cache_size = CACHE_SIZE;
    config.lookahead_size = LOOKAHEAD_SIZE;

    let storage = ImageStorage {
        data: image.to_vec(),
        block_size: BLOCK_SIZE,
    };
    let fs = Filesystem::mount(storage, config).map_err(|(error, _)| error)?;

    let mut files = Vec::new();
    walk_dir(&fs, "/", 0, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// A read-only [`Storage`] over an image already in memory.
///
/// Writes and erases are rejected rather than silently accepted: this is a
/// BACKUP path, and a mount that quietly repaired the image would be
/// modifying the only copy of a user's damaged filesystem.
struct ImageStorage {
    data: Vec<u8>,
    block_size: u32,
}

impl Storage for ImageStorage {
    fn read(&mut self, block: u32, offset: u32, buf: &mut [u8]) -> Result<(), LfsError> {
        let start = (block as usize)
            .checked_mul(self.block_size as usize)
            .and_then(|base| base.checked_add(offset as usize))
            .ok_or(LfsError::Io)?;
        let end = start.checked_add(buf.len()).ok_or(LfsError::Io)?;
        if end > self.data.len() {
            return Err(LfsError::Io);
        }
        buf.copy_from_slice(&self.data[start..end]);
        Ok(())
    }

    fn write(&mut self, _block: u32, _offset: u32, _data: &[u8]) -> Result<(), LfsError> {
        Err(LfsError::Io)
    }

    fn erase(&mut self, _block: u32) -> Result<(), LfsError> {
        Err(LfsError::Io)
    }
}

/// Depth-first walk collecting regular files. Unreadable entries abort the
/// walk: a backup that silently omits a file is worse than one that refuses.
fn walk_dir(
    fs: &Filesystem<ImageStorage>,
    dir: &str,
    depth: usize,
    out: &mut Vec<BackupFile>,
) -> Result<(), LfsError> {
    if depth >= MAX_DEPTH {
        return Ok(());
    }
    for entry in fs.list_dir(dir)? {
        if entry.name == "." || entry.name == ".." {
            continue;
        }
        let path = join_path(dir, &entry.name);
        if entry.file_type == FileType::Dir {
            walk_dir(fs, &path, depth + 1, out)?;
        } else {
            let bytes = fs.read_to_vec(&path)?;
            out.push(BackupFile { path, bytes });
        }
    }
    Ok(())
}

fn join_path(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}
