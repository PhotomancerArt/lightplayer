pub mod app;
pub mod base;
mod clipboard;
pub mod core;
pub mod exploration;
#[cfg(target_arch = "wasm32")]
mod library_host_opfs;
mod local_store;
mod openrouter_oauth;
mod router;
mod settings_io;
#[cfg(feature = "stories")]
mod stories;
mod unsaved_gate;
mod web_app;

fn main() {
    dioxus::launch(web_app::App);
}
