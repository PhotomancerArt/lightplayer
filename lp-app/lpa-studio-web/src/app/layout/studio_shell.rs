use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceController, SettingsCommand, UiAction, UiPaneView, UiStudioView, UiViewContent,
};

use crate::app::layout::{StudioSettingsPopover, VersionBadge};
use crate::app::{HomeGallery, ProjectNodeWorkspace, ProjectOpeningFrame};
use crate::core::PaneView;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioShell(
    view: UiStudioView,
    running: bool,
    /// Fixed clock for home-gallery stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    /// The route says a project but the view hasn't reached it yet: render
    /// the project-shaped opening frame instead of the gallery (the URL's
    /// intent picks the frame — no gallery flash on a project reload).
    #[props(default = false)]
    opening_frame: bool,
    on_action: EventHandler<UiAction>,
    on_settings: EventHandler<SettingsCommand>,
) -> Element {
    let UiStudioView {
        panes,
        // The global console UI retired with M7′ P2 (D42): device/sim
        // streams live on their cards; app-level entries keep their
        // devtools mirror (`web_app::log_to_js_console`).
        console: _,
        home,
        // consumed by the web shell's URL sync, not the layout
        lens: _,
        open_project_uid: _,
        open_project_slug: _,
        // rendered by the device pane (M5)
        device_sync: _,
        lens_card,
        settings,
        // consumed by the web shell's unload gate; the project pane
        // computes its own dirty affordances from the editor view
        dirty: _,
    } = view;

    if opening_frame && panes.is_empty() {
        return rsx! {
            main { class: "tw:mx-auto tw:min-h-screen tw:w-[min(1520px,100%)] tw:px-7 tw:pb-16 tw:pt-7 tw:max-[880px]:px-[18px] tw:max-[880px]:pb-[72px] tw:max-[880px]:pt-[18px]",
                header { class: "tw:mb-[18px] tw:flex tw:items-center tw:justify-start tw:gap-5",
                    ShellLogo { on_action }
                    VersionBadge {}
                    StudioSettingsPopover { settings, on_settings }
                }
                div { class: "tw:grid tw:gap-7", ProjectOpeningFrame {} }
            }
        };
    }

    if let Some(home) = home {
        return rsx! {
            main { class: "tw:mx-auto tw:min-h-screen tw:w-[min(1520px,100%)] tw:px-7 tw:pb-16 tw:pt-7 tw:max-[880px]:px-[18px] tw:max-[880px]:pb-[72px] tw:max-[880px]:pt-[18px]",
                header { class: "tw:mb-[18px] tw:flex tw:items-center tw:justify-start tw:gap-5",
                    ShellLogo { on_action }
                    VersionBadge {}
                    StudioSettingsPopover { settings, on_settings }
                }
                div { class: "tw:grid tw:gap-7",
                    HomeGallery { home: *home, now_secs, on_action }
                }
            }
        };
    }

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
                ShellLogo { on_action }
                VersionBadge {}
                StudioSettingsPopover { settings, on_settings }
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
                        ProjectNodeWorkspace { view: project_editor, on_action }
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
                    if let Some(card) = lens_card {
                        // D43: the LENS session's card, grown — the same
                        // control panel the gallery shows, docked as the
                        // editor's right-side pane. The old device pane
                        // surface renders only while no lens session
                        // exists (defensive: the editor implies a lens).
                        crate::app::home::device_card::DeviceCard {
                            sim: card.sim,
                            pane: true,
                            card: *card,
                            now_secs,
                            on_action,
                        }
                    } else if let Some(device) = device {
                        PaneView {
                            key: "{device.node_id}",
                            view: device,
                            primary: device_is_primary,
                            running,
                            on_action,
                        }
                    }
                }
            }
        }
    }
}

/// The shell wordmark; links home. Navigating to `#/` fires `hashchange`,
/// which the route listener turns into the lens detach (runtime-pool P3:
/// the editor closes, sessions keep running) — the same path as the
/// browser back button. The click ALSO dispatches the detach directly:
/// the D29 device editor lives at `#/` (no URL until M5), so a wordmark
/// click there changes no hash and the listener never fires — the direct
/// dispatch is its way home. Detaching an already-detached lens is a
/// no-op, so the doubled dispatch on project routes is harmless.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShellLogo(on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        a {
            class: "tw:text-xs tw:font-bold tw:uppercase tw:text-heading tw:no-underline tw:transition-colors tw:hover:text-strong-foreground",
            href: "#/",
            title: "Back to the gallery",
            onclick: move |_| {
                on_action.call(UiAction::from_op(
                    lpa_studio_core::ProjectController::NODE_ID,
                    lpa_studio_core::ProjectOp::DetachLens,
                ));
            },
            "LightPlayer Studio"
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
