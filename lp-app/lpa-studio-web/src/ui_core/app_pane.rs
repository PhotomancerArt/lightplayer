use dioxus::prelude::*;
use lpa_studio_ux::{UiAction, UiPaneView};

use crate::ui_core::{ActionStrip, AppBody};
use crate::ui_studio::PaneFrame;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn AppPane(
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
            AppBody {
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
