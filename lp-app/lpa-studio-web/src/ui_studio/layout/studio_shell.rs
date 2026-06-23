use dioxus::prelude::*;
use lpa_studio_ux::{DeviceUx, StudioView, UiAction, UiBody, UiPaneView};

use crate::ui_core::AppPane;
use crate::ui_studio::{ProjectNodeWorkspace, RuntimeLog};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioShell(view: StudioView, running: bool, on_action: EventHandler<UiAction>) -> Element {
    let StudioView { panes, logs } = view;
    let PaneGroups { main, device } = group_panes(panes);
    let project_editor = project_editor_view(&main);
    let layout_class = if project_editor.is_some() {
        "ux-layout ux-layout-project-editor"
    } else if main.is_empty() {
        "ux-layout ux-layout-device-only"
    } else {
        "ux-layout ux-layout-main-device"
    };
    let device_is_primary = main.is_empty();

    rsx! {
        main { class: "ux-shell",
            header { class: "ux-header",
                div {
                    p { class: "ux-eyebrow", "LightPlayer Studio" }
                }
            }

            section { class: "{layout_class}",
                if let Some(project_editor) = project_editor {
                    div { class: "ux-project-sidebar-column",
                        for (index, pane) in main.into_iter().enumerate() {
                            AppPane {
                                key: "{pane.node_id}",
                                view: pane,
                                primary: index == 0,
                                running,
                                on_action,
                            }
                        }
                    }
                    div { class: "ux-editor-center-column",
                        ProjectNodeWorkspace { view: project_editor }
                    }
                } else if !main.is_empty() {
                    div { class: "ux-main-column",
                        for (index, pane) in main.into_iter().enumerate() {
                            AppPane {
                                key: "{pane.node_id}",
                                view: pane,
                                primary: index == 0,
                                running,
                                on_action,
                            }
                        }
                    }
                }

                div { class: "ux-device-column",
                    if let Some(device) = device {
                        AppPane {
                            key: "{device.node_id}",
                            view: device,
                            primary: device_is_primary,
                            running,
                            on_action,
                        }
                    }
                    RuntimeLog { logs }
                }
            }
        }
    }
}

struct PaneGroups {
    main: Vec<UiPaneView>,
    device: Option<UiPaneView>,
}

fn group_panes(panes: Vec<UiPaneView>) -> PaneGroups {
    let mut main = Vec::new();
    let mut device = None;
    for pane in panes {
        if pane.node_id.as_str() == DeviceUx::NODE_ID {
            device = Some(pane);
        } else {
            main.push(pane);
        }
    }
    PaneGroups { main, device }
}

fn project_editor_view(panes: &[UiPaneView]) -> Option<lpa_studio_ux::ProjectEditorView> {
    panes.iter().find_map(|pane| match &pane.body {
        UiBody::ProjectEditor(editor) => Some((**editor).clone()),
        _ => None,
    })
}
