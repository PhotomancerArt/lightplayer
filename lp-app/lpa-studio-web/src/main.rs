pub mod app;
pub mod base;
pub mod core;
pub mod exploration;
#[cfg(target_arch = "wasm32")]
mod library_host_opfs;
mod local_store;
mod router;
#[cfg(feature = "stories")]
mod stories;
mod web_app;

fn main() {
    dioxus::launch(web_app::App);
}
