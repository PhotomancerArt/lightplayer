pub mod app;
pub mod base;
mod clipboard;
pub mod cloud;
pub mod core;
mod device_events_io;
pub mod exploration;
#[cfg(target_arch = "wasm32")]
mod library_host_opfs;
mod local_model_probe;
mod local_store;
mod openrouter_oauth;
mod router;
mod settings_io;
#[cfg(feature = "stories")]
mod stories;
mod unsaved_gate;
mod web_app;

fn main() {
    // Before ANYTHING reads the URL — the router's boot parse, but also the
    // story book's and the preview lab's own early-return checks inside
    // `App` — turn a legacy `#/…` location into its path equivalent. Old
    // bookmarks, pasted links and the story-capture harness all still speak
    // hash; this keeps them working, and it is remove-never.
    router::install_legacy_hash_shim();
    dioxus::launch(web_app::App);
}
