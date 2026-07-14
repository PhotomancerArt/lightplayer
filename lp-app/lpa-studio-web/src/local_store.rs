//! Startup probe + host construction for the local project library.
//!
//! Since M4b there is no whole-store mount and no page-level library
//! lock: the studio probes that OPFS can back a library at all, builds
//! the per-project [`OpfsLibraryHost`](crate::library_host_opfs), and
//! attaches it to the actor. Locks are per project (acquired on open)
//! plus a short-lived catalog lock inside transactions — see
//! `library_host_opfs` for the model. The simulator never sees this
//! store: persistence belongs to the local project store and the sim is
//! an ephemeral place (roadmap D19/D20).

use std::fmt;

/// Where the local store stands, for the shell banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalStoreStatus {
    /// Probe in progress; render nothing.
    Initializing,
    /// The library host is attached; projects persist.
    Ready,
    /// OPFS (or this environment) can't back the store.
    Unavailable(String),
}

impl fmt::Display for LocalStoreStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LocalStoreStatus::Initializing => f.write_str("initializing"),
            LocalStoreStatus::Ready => f.write_str("ready"),
            LocalStoreStatus::Unavailable(reason) => write!(f, "unavailable: {reason}"),
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lpa_fs_opfs::open_library_root;
    use wasm_bindgen_futures::JsFuture;

    use super::LocalStoreStatus;
    use crate::library_host_opfs::OpfsLibraryHost;

    thread_local! {
        static LIBRARY_HOST: RefCell<Option<Rc<OpfsLibraryHost>>> = const { RefCell::new(None) };
    }

    /// The concrete OPFS host, once [`init_local_store`] returned `Ready`
    /// (the pagehide flush hook needs the concrete type).
    pub fn opfs_library_host() -> Option<Rc<OpfsLibraryHost>> {
        LIBRARY_HOST.with(|host| host.borrow().clone())
    }

    /// The host as the core's seam type — what `AttachLibrary` carries and
    /// what read paths (zip export) hydrate snapshots through.
    pub fn library_host() -> Option<Rc<dyn lpa_studio_core::app::library::LibraryHost>> {
        opfs_library_host().map(|host| host as Rc<dyn lpa_studio_core::app::library::LibraryHost>)
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

    /// Probe OPFS and construct the library host. No lock is taken —
    /// per-project locks are acquired when projects open. Idempotent.
    pub async fn init_local_store() -> LocalStoreStatus {
        if opfs_library_host().is_some() {
            return LocalStoreStatus::Ready;
        }
        if let Err(e) = open_library_root().await {
            return LocalStoreStatus::Unavailable(e.to_string());
        }
        LIBRARY_HOST.with(|host| *host.borrow_mut() = Some(Rc::new(OpfsLibraryHost::new())));
        log::info!("local project library host ready (per-project locking)");
        LocalStoreStatus::Ready
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{init_local_store, library_host, opfs_library_host, request_persist};

/// Host builds run unit tests only and never mount a store.
#[cfg(not(target_arch = "wasm32"))]
pub async fn init_local_store() -> LocalStoreStatus {
    LocalStoreStatus::Unavailable("not a browser".to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn request_persist() {}
