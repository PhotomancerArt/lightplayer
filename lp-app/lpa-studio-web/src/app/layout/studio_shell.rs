use dioxus::prelude::*;
use lpa_studio_core::{DeviceController, UiAction, UiPaneView, UiStudioView, UiViewContent};

use crate::app::{ProjectNodeWorkspace, RuntimeLog};
use crate::core::PaneView;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioShell(
    view: UiStudioView,
    running: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let UiStudioView { panes, logs } = view;
    let PaneGroups { main, device } = group_panes(panes);
    let project_editor = project_editor_view(&main);
    let layout_class = if project_editor.is_some() {
        "tw:grid tw:grid-cols-[minmax(220px,280px)_minmax(0,1fr)_minmax(300px,360px)] tw:gap-3.5 tw:max-[960px]:grid-cols-1"
    } else if main.is_empty() {
        "tw:grid tw:grid-cols-1 tw:gap-3.5"
    } else {
        "tw:grid tw:grid-cols-[minmax(0,1fr)_minmax(300px,380px)] tw:gap-3.5 tw:max-[880px]:grid-cols-1"
    };
    let device_is_primary = main.is_empty();

    rsx! {
        main { class: "tw:mx-auto tw:min-h-screen tw:w-[min(1520px,100%)] tw:px-7 tw:pb-16 tw:pt-7 tw:max-[880px]:px-[18px] tw:max-[880px]:pb-[72px] tw:max-[880px]:pt-[18px]",
            header { class: "tw:mb-[18px] tw:flex tw:items-center tw:justify-start tw:gap-5",
                div {
                    p { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:text-heading", "LightPlayer Studio" }
                }
            }

            section { class: "{layout_class}",
                if let Some(project_editor) = project_editor {
                    div { class: "tw:order-1 tw:grid tw:min-w-0 tw:content-start tw:gap-3.5 tw:max-[960px]:order-2",
                        for (index, pane) in main.into_iter().enumerate() {
                            PaneView {
                                key: "{pane.node_id}",
                                view: pane,
                                primary: index == 0,
                                running,
                                on_action,
                            }
                        }
                    }
                    div { class: "tw:order-2 tw:grid tw:min-w-0 tw:content-start tw:gap-3.5 tw:max-[960px]:order-1",
                        ProjectNodeWorkspace { view: project_editor }
                    }
                } else if !main.is_empty() {
                    div { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-3.5",
                        for (index, pane) in main.into_iter().enumerate() {
                            PaneView {
                                key: "{pane.node_id}",
                                view: pane,
                                primary: index == 0,
                                running,
                                on_action,
                            }
                        }
                    }
                }

                div { class: "tw:order-3 tw:grid tw:min-w-0 tw:content-start tw:gap-3.5",
                    if let Some(device) = device {
                        PaneView {
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
        if pane.node_id.as_str() == DeviceController::NODE_ID {
            device = Some(pane);
        } else {
            main.push(pane);
        }
    }
    PaneGroups { main, device }
}

fn project_editor_view(panes: &[UiPaneView]) -> Option<lpa_studio_core::ProjectEditorView> {
    panes.iter().find_map(|pane| match &pane.body {
        UiViewContent::ProjectEditor(editor) => Some((**editor).clone()),
        _ => None,
    })
}
