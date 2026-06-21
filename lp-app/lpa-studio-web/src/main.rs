mod app;
mod components;
#[cfg(feature = "stories")]
mod stories;
mod web_provisioning_controller;

fn main() {
    dioxus::launch(app::App);
}
