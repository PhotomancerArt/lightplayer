//! Where Studio keeps the backup a layout migration takes before it writes
//! anything (the C6 repartition, plan MQ4 / Q14).
//!
//! A migration's write order (MQ9) has a window in which the stored backup
//! is the ONLY copy of a board's files, so the backup is stored — and read
//! back — before the first write, and the plan is not marked confirmed
//! until it is. The store is a seam: the browser's is OPFS
//! (`lpa-studio-web`, `device-backups/` beside the worker-owned
//! `emu-flash/`), tests and hosts without one use [`MemoryBackupStore`].
//!
//! # The index (`device-backups/index.json`, `version: 1`)
//!
//! Per board (base MAC): which archive, why it was taken, and whether the
//! migration that took it finished (`pending` / `completed`). It is a
//! cache-like sidecar: **unreadable or a foreign version reads as empty**,
//! and an archive is never deleted because the index could not be read. A
//! `pending` entry is what makes Studio offer "Restore files".

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::app::library::library_host::LocalBoxFuture;

/// The index format this build reads and writes.
pub const BACKUP_INDEX_VERSION: u32 = 1;

/// How many backups per board survive a prune.
pub const BACKUPS_KEPT_PER_BOARD: usize = 2;

/// Whether the migration that took a backup finished.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackupStatus {
    /// Taken; the board has not yet proven it holds these files.
    Pending,
    /// The board came back with the files mounted and its own identity.
    Completed,
}

/// One stored backup.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    /// The board's base MAC (lowercase colon hex).
    pub base_mac: String,
    /// The archive's file name inside the store.
    pub archive: String,
    pub captured_at_epoch_seconds: f64,
    /// `"backup"` or `"layout-migration"` (the archive manifest's purpose).
    pub purpose: String,
    pub status: BackupStatus,
    pub file_count: u32,
    pub total_bytes: u64,
}

/// `device-backups/index.json`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackupIndex {
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<BackupEntry>,
}

impl Default for BackupIndex {
    fn default() -> Self {
        Self {
            version: BACKUP_INDEX_VERSION,
            entries: Vec::new(),
        }
    }
}

impl BackupIndex {
    /// Parse the stored index; anything unreadable — or a version this build
    /// does not know — is an empty index (cache-like, plan Q14).
    pub fn parse(bytes: &[u8]) -> Self {
        match serde_json::from_slice::<BackupIndex>(bytes) {
            Ok(index) if index.version == BACKUP_INDEX_VERSION => index,
            Ok(index) => {
                log::debug!("backup index version {} ignored", index.version);
                Self::default()
            }
            Err(error) => {
                log::debug!("backup index unreadable ({error}); treated as empty");
                Self::default()
            }
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec_pretty(self).unwrap_or_default()
    }

    /// The newest still-pending backup for `base_mac`, unless a completed
    /// one is newer (the resume rule).
    pub fn pending_for(&self, base_mac: &str) -> Option<&BackupEntry> {
        let newest = self
            .entries
            .iter()
            .filter(|entry| entry.base_mac == base_mac)
            .max_by(|a, b| {
                a.captured_at_epoch_seconds
                    .total_cmp(&b.captured_at_epoch_seconds)
            })?;
        (newest.status == BackupStatus::Pending).then_some(newest)
    }

    /// Record `entry` (replacing one with the same archive name).
    pub fn upsert(&mut self, entry: BackupEntry) {
        self.entries.retain(|e| e.archive != entry.archive);
        self.entries.push(entry);
    }

    /// Mark `archive`'s status; `false` when the index does not list it.
    pub fn mark(&mut self, archive: &str, status: BackupStatus) -> bool {
        match self.entries.iter_mut().find(|e| e.archive == archive) {
            Some(entry) => {
                entry.status = status;
                true
            }
            None => false,
        }
    }

    /// The archives a prune drops for `base_mac`: all but the newest
    /// `keep`, never one still pending.
    pub fn prune_victims(&self, base_mac: &str, keep: usize) -> Vec<String> {
        let mut mine: Vec<&BackupEntry> = self
            .entries
            .iter()
            .filter(|e| e.base_mac == base_mac)
            .collect();
        mine.sort_by(|a, b| {
            b.captured_at_epoch_seconds
                .total_cmp(&a.captured_at_epoch_seconds)
        });
        mine.into_iter()
            .skip(keep)
            .filter(|e| e.status == BackupStatus::Completed)
            .map(|e| e.archive.clone())
            .collect()
    }
}

/// Why the store could not do something.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackupStoreError {
    /// No storage, or a write/read-back that failed: the flow asks the user
    /// to download the backup instead (plan MQ4).
    Unavailable(String),
    /// The named archive is not there.
    Missing(String),
}

impl core::fmt::Display for BackupStoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unavailable(why) => write!(f, "the backup could not be stored here: {why}"),
            Self::Missing(name) => write!(f, "backup {name} is not in this browser"),
        }
    }
}

/// The seam. Futures are runtime-neutral (`LibraryHost`'s rule): no spawns,
/// no executor-flavored sleeps.
pub trait DeviceBackupStore {
    /// Store `bytes` as `entry.archive` and record `entry`; `Ok` only once
    /// the bytes were READ BACK equal.
    fn put(
        &self,
        entry: BackupEntry,
        bytes: Vec<u8>,
    ) -> LocalBoxFuture<'_, Result<(), BackupStoreError>>;
    /// An archive's bytes.
    fn get(&self, archive: &str) -> LocalBoxFuture<'_, Result<Vec<u8>, BackupStoreError>>;
    /// The index (empty when unreadable).
    fn index(&self) -> LocalBoxFuture<'_, BackupIndex>;
    /// Set an archive's status.
    fn mark(
        &self,
        archive: &str,
        status: BackupStatus,
    ) -> LocalBoxFuture<'_, Result<(), BackupStoreError>>;
    /// Drop all but the newest `keep` completed backups of `base_mac`.
    fn prune(
        &self,
        base_mac: &str,
        keep: usize,
    ) -> LocalBoxFuture<'_, Result<(), BackupStoreError>>;
}

/// An in-memory store (tests, sims, hosts with no OPFS).
#[derive(Clone, Default)]
pub struct MemoryBackupStore {
    inner: Rc<RefCell<MemoryInner>>,
}

#[derive(Default)]
struct MemoryInner {
    archives: BTreeMap<String, Vec<u8>>,
    index: BackupIndex,
    /// Test knob: every put fails as unavailable.
    unavailable: bool,
}

impl MemoryBackupStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// A store whose puts fail (the "storage unavailable" path).
    pub fn unavailable() -> Self {
        let store = Self::default();
        store.inner.borrow_mut().unavailable = true;
        store
    }

    /// The stored archive names.
    pub fn archive_names(&self) -> Vec<String> {
        self.inner.borrow().archives.keys().cloned().collect()
    }
}

impl DeviceBackupStore for MemoryBackupStore {
    fn put(
        &self,
        entry: BackupEntry,
        bytes: Vec<u8>,
    ) -> LocalBoxFuture<'_, Result<(), BackupStoreError>> {
        let result = {
            let mut inner = self.inner.borrow_mut();
            if inner.unavailable {
                Err(BackupStoreError::Unavailable("no storage".to_string()))
            } else {
                inner.archives.insert(entry.archive.clone(), bytes);
                inner.index.upsert(entry);
                Ok(())
            }
        };
        Box::pin(core::future::ready(result))
    }

    fn get(&self, archive: &str) -> LocalBoxFuture<'_, Result<Vec<u8>, BackupStoreError>> {
        let result = self
            .inner
            .borrow()
            .archives
            .get(archive)
            .cloned()
            .ok_or_else(|| BackupStoreError::Missing(archive.to_string()));
        Box::pin(core::future::ready(result))
    }

    fn index(&self) -> LocalBoxFuture<'_, BackupIndex> {
        let index = self.inner.borrow().index.clone();
        Box::pin(core::future::ready(index))
    }

    fn mark(
        &self,
        archive: &str,
        status: BackupStatus,
    ) -> LocalBoxFuture<'_, Result<(), BackupStoreError>> {
        let marked = self.inner.borrow_mut().index.mark(archive, status);
        let result = match marked {
            true => Ok(()),
            false => Err(BackupStoreError::Missing(archive.to_string())),
        };
        Box::pin(core::future::ready(result))
    }

    fn prune(
        &self,
        base_mac: &str,
        keep: usize,
    ) -> LocalBoxFuture<'_, Result<(), BackupStoreError>> {
        let mut inner = self.inner.borrow_mut();
        for victim in inner.index.prune_victims(base_mac, keep) {
            inner.archives.remove(&victim);
            inner.index.entries.retain(|e| e.archive != victim);
        }
        Box::pin(core::future::ready(Ok(())))
    }
}

/// The contract every [`DeviceBackupStore`] keeps — run against the memory
/// store here, and against the OPFS store in `lpa-studio-web`'s browser
/// tests.
pub async fn check_store_contract(store: &dyn DeviceBackupStore) {
    let entry = |name: &str, at: f64, status: BackupStatus| BackupEntry {
        base_mac: "10:bd:a3:b0:8e:30".to_string(),
        archive: name.to_string(),
        captured_at_epoch_seconds: at,
        purpose: "layout-migration".to_string(),
        status,
        file_count: 3,
        total_bytes: 100,
    };
    store
        .put(entry("a.zip", 1.0, BackupStatus::Completed), vec![1, 2, 3])
        .await
        .expect("put a");
    store
        .put(entry("b.zip", 2.0, BackupStatus::Completed), vec![4])
        .await
        .expect("put b");
    store
        .put(entry("c.zip", 3.0, BackupStatus::Pending), vec![5, 6])
        .await
        .expect("put c");
    assert_eq!(store.get("a.zip").await.expect("get a"), vec![1, 2, 3]);
    let index = store.index().await;
    assert_eq!(index.entries.len(), 3);
    assert_eq!(
        index
            .pending_for("10:bd:a3:b0:8e:30")
            .map(|e| e.archive.as_str()),
        Some("c.zip")
    );
    store
        .mark("c.zip", BackupStatus::Completed)
        .await
        .expect("mark c");
    assert!(
        store
            .index()
            .await
            .pending_for("10:bd:a3:b0:8e:30")
            .is_none()
    );
    store
        .prune("10:bd:a3:b0:8e:30", BACKUPS_KEPT_PER_BOARD)
        .await
        .expect("prune");
    assert!(matches!(
        store.get("a.zip").await,
        Err(BackupStoreError::Missing(_))
    ));
    assert_eq!(store.get("c.zip").await.expect("c kept"), vec![5, 6]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_on<F: Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, Waker};
        let mut future = std::pin::pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("a memory store future is immediately ready"),
        }
    }

    #[test]
    fn the_memory_store_keeps_the_contract() {
        block_on(check_store_contract(&MemoryBackupStore::new()));
    }

    #[test]
    fn an_unreadable_or_foreign_index_reads_as_empty() {
        assert_eq!(BackupIndex::parse(b"not json"), BackupIndex::default());
        assert_eq!(
            BackupIndex::parse(br#"{"version":2,"entries":[]}"#),
            BackupIndex::default()
        );
        let index = BackupIndex::parse(
            br#"{"version":1,"entries":[{"baseMac":"aa","archive":"x.zip",
                "capturedAtEpochSeconds":1.0,"purpose":"layout-migration",
                "status":"pending","fileCount":1,"totalBytes":2}]}"#,
        );
        assert_eq!(index.entries.len(), 1);
        assert_eq!(
            index.pending_for("aa").unwrap().status,
            BackupStatus::Pending
        );
    }

    #[test]
    fn a_newer_completed_backup_supersedes_an_older_pending_one() {
        let mut index = BackupIndex::default();
        let entry = |name: &str, at: f64, status| BackupEntry {
            base_mac: "aa".to_string(),
            archive: name.to_string(),
            captured_at_epoch_seconds: at,
            purpose: "layout-migration".to_string(),
            status,
            file_count: 1,
            total_bytes: 1,
        };
        index.upsert(entry("old.zip", 1.0, BackupStatus::Pending));
        index.upsert(entry("new.zip", 2.0, BackupStatus::Completed));
        assert!(index.pending_for("aa").is_none());
        // A prune never drops a pending backup, however old.
        assert!(index.prune_victims("aa", 0).iter().all(|v| v != "old.zip"));
    }

    #[test]
    fn an_unavailable_store_refuses_puts_by_name() {
        let store = MemoryBackupStore::unavailable();
        let entry = BackupEntry {
            base_mac: "aa".to_string(),
            archive: "x.zip".to_string(),
            captured_at_epoch_seconds: 1.0,
            purpose: "layout-migration".to_string(),
            status: BackupStatus::Pending,
            file_count: 1,
            total_bytes: 1,
        };
        assert!(matches!(
            block_on(store.put(entry, vec![1])),
            Err(BackupStoreError::Unavailable(_))
        ));
    }
}
