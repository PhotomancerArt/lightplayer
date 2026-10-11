pub mod app;
pub mod base;
#[cfg(target_arch = "wasm32")]
mod browser_board_hold;
mod clipboard;
pub mod cloud;
pub mod core;
mod dev_url_flags;
#[cfg(target_arch = "wasm32")]
mod device_backup_store_opfs;
mod device_events_io;
mod device_hint;
#[cfg(target_arch = "wasm32")]
mod engine_cache_opfs;
pub mod exploration;
#[cfg(target_arch = "wasm32")]
mod firmware_fetch_web;
#[cfg(target_arch = "wasm32")]
mod library_host_opfs;
mod local_model_probe;
mod local_store;
mod openrouter_oauth;
mod place_report;
mod record_lines;
mod record_sink;
mod route_recording;
mod router;
#[cfg(test)]
mod select_mirror_lint;
mod settings_io;
#[cfg(feature = "stories")]
mod stories;
mod unsaved_gate;
mod web_app;
mod wifi_addresses_io;

fn main() {
    // Before ANYTHING reads the URL — the router's boot parse, but also the
    // story book's and the preview lab's own early-return checks inside
    // `App` — turn a legacy `#/…` location into its path equivalent. Old
    // bookmarks, pasted links and the story-capture harness all still speak
    // hash; this keeps them working, and it is remove-never.
    router::install_legacy_hash_shim();
    // A shared device password rides `/unlock#…`: read and clear it before
    // the router writes the address.
    app::home::unlock_page::capture_unlock_fragment();
    dioxus::launch(web_app::App);
}
