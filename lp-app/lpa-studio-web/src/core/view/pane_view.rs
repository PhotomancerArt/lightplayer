use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPaneView};

use crate::app::PaneFrame;
use crate::core::{ActionStrip, ViewContent};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PaneView(
    view: UiPaneView,
    primary: bool,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let UiPaneView {
        title,
        status,
        body,
        actions,
        ..
    } = view;
    rsx! {
        PaneFrame {
            title,
            primary,
            status: Some(status),
            ViewContent {
                body,
                running,
                on_action,
            }
            if !actions.is_empty() {
                ActionStrip {
                    actions,
                    running,
                    on_action,
                }
            }
        }
    }
}
