//! A real littlefs image of the scripted device's storage.
//!
//! The fake provider's raw-filesystem read hands back an image the *actual*
//! parser can mount, rather than a placeholder blob. That is what makes the
//! studio-side backup flow testable without hardware: a fake that returned
//! arbitrary bytes would exercise dispatch and nothing else, and the failure
//! it would hide — an image that does not mount — is the whole operation.
//!
//! The geometry mirrors `fw-esp32c6/src/flash_storage.rs` (4 KB blocks, 512 B
//! cache, 64 B lookahead) at the C6's `lpfs` size, because the scripted device
//! presents itself as a C6.

use littlefs_rust::{Config, Filesystem, RamStorage};

use crate::LinkFlashRegion;

/// littlefs geometry, matching the firmware's `lpfs_config()`.
const BLOCK_SIZE: u32 = 4096;
const CACHE_SIZE: u32 = 512;
const LOOKAHEAD_SIZE: u32 = 64;

/// Format a fresh image at `region`'s geometry and write `files` into it.
///
/// Paths are absolute device paths (`/projects/studio/project.json`); parent
/// directories are created as needed. Returns the raw partition bytes.
pub(crate) fn build_image(region: LinkFlashRegion, files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let block_count = region.block_count(BLOCK_SIZE);
    let mut storage = RamStorage::new(BLOCK_SIZE, block_count);
    let config = image_config(block_count);
    Filesystem::format(&mut storage, &config).expect("format the fake lpfs image");
    let fs = Filesystem::mount(storage, config)
        .map_err(|(error, _)| error)
        .expect("mount the fake lpfs image");
    for (path, bytes) in files {
        create_parents(&fs, path);
        fs.write_file(path, bytes)
            .unwrap_or_else(|error| panic!("seed {path} into the fake lpfs image: {error:?}"));
    }
    let storage = fs.unmount().expect("unmount the fake lpfs image");
    storage.data().to_vec()
}

fn image_config(block_count: u32) -> Config {
    let mut config = Config::new(BLOCK_SIZE, block_count);
    config.cache_size = CACHE_SIZE;
    config.lookahead_size = LOOKAHEAD_SIZE;
    config
}

/// `mkdir -p` for the file's parents. littlefs has no recursive create, and
/// an existing directory is not an error worth distinguishing here.
fn create_parents(fs: &Filesystem<RamStorage>, path: &str) {
    let mut prefix = String::new();
    let mut segments = path.trim_start_matches('/').split('/').peekable();
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            break;
        }
        prefix.push('/');
        prefix.push_str(segment);
        let _ = fs.mkdir(&prefix);
    }
}
