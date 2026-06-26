pub mod app;
pub mod base;
pub mod core;
pub mod exploration;
#[cfg(feature = "stories")]
mod stories;
mod studio_url;
mod web_app;

fn main() {
    dioxus::launch(web_app::App);
}
