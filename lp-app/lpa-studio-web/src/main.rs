mod app;
#[cfg(feature = "stories")]
mod stories;

fn main() {
    dioxus::launch(app::App);
}
