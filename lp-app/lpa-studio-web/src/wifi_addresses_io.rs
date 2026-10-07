//! The browser edge of Studio's Wi‑Fi address book: where each board this
//! browser has met is on Wi‑Fi, kept in `localStorage` under
//! [`lpa_studio_core::WIFI_ADDRESSES_STORAGE_KEY`].
//!
//! The book itself is core's (`wifi_address_book.rs`); this file only reads
//! its stored form once at boot and writes it back when core says it
//! changed. A convenience, not saved data: it never reaches the registry or
//! the account, and every access tolerates a storage that is blocked,
//! full or missing (private windows, a cleared site) — the page then starts
//! with an empty book and still works.

use lpa_studio_core::WIFI_ADDRESSES_STORAGE_KEY;

/// The book's stored form, or `None` when there is none or storage is
/// unavailable.
pub fn load_wifi_addresses_json() -> Option<String> {
    let storage = web_sys::window()?.local_storage().ok()??;
    storage.get_item(WIFI_ADDRESSES_STORAGE_KEY).ok()?
}

/// Keep the book's stored form (the controller's `on_wifi_addresses`
/// hook). A failure only warns: the addresses still apply this session.
pub fn store_wifi_addresses_json(json: &str) {
    let storage = web_sys::window().and_then(|window| window.local_storage().ok().flatten());
    let Some(storage) = storage else {
        log::warn!("wi-fi addresses not kept: localStorage is unavailable");
        return;
    };
    if let Err(error) = storage.set_item(WIFI_ADDRESSES_STORAGE_KEY, json) {
        log::warn!("wi-fi addresses not kept in localStorage: {error:?}");
    }
}
