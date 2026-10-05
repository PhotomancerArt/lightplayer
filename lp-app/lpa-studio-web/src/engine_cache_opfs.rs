//! The browser's [`EngineCache`]: engines in OPFS at
//! `firmware-cache/engines/<sha256>.bin` (beside `device-backups/` and the
//! worker-owned `emu-flash/`), with the index at `firmware-cache/index.json`
//! (`"format": 1`, `lpa_firmware_store::EngineCacheIndex`).
//!
//! `put` verifies the bytes against the entry's key, writes them, READS
//! THEM BACK and compares before it records the entry, then evicts to the
//! bound (least recently used first, never `held`, never the one just put):
//! a victim whose file cannot be removed is logged and kept in the index.
//! `get` re-hashes what it reads; a corrupt file is removed with its row and
//! answered `Missing`. Anything storage refuses is `Unavailable`. The index
//! is cache-like: unreadable reads as empty, and an engine file is never
//! deleted because the index could not be read.
//!
//! Time is the caller's: `put` takes it from the entry and `get` from its
//! argument; nothing here reads a clock. The OPFS calls are `lpa-fs-opfs`'s
//! (whole-file atomic writes through a JS-owned copy — never a view over
//! wasm memory).

use lpa_firmware_store::{
    ENGINE_CACHE_DEFAULT_BOUND_BYTES, EngineCache, EngineCacheEntry, EngineCacheError,
    EngineCacheIndex, LocalBoxFuture, verify_engine_bytes,
};
use lpa_fs_opfs::{OpfsError, open_dir, opfs_root, read_file, remove_path, write_file};
use lpc_firmware_release::{SHA256_HEX_LEN, is_lower_hex, sha256_hex};
use lpfs::LpPathBuf;

/// The cache's directory at the OPFS root.
const CACHE_DIR: &str = "firmware-cache";

/// The index's file name inside [`CACHE_DIR`].
const INDEX_FILE: &str = "index.json";

/// Where the engines live inside [`CACHE_DIR`].
const ENGINES_DIR: &str = "engines";

/// OPFS-backed engine cache (stateless apart from its bound: every call
/// opens the root).
#[derive(Clone, Copy)]
pub struct OpfsEngineCache {
    bound_bytes: u64,
}

impl Default for OpfsEngineCache {
    fn default() -> Self {
        Self {
            bound_bytes: ENGINE_CACHE_DEFAULT_BOUND_BYTES,
        }
    }
}

impl EngineCache for OpfsEngineCache {
    fn put(
        &self,
        entry: EngineCacheEntry,
        bytes: Vec<u8>,
    ) -> LocalBoxFuture<'_, Result<(), EngineCacheError>> {
        let bound_bytes = self.bound_bytes;
        Box::pin(async move {
            verify_engine_bytes(&entry, &bytes)?;
            let path = engine_path(&entry.sha256)?;
            let dir = cache_dir().await.map_err(unavailable)?;
            write_file(&dir, &path, &bytes).await.map_err(unavailable)?;
            let back = read_file(&dir, path.as_str()).await.map_err(unavailable)?;
            if back.as_deref() != Some(bytes.as_slice()) {
                return Err(EngineCacheError::Unavailable(
                    "the stored engine did not read back equal".to_string(),
                ));
            }
            let just_put = entry.sha256.clone();
            let mut index = read_index(&dir).await;
            index.upsert(entry);
            for victim in index.eviction_victims(bound_bytes, &just_put) {
                let Ok(victim_path) = engine_path(&victim) else {
                    index.remove(&victim);
                    continue;
                };
                match remove_path(&dir, &victim_path).await {
                    Ok(()) => {
                        index.remove(&victim);
                    }
                    Err(error) => {
                        log::warn!("engine cache: could not evict {victim}: {error}");
                    }
                }
            }
            write_index(&dir, &index).await
        })
    }

    fn get(&self, sha256: &str, now: f64) -> LocalBoxFuture<'_, Result<Vec<u8>, EngineCacheError>> {
        let sha256 = sha256.to_string();
        Box::pin(async move {
            let path = engine_path(&sha256)?;
            let dir = cache_dir().await.map_err(unavailable)?;
            let Some(bytes) = read_file(&dir, path.as_str()).await.map_err(unavailable)? else {
                return Err(EngineCacheError::Missing(sha256));
            };
            let mut index = read_index(&dir).await;
            if sha256_hex(&bytes) != sha256 {
                log::warn!("engine cache: {sha256} was corrupt; removed");
                if let Err(error) = remove_path(&dir, &path).await {
                    log::warn!("engine cache: could not remove corrupt {sha256}: {error}");
                }
                if index.remove(&sha256) {
                    write_index(&dir, &index).await?;
                }
                return Err(EngineCacheError::Missing(sha256));
            }
            if index.touch(&sha256, now) {
                // A failed touch only costs LRU accuracy, never the bytes.
                if let Err(error) = write_index(&dir, &index).await {
                    log::debug!("engine cache: touch not recorded: {error}");
                }
            }
            Ok(bytes)
        })
    }

    fn has(&self, sha256: &str) -> LocalBoxFuture<'_, bool> {
        let sha256 = sha256.to_string();
        Box::pin(async move {
            match cache_dir().await {
                Ok(dir) => read_index(&dir).await.get(&sha256).is_some(),
                Err(_) => false,
            }
        })
    }

    fn index(&self) -> LocalBoxFuture<'_, EngineCacheIndex> {
        Box::pin(async move {
            match cache_dir().await {
                Ok(dir) => read_index(&dir).await,
                Err(error) => {
                    log::debug!("engine cache: no store ({error}); index empty");
                    EngineCacheIndex::default()
                }
            }
        })
    }

    fn set_held(
        &self,
        sha256: &str,
        held: bool,
    ) -> LocalBoxFuture<'_, Result<(), EngineCacheError>> {
        let sha256 = sha256.to_string();
        Box::pin(async move {
            let dir = cache_dir().await.map_err(unavailable)?;
            let mut index = read_index(&dir).await;
            if !index.set_held(&sha256, held) {
                return Err(EngineCacheError::Missing(sha256));
            }
            write_index(&dir, &index).await
        })
    }
}

async fn cache_dir() -> Result<web_sys::FileSystemDirectoryHandle, OpfsError> {
    let root = opfs_root().await?;
    open_dir(&root, CACHE_DIR, true).await
}

async fn read_index(dir: &web_sys::FileSystemDirectoryHandle) -> EngineCacheIndex {
    match read_file(dir, &format!("/{INDEX_FILE}")).await {
        Ok(Some(bytes)) => EngineCacheIndex::parse(&bytes),
        Ok(None) => EngineCacheIndex::default(),
        Err(error) => {
            log::debug!("engine cache: index unreadable ({error}); treated as empty");
            EngineCacheIndex::default()
        }
    }
}

async fn write_index(
    dir: &web_sys::FileSystemDirectoryHandle,
    index: &EngineCacheIndex,
) -> Result<(), EngineCacheError> {
    let path = LpPathBuf::from(format!("/{INDEX_FILE}").as_str());
    write_file(dir, &path, &index.to_bytes())
        .await
        .map_err(unavailable)
}

/// An engine's path inside the cache. Only a 64-hex key names a file, so a
/// key can never leave the directory or collide with the index.
fn engine_path(sha256: &str) -> Result<LpPathBuf, EngineCacheError> {
    if !is_lower_hex(sha256, SHA256_HEX_LEN) {
        return Err(EngineCacheError::Missing(sha256.to_string()));
    }
    Ok(LpPathBuf::from(
        format!("/{ENGINES_DIR}/{sha256}.bin").as_str(),
    ))
}

fn unavailable(error: OpfsError) -> EngineCacheError {
    EngineCacheError::Unavailable(error.to_string())
}
