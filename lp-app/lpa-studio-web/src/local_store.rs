//! Studio-side lifecycle of the local project store.
//!
//! At startup the studio requests best-effort storage durability, takes the
//! library single-writer Web Lock, and mounts the OPFS-backed store
//! (`lpa_fs_opfs::LpFsOpfs`) with its background flush loop. The simulator
//! never sees this store — persistence belongs to the local project store
//! and the sim is an ephemeral place (roadmap D19/D20).
//!
//! The lock key currently guards the whole library: until the places layer
//! (roadmap M3) gives projects identity and an open-project flow, the mount
//! is the only meaningful acquisition point. M3 re-keys locking per project.

use std::fmt;

/// Where the local store stands, for the shell banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalStoreStatus {
    /// Mount in progress; render nothing.
    Initializing,
    /// Mounted, flusher running.
    Ready,
    /// Another tab holds the library lock; retry after closing it.
    LockedByAnotherTab,
    /// OPFS (or this environment) can't back the store.
    Unavailable(String),
}

impl fmt::Display for LocalStoreStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocalStoreStatus::Initializing => f.write_str("initializing"),
            LocalStoreStatus::Ready => f.write_str("ready"),
            LocalStoreStatus::LockedByAnotherTab => f.write_str("locked by another tab"),
            LocalStoreStatus::Unavailable(reason) => write!(f, "unavailable: {reason}"),
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::cell::RefCell;

    use lpa_fs_opfs::{LpFsOpfs, acquire_exclusive_lock, open_library_root, run_flush_loop};
    use wasm_bindgen_futures::JsFuture;

    use super::LocalStoreStatus;

    /// Flush cadence for the library store.
    const FLUSH_INTERVAL_MS: u32 = 100;

    /// Web Lock guarding the library store (see module docs re: granularity).
    const LIBRARY_LOCK_KEY: &str = "lp-library";

    thread_local! {
        static LOCAL_STORE: RefCell<Option<LpFsOpfs>> = const { RefCell::new(None) };
    }

    /// The mounted store, once [`init_local_store`] returned `Ready`.
    ///
    /// This is the handle the places/library layer (roadmap M3) builds on.
    pub fn local_store() -> Option<LpFsOpfs> {
        LOCAL_STORE.with(|s| s.borrow().clone())
    }

    /// The library layer over the mounted store, with browser randomness
    /// (crypto.getRandomValues) injected for uid minting.
    pub fn library_store() -> Option<lpa_studio_core::app::library::LibraryStore> {
        let store = local_store()?;
        let fs: std::rc::Rc<std::cell::RefCell<dyn lpfs::LpFs>> =
            std::rc::Rc::new(std::cell::RefCell::new(store));
        Some(lpa_studio_core::app::library::LibraryStore::new(
            fs,
            std::rc::Rc::new(random_bytes),
            std::rc::Rc::new(local_slug_stamp),
        ))
    }

    /// Local wall-clock `YYYY-MM-DD-HHMM` for new-package slugs (the
    /// sans-IO core takes this injected — it never reads a clock).
    fn local_slug_stamp() -> String {
        let now = js_sys::Date::new_0();
        format!(
            "{:04}-{:02}-{:02}-{:02}{:02}",
            now.get_full_year(),
            now.get_month() + 1,
            now.get_date(),
            now.get_hours(),
            now.get_minutes(),
        )
    }

    fn random_bytes() -> [u8; 16] {
        let mut bytes = [0u8; 16];
        let filled = web_sys::window()
            .and_then(|w| w.crypto().ok())
            .and_then(|c| c.get_random_values_with_u8_array(&mut bytes).ok())
            .is_some();
        if !filled {
            // last-resort fallback; uids only need uniqueness, not secrecy
            for b in bytes.iter_mut() {
                *b = (js_sys::Math::random() * 256.0) as u8;
            }
        }
        bytes
    }

    /// Request best-effort storage durability; fire-and-forget, logged.
    pub fn request_persist() {
        let Some(window) = web_sys::window() else {
            return;
        };
        let storage = window.navigator().storage();
        wasm_bindgen_futures::spawn_local(async move {
            let promise = match storage.persist() {
                Ok(promise) => promise,
                Err(e) => {
                    log::warn!("storage.persist() unavailable: {e:?}");
                    return;
                }
            };
            match JsFuture::from(promise).await {
                Ok(granted) => {
                    log::info!("storage.persist() granted: {:?}", granted.as_bool());
                }
                Err(e) => log::warn!("storage.persist() failed: {e:?}"),
            }
        });
    }

    /// Acquire the library lock and mount the store.
    ///
    /// Safe to call again after `LockedByAnotherTab` (the Retry flow); a
    /// `Ready` result is idempotent.
    pub async fn init_local_store() -> LocalStoreStatus {
        if local_store().is_some() {
            return LocalStoreStatus::Ready;
        }
        match acquire_exclusive_lock(LIBRARY_LOCK_KEY).await {
            Ok(true) => {}
            Ok(false) => return LocalStoreStatus::LockedByAnotherTab,
            Err(e) => {
                // Web Locks missing (very old browser / non-secure context):
                // proceed unguarded rather than losing persistence entirely.
                log::warn!("web locks unavailable, proceeding without single-writer guard: {e:?}");
            }
        }
        let dir = match open_library_root().await {
            Ok(dir) => dir,
            Err(e) => return LocalStoreStatus::Unavailable(e.to_string()),
        };
        match LpFsOpfs::mount(dir).await {
            Ok(store) => {
                wasm_bindgen_futures::spawn_local(run_flush_loop(store.clone(), FLUSH_INTERVAL_MS));
                LOCAL_STORE.with(|s| *s.borrow_mut() = Some(store));
                log::info!("local project store mounted");
                LocalStoreStatus::Ready
            }
            Err(e) => LocalStoreStatus::Unavailable(e.to_string()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{init_local_store, library_store, request_persist};

/// Host builds run unit tests only and never mount a store.
#[cfg(not(target_arch = "wasm32"))]
pub async fn init_local_store() -> LocalStoreStatus {
    LocalStoreStatus::Unavailable("not a browser".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn request_persist() {}
