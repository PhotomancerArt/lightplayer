//! The effects layer's half of the layout step (C6 repartition, plan P06):
//! the backup store, what each device's inspection staged, the store's
//! index as last read, and the backup a user asked to download.
//!
//! The decisions are `device_layout_step.rs`'s (pure); this file does the
//! awaiting — reading a pending backup, storing a new one and reading it
//! back, marking it completed — and holds the results between the
//! inspection, the user's yes, and the write. Shared (`Rc`) because spawned
//! effect futures reach it after the `&mut` borrow that spawned them ends.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpa_devices::LayoutVerdict;
use lpa_devices::identity::DeviceId;
use lpa_link::layout_migration::device_backup_archive::{backup_file_name, read_archive};
use lpa_link::{FlashPlan, LinkLayoutInspection, normalize_base_mac};

use super::device_backup_store::{
    BACKUPS_KEPT_PER_BOARD, BackupEntry, BackupIndex, BackupStatus, DeviceBackupStore,
};
use super::device_effects::DeviceTaskFuture;
use super::device_layout_step::{LayoutStaging, PendingBackup, stage_layout};
use crate::app::library::library_host::LocalBoxFuture;

/// A backup the web shell should hand the user as a file. A new `seq` is a
/// new download; the shell downloads when it sees `seq` advance.
#[derive(Clone, Debug, PartialEq)]
pub struct BackupDownload {
    pub seq: u64,
    pub file_name: String,
    pub bytes: Rc<Vec<u8>>,
}

/// The layout step's state.
#[derive(Clone, Default)]
pub struct LayoutEffects {
    inner: Rc<RefCell<LayoutInner>>,
}

#[derive(Default)]
struct LayoutInner {
    store: Option<Rc<dyn DeviceBackupStore>>,
    clock: Option<Rc<dyn Fn() -> f64>>,
    staged: BTreeMap<DeviceId, LayoutStaging>,
    /// The store's index as last read (refreshed after every write).
    index: BackupIndex,
    download: Option<BackupDownload>,
    download_seq: u64,
}

impl LayoutEffects {
    pub(crate) fn set_store(&self, store: Rc<dyn DeviceBackupStore>) {
        self.inner.borrow_mut().store = Some(store);
    }

    pub(crate) fn set_clock(&self, clock: Rc<dyn Fn() -> f64>) {
        self.inner.borrow_mut().clock = Some(clock);
    }

    fn now(&self) -> f64 {
        self.inner
            .borrow()
            .clock
            .as_ref()
            .map(|clock| clock())
            .unwrap_or(0.0)
    }

    fn store(&self) -> Option<Rc<dyn DeviceBackupStore>> {
        self.inner.borrow().store.clone()
    }

    /// Re-read the store's index into the cache the card joins against.
    pub(crate) fn refresh_index_task(&self) -> DeviceTaskFuture {
        let this = self.clone();
        Box::pin(async move { this.refresh_index().await })
    }

    async fn refresh_index(&self) {
        let Some(store) = self.store() else {
            return;
        };
        let index = store.index().await;
        self.inner.borrow_mut().index = index;
    }

    /// What the inspection staged for `device`.
    pub fn staged(&self, device: DeviceId) -> Option<LayoutStaging> {
        self.inner.borrow().staged.get(&device).cloned()
    }

    /// The pending backup the resume rule would offer for a board with this
    /// base MAC (from the cached index).
    pub fn pending_for(&self, base_mac: &str) -> Option<BackupEntry> {
        let mac = normalize_base_mac(base_mac)?;
        self.inner.borrow().index.pending_for(&mac).cloned()
    }

    /// The plan a carried flash runs, confirmed — or why it may not run.
    pub(crate) fn plan_for_flash(&self, device: DeviceId) -> Result<FlashPlan, String> {
        self.inner
            .borrow()
            .staged
            .get(&device)
            .ok_or_else(|| {
                "the board's layout was not read first — nothing was written".to_string()
            })?
            .confirmed_plan()
    }

    /// The layout step after the provider's inspection: classify, plan,
    /// store the backup (read back), and answer the verdict the model is
    /// told. `inspection == None` (a transport that inspects nothing — a
    /// sim, an emulated tab board) is a plain flash.
    pub(crate) fn after_inspection(
        &self,
        device: DeviceId,
        inspection: Option<LinkLayoutInspection>,
        restore_requested: bool,
    ) -> LocalBoxFuture<'static, Result<LayoutVerdict, String>> {
        let this = self.clone();
        Box::pin(async move {
            let Some(inspection) = inspection else {
                this.inner.borrow_mut().staged.remove(&device);
                return Ok(LayoutVerdict::Plain);
            };
            let store = this.store();
            // The resume rule's candidate: this board's newest backup, when
            // it is still pending — read whole, so a restore can re-pack it.
            if let Some(store) = &store {
                let index = store.index().await;
                this.inner.borrow_mut().index = index;
            }
            let pending_entry = inspection
                .probed_mac
                .as_deref()
                .and_then(normalize_base_mac)
                .and_then(|mac| this.inner.borrow().index.pending_for(&mac).cloned());
            let pending_tree = match (&store, &pending_entry) {
                (Some(store), Some(entry)) => match store.get(&entry.archive).await {
                    Ok(bytes) => read_archive(&bytes).ok().map(|(_, tree)| tree),
                    Err(error) => {
                        log::warn!("pending backup {} unreadable: {error}", entry.archive);
                        None
                    }
                },
                _ => None,
            };
            let pending = match (&pending_entry, &pending_tree) {
                (Some(entry), Some(tree)) => Some(PendingBackup { entry, tree }),
                _ => None,
            };
            let mut staging = stage_layout(&inspection, pending, restore_requested, this.now())?;
            // A migration's backup goes into the store BEFORE the verdict is
            // reported — and only a put that read back equal counts.
            if let (LayoutVerdict::Migrate { backup_stored, .. }, Some(archive)) =
                (&mut staging.verdict, &staging.archive)
            {
                *backup_stored = match &store {
                    Some(store) => match store
                        .put(archive.entry.clone(), archive.bytes.clone())
                        .await
                    {
                        Ok(()) => true,
                        Err(error) => {
                            log::warn!("layout backup not stored: {error}");
                            false
                        }
                    },
                    None => false,
                };
                if let Some(store) = &store {
                    let index = store.index().await;
                    this.inner.borrow_mut().index = index;
                }
            }
            let verdict = staging.verdict.clone();
            this.inner.borrow_mut().staged.insert(device, staging);
            Ok(verdict)
        })
    }

    /// The post-flash hello proved the files arrived: the backup that
    /// covered the write is completed, and older ones are pruned.
    pub(crate) fn complete_task(&self, device: DeviceId) -> DeviceTaskFuture {
        let this = self.clone();
        Box::pin(async move {
            let Some(staging) = this.inner.borrow_mut().staged.remove(&device) else {
                return;
            };
            let Some(store) = this.store() else {
                return;
            };
            let (archive, mac) = match (&staging.archive, &staging.restoring) {
                (Some(archive), _) => (
                    archive.entry.archive.clone(),
                    archive.entry.base_mac.clone(),
                ),
                (None, Some(name)) => {
                    let mac = staging
                        .plan
                        .as_ref()
                        .and_then(|plan| plan.base_mac.clone())
                        .unwrap_or_default();
                    (name.clone(), mac)
                }
                (None, None) => return,
            };
            if let Err(error) = store.mark(&archive, BackupStatus::Completed).await {
                log::warn!("could not mark backup {archive} completed: {error}");
            }
            if let Err(error) = store.prune(&mac, BACKUPS_KEPT_PER_BOARD).await {
                log::warn!("could not prune backups of {mac}: {error}");
            }
            this.refresh_index().await;
        })
    }

    /// Hand the user `device`'s backup as a file: the one its inspection
    /// staged, or — on a card offering a restore — the stored pending one.
    pub(crate) fn download_task(
        &self,
        device: DeviceId,
        base_mac: Option<String>,
        label: Option<String>,
    ) -> DeviceTaskFuture {
        let this = self.clone();
        Box::pin(async move {
            let now = this.now();
            let file_name = backup_file_name(label.as_deref().or(base_mac.as_deref()), now);
            let staged = this
                .inner
                .borrow()
                .staged
                .get(&device)
                .and_then(|staging| staging.archive.as_ref().map(|a| a.bytes.clone()));
            let bytes = match staged {
                Some(bytes) => Some(bytes),
                None => match (
                    this.store(),
                    base_mac.as_deref().and_then(|mac| this.pending_for(mac)),
                ) {
                    (Some(store), Some(entry)) => store.get(&entry.archive).await.ok(),
                    _ => None,
                },
            };
            let Some(bytes) = bytes else {
                log::warn!("no backup to download for device {device:?}");
                return;
            };
            let mut inner = this.inner.borrow_mut();
            inner.download_seq += 1;
            let seq = inner.download_seq;
            inner.download = Some(BackupDownload {
                seq,
                file_name,
                bytes: Rc::new(bytes),
            });
            // A download is the other way a migration's backup is safe: the
            // consent's Continue unlocks on it (plan MQ4).
            if let Some(staging) = inner.staged.get_mut(&device) {
                staging.downloaded = true;
            }
        })
    }

    /// The latest requested download (the shell downloads on a new `seq`).
    pub fn download(&self) -> Option<BackupDownload> {
        self.inner.borrow().download.clone()
    }

    /// Drop what was staged for a device whose activity ended without
    /// writing (refused, cancelled) or that was forgotten. The stored backup
    /// stays in the store.
    pub fn forget(&self, device: DeviceId) {
        self.inner.borrow_mut().staged.remove(&device);
    }
}
