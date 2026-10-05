//! Where Studio gets an engine without a board: its engine cache, and the
//! firmware store (lightplayer.app's `/firmware/` lookup).
//!
//! Both are injected the way the device backup store is: the browser shell
//! installs OPFS (`firmware-cache/`) and a `fetch` port at the store origin
//! (`https://lightplayer.app`, or the loopback/LAN-only
//! `?firmware-store=` dev flag); tests, sims and hosts without OPFS keep the
//! defaults — a [`MemoryEngineCache`] and no store.
//!
//! Nothing reads them yet. The update flow (M4's host crate, ordered by M7)
//! asks the cache first, then the store, then the board; the USB install
//! (M5 P8) puts the engine it just flashed.

use std::rc::Rc;

use lpa_firmware_store::{EngineCache, FirmwareFetch, FirmwareStore, MemoryEngineCache};

/// The store client Studio holds: a type-erased fetch at one origin.
pub type StudioFirmwareStore = FirmwareStore<Rc<dyn FirmwareFetch>>;

/// The engine cache and the store, as the device effects hold them.
pub struct DeviceFirmwareSources {
    engine_cache: Rc<dyn EngineCache>,
    store: Option<Rc<StudioFirmwareStore>>,
}

impl Default for DeviceFirmwareSources {
    fn default() -> Self {
        Self {
            engine_cache: Rc::new(MemoryEngineCache::new()),
            store: None,
        }
    }
}

impl DeviceFirmwareSources {
    /// Install the engine cache (OPFS in the browser).
    pub fn set_engine_cache(&mut self, cache: Rc<dyn EngineCache>) {
        self.engine_cache = cache;
    }

    /// Install the firmware store client.
    pub fn set_store(&mut self, store: Rc<StudioFirmwareStore>) {
        self.store = Some(store);
    }

    /// The engine cache (a memory one until the shell installs its own).
    pub fn engine_cache(&self) -> Rc<dyn EngineCache> {
        Rc::clone(&self.engine_cache)
    }

    /// The firmware store, once the shell installed one.
    pub fn store(&self) -> Option<Rc<StudioFirmwareStore>> {
        self.store.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    use lpa_firmware_store::{
        EngineCacheEntry, EngineSource, FetchError, LocalBoxFuture, MemoryEngineCache,
    };
    use lpc_firmware_release::{ReleaseSelector, TargetName, sha256_hex};

    use super::*;
    use crate::StudioController;

    /// The injected cache and store are what the controller hands back, and
    /// a put/get round-trips through them.
    #[test]
    fn the_injected_cache_and_store_are_reachable_through_the_controller() {
        let mut controller = StudioController::new(|| 1.0);
        assert!(
            controller.firmware_store().is_none(),
            "no store until one is installed"
        );

        let cache = MemoryEngineCache::with_bound(1024);
        controller.set_engine_cache(Rc::new(cache.clone()));
        let fetch = Rc::new(RecordingFetch::default());
        controller.set_firmware_store(Rc::new(FirmwareStore::new(
            "http://127.0.0.1:2812",
            fetch.clone() as Rc<dyn FirmwareFetch>,
        )));

        let engine = vec![0xe0; 300];
        let entry = EngineCacheEntry::new(sha256_hex(&engine), 300, EngineSource::Installed, 2.0);
        block_on(controller.engine_cache().put(entry.clone(), engine.clone())).unwrap();
        assert_eq!(
            block_on(controller.engine_cache().get(&entry.sha256, 3.0)).unwrap(),
            engine
        );
        // The very cache the shell installed holds it.
        assert!(block_on(lpa_firmware_store::EngineCache::has(
            &cache,
            &entry.sha256
        )));

        let store = controller.firmware_store().expect("installed");
        assert_eq!(store.origin(), "http://127.0.0.1:2812");
        let target = TargetName::parse("esp32c6-4mb").unwrap();
        assert_eq!(
            block_on(store.manifest(&target, &ReleaseSelector::Latest)),
            Ok(None)
        );
        assert_eq!(
            fetch.urls.borrow().as_slice(),
            ["http://127.0.0.1:2812/firmware/esp32c6-4mb/latest/ota-manifest.json"]
        );
    }

    /// Answers 404 to everything and remembers what it was asked.
    #[derive(Default)]
    struct RecordingFetch {
        urls: RefCell<Vec<String>>,
    }

    impl FirmwareFetch for RecordingFetch {
        fn get(&self, url: &str) -> LocalBoxFuture<'_, Result<Option<Vec<u8>>, FetchError>> {
            self.urls.borrow_mut().push(url.to_string());
            Box::pin(std::future::ready(Ok(None)))
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("a memory cache / fake fetch future is immediately ready"),
        }
    }
}
