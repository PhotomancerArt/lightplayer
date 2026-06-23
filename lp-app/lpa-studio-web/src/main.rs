mod app;
#[cfg(feature = "stories")]
mod stories;
pub mod ui_base;
pub mod ui_core;
pub mod ui_studio;

fn main() {
    dioxus::launch(app::App);
}
