//! The browser's [`DeviceBackupStore`]: a board's backup lives in OPFS at
//! `device-backups/` (beside the worker-owned `emu-flash/`), with the index
//! at `device-backups/index.json` (C6 repartition, plan MQ4 / Q14).
//!
//! `put` writes the archive, then READS IT BACK and compares before it
//! records the entry — that read-back is what "the backup is stored" means
//! when the migration then writes the board. Anything that fails is
//! [`BackupStoreError::Unavailable`], which the card turns into "download
//! it first". The index is cache-like: unreadable reads as empty, and an
//! archive is never deleted because the index could not be read.
//!
//! The OPFS calls are `lpa-fs-opfs`'s (whole-file atomic writes through a
//! JS-owned copy — never a view over wasm memory).

use lpa_fs_opfs::{OpfsError, open_dir, opfs_root, read_file, remove_path, write_file};
use lpa_studio_core::app::library::LocalBoxFuture;
use lpa_studio_core::{
    BackupEntry, BackupIndex, BackupStatus, BackupStoreError, DeviceBackupStore,
};
use lpfs::LpPathBuf;

/// The store's directory at the OPFS root.
const BACKUPS_DIR: &str = "device-backups";

/// The index's file name inside [`BACKUPS_DIR`].
const INDEX_FILE: &str = "index.json";

/// OPFS-backed device backups (stateless: every call opens the root).
#[derive(Clone, Copy, Default)]
pub struct OpfsDeviceBackupStore;

impl DeviceBackupStore for OpfsDeviceBackupStore {
    fn put(
        &self,
        entry: BackupEntry,
        bytes: Vec<u8>,
    ) -> LocalBoxFuture<'_, Result<(), BackupStoreError>> {
        Box::pin(async move {
            let dir = backups_dir().await.map_err(unavailable)?;
            let path = file_path(&entry.archive)?;
            write_file(&dir, &path, &bytes).await.map_err(unavailable)?;
            let back = read_file(&dir, path.as_str()).await.map_err(unavailable)?;
            if back.as_deref() != Some(bytes.as_slice()) {
                return Err(BackupStoreError::Unavailable(
                    "the stored backup did not read back equal".to_string(),
                ));
            }
            let mut index = read_index(&dir).await;
            index.upsert(entry);
            write_index(&dir, &index).await
        })
    }

    fn get(&self, archive: &str) -> LocalBoxFuture<'_, Result<Vec<u8>, BackupStoreError>> {
        let archive = archive.to_string();
        Box::pin(async move {
            let dir = backups_dir().await.map_err(unavailable)?;
            let path = file_path(&archive)?;
            read_file(&dir, path.as_str())
                .await
                .map_err(unavailable)?
                .ok_or(BackupStoreError::Missing(archive))
        })
    }

    fn index(&self) -> LocalBoxFuture<'_, BackupIndex> {
        Box::pin(async move {
            match backups_dir().await {
                Ok(dir) => read_index(&dir).await,
                Err(error) => {
                    log::debug!("device backups: no store ({error}); index empty");
                    BackupIndex::default()
                }
            }
        })
    }

    fn mark(
        &self,
        archive: &str,
        status: BackupStatus,
    ) -> LocalBoxFuture<'_, Result<(), BackupStoreError>> {
        let archive = archive.to_string();
        Box::pin(async move {
            let dir = backups_dir().await.map_err(unavailable)?;
            let mut index = read_index(&dir).await;
            if !index.mark(&archive, status) {
                return Err(BackupStoreError::Missing(archive));
            }
            write_index(&dir, &index).await
        })
    }

    fn prune(
        &self,
        base_mac: &str,
        keep: usize,
    ) -> LocalBoxFuture<'_, Result<(), BackupStoreError>> {
        let base_mac = base_mac.to_string();
        Box::pin(async move {
            let dir = backups_dir().await.map_err(unavailable)?;
            let mut index = read_index(&dir).await;
            // Only archives the index NAMES as completed and surplus are
            // removed — never anything an unreadable index cannot vouch for.
            for victim in index.prune_victims(&base_mac, keep) {
                let path = file_path(&victim)?;
                if let Err(error) = remove_path(&dir, &path).await {
                    log::warn!("device backups: could not remove {victim}: {error}");
                    continue;
                }
                index.entries.retain(|entry| entry.archive != victim);
            }
            write_index(&dir, &index).await
        })
    }
}

async fn backups_dir() -> Result<web_sys::FileSystemDirectoryHandle, OpfsError> {
    let root = opfs_root().await?;
    open_dir(&root, BACKUPS_DIR, true).await
}

async fn read_index(dir: &web_sys::FileSystemDirectoryHandle) -> BackupIndex {
    match read_file(dir, &format!("/{INDEX_FILE}")).await {
        Ok(Some(bytes)) => BackupIndex::parse(&bytes),
        Ok(None) => BackupIndex::default(),
        Err(error) => {
            log::debug!("device backups: index unreadable ({error}); treated as empty");
            BackupIndex::default()
        }
    }
}

async fn write_index(
    dir: &web_sys::FileSystemDirectoryHandle,
    index: &BackupIndex,
) -> Result<(), BackupStoreError> {
    let path = LpPathBuf::from(format!("/{INDEX_FILE}").as_str());
    write_file(dir, &path, &index.to_bytes())
        .await
        .map_err(unavailable)
}

/// An archive's path inside the store; a name that would leave the
/// directory (or collide with the index) is refused.
fn file_path(archive: &str) -> Result<LpPathBuf, BackupStoreError> {
    if archive.is_empty()
        || archive == INDEX_FILE
        || archive.contains('/')
        || archive.contains('\\')
        || archive.starts_with('.')
    {
        return Err(BackupStoreError::Missing(archive.to_string()));
    }
    Ok(LpPathBuf::from(format!("/{archive}").as_str()))
}

fn unavailable(error: OpfsError) -> BackupStoreError {
    BackupStoreError::Unavailable(error.to_string())
}
