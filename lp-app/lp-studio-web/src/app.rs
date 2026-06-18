use dioxus::prelude::*;
use lp_studio_core::StudioApp;
use lp_studio_runtime::run_browser_worker_demo;

use crate::components::device_panel::DevicePanel;
use crate::components::inventory_view::InventoryView;
use crate::components::log_panel::LogPanel;
use crate::components::project_panel::ProjectPanel;
use crate::components::status_bar::StatusBar;

const STYLE: &str = include_str!("style.css");
const WORKER_URL: &str = "./fw-browser-worker.js";

#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn App() -> Element {
    #[cfg(feature = "stories")]
    if crate::stories::story_book::should_show_story_book() {
        return rsx! {
            style { "{STYLE}" }
            crate::stories::story_book::StoryBook {}
        };
    }

    let mut studio = use_signal(StudioApp::new);
    let mut running = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    let state = studio.read().state().clone();
    let is_running = *running.read();
    let error_text = error.read().clone();
    let start_demo = move |_| {
        if *running.read() {
            return;
        }
        running.set(true);
        error.set(None);
        spawn(async move {
            match run_browser_worker_demo(WORKER_URL).await {
                Ok(app) => studio.set(app),
                Err(runtime_error) => error.set(Some(runtime_error.to_string())),
            }
            running.set(false);
        });
    };

    rsx! {
        style { "{STYLE}" }
        main { class: "studio-shell",
            StatusBar { state: state.clone(), running: is_running, error: error_text.clone() }
            section { class: "studio-grid",
                DevicePanel { state: state.clone(), running: is_running, on_start_demo: start_demo }
                ProjectPanel { state: state.clone() }
                InventoryView { state: state.clone() }
                LogPanel { state }
            }
        }
    }
}
