//! Flash-backed LpFs implementation using littlefs-rust.
//!
//! Wraps littlefs Filesystem over LpFlashStorage and implements the LpFs trait.
//! Supports mount/format with fallback to memory on failure.

use alloc::{format, rc::Rc, string::ToString, vec::Vec};
use core::cell::RefCell;
use hashbrown::HashMap;

use littlefs_rust::{metadata::FileType, Config, Error as LfsError, Filesystem, OpenFlags};
use lp_model::path::{LpPath, LpPathBuf};
use lp_shared::fs::{
    fs_event::ChangeType,
    fs_event::{FsChange, FsVersion},
    LpFs, LpFsView,
};
use lp_shared::FsError;

use crate::flash_storage::{lpfs_config, LpFlashStorage};

/// Convert LpPath to littlefs path (strip leading /, root becomes "")
fn to_lfs_path(path: &LpPath) -> &str {
    let s = path.as_str();
    if s == "/" || s.is_empty() {
        ""
    } else if s.starts_with('/') {
        &s[1..]
    } else {
        s
    }
}

/// Convert littlefs path to LpPathBuf (ensure leading /)
fn from_lfs_path(parent: &str, name: &str) -> LpPathBuf {
    let parent = parent.trim_end_matches('/');
    if parent.is_empty() {
        LpPathBuf::from(format!("/{name}"))
    } else {
        LpPathBuf::from(format!("/{parent}/{name}"))
    }
}

/// Inner state shared between LpFsFlash and its chroot views
struct LpFsFlashInner {
    fs: Filesystem<LpFlashStorage>,
    current_version: RefCell<FsVersion>,
    changes: RefCell<HashMap<LpPathBuf, (FsVersion, ChangeType)>>,
}

impl LpFsFlashInner {
    fn record_change(&self, path: &LpPath, change_type: ChangeType) {
        let mut current = self.current_version.borrow_mut();
        *current = current.next();
        let version = *current;
        drop(current);
        self.changes
            .borrow_mut()
            .insert(path.to_path_buf(), (version, change_type));
    }
}

/// Flash-backed filesystem implementing LpFs
pub struct LpFsFlash {
    inner: Rc<RefCell<LpFsFlashInner>>,
}

impl LpFsFlash {
    /// Initialize flash filesystem: try mount, on failure format and retry.
    ///
    /// Returns Ok(LpFsFlash) on success, Err on persistent failure.
    pub fn init(flash: esp_storage::FlashStorage<'static>) -> Result<Self, LfsError> {
        let config = lpfs_config();
        let storage = LpFlashStorage::new(flash);

        // Try mount first
        match Filesystem::mount(storage, config) {
            Ok(fs) => {
                let inner = LpFsFlashInner {
                    fs,
                    current_version: RefCell::new(FsVersion::default()),
                    changes: RefCell::new(HashMap::new()),
                };
                Ok(Self {
                    inner: Rc::new(RefCell::new(inner)),
                })
            }
            Err(_) => {
                // Mount failed - format and retry
                let mut storage = LpFlashStorage::new(esp_storage::FlashStorage::new(
                    unsafe { core::mem::transmute(esp_hal::peripherals::FLASH::steal()) },
                ));
                // Actually we can't easily recover the storage from the failed mount.
                // The mount takes ownership. So we need a different approach.
                // Create fresh FlashStorage - but FlashStorage::new can only be called once!
                // So we need to get the flash peripheral from somewhere. The init receives it.
                // On mount failure, we've consumed the storage. We need to not consume it on
                // format. Looking at littlefs Filesystem::format - it takes &mut storage.
                // So we need to: 1) create storage, 2) try format, 3) try mount.
                // Let me re-read the init flow. We receive flash: FlashStorage. So the caller
                // has already created FlashStorage. We create LpFlashStorage::new(flash) - that
                // takes ownership. Then we try mount - that takes ownership of LpFlashStorage.
                // On mount failure, we've lost the storage. We can't retry without a new
                // FlashStorage. So the init needs to receive the FLASH peripheral, create
                // FlashStorage once, then create LpFlashStorage, try format (which borrows
                // storage), then mount. Let me check Filesystem::format signature.
            }
        }
    }
}
