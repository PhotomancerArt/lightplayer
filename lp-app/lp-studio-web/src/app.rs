use dioxus::prelude::*;
use lp_studio_core::{ActionOrigin, StudioActionKind, StudioApp};
use lpa_link::LinkProviderId;

use crate::components::device_panel::DevicePanel;
use crate::components::inventory_view::InventoryView;
use crate::components::log_panel::LogPanel;
use crate::components::project_panel::ProjectPanel;
use crate::components::status_bar::StatusBar;
use crate::web_provisioning_controller::{
    WebProvisioningController, auto_advance_web_flow, dispatch_web_action,
};

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

    let studio = use_signal(StudioApp::new);
    let controller = use_signal(|| WebProvisioningController::new(WORKER_URL));
    let mut running = use_signal(|| false);
    use_future(move || async move {
        running.set(true);
        dispatch_web_action(
            studio,
            controller,
            StudioActionKind::RefreshProviderCatalog,
            ActionOrigin::System,
        )
        .await;
        running.set(false);
    });

    let state = studio.read().state().clone();
    let is_running = *running.read();
    let error_text = controller.read().error().map(str::to_string);
    let refresh_catalog = move |_| {
        if *running.read() {
            return;
        }
        running.set(true);
        spawn(async move {
            dispatch_web_action(
                studio,
                controller,
                StudioActionKind::RefreshProviderCatalog,
                ActionOrigin::System,
            )
            .await;
            running.set(false);
        });
    };
    let start_provider = move |provider_id: LinkProviderId| {
        if *running.read() {
            return;
        }
        running.set(true);
        spawn(async move {
            dispatch_web_action(
                studio,
                controller,
                StudioActionKind::StartProvisioning { provider_id },
                ActionOrigin::User,
            )
            .await;
            auto_advance_web_flow(studio, controller).await;
            running.set(false);
        });
    };
    let load_starter_project = move |_| {
        if *running.read() {
            return;
        }
        running.set(true);
        spawn(async move {
            dispatch_web_action(
                studio,
                controller,
                StudioActionKind::LoadDemoProject,
                ActionOrigin::User,
            )
            .await;
            running.set(false);
        });
    };

    rsx! {
        style { "{STYLE}" }
        main { class: "studio-shell",
            StatusBar { state: state.clone(), running: is_running, error: error_text.clone() }
            section { class: "studio-grid",
                DevicePanel {
                    state: state.clone(),
                    running: is_running,
                    on_refresh_catalog: refresh_catalog,
                    on_start_provider: start_provider,
                    on_load_starter_project: load_starter_project,
                }
                ProjectPanel { state: state.clone() }
                InventoryView { state: state.clone() }
                LogPanel { state }
            }
        }
    }
}
