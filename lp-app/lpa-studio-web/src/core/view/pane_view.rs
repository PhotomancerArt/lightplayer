use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPaneView, UiViewContent};

use crate::app::{PaneFrame, ProjectPane};
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
    match body {
        // The project editor is one StudioPane carrying the pane's status in
        // its own header — no PaneFrame (no second header) and no pane-level
        // button strip (P6 sidebar tidy: a ready project produces no pane
        // actions; recovery states render through the generic branch below).
        UiViewContent::ProjectEditor(editor) => rsx! {
            ProjectPane {
                view: *editor,
                status,
                running,
                on_action,
            }
        },
        body => rsx! {
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
        },
    }
}
