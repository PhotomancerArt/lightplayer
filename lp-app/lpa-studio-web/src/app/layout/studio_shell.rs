use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiNodeFace, UiPaneView, UiStudioView, UiViewContent};

use crate::app::module::{PlayModeSurface, panel_gesture_actions};
use crate::app::workbench::{WorkbenchFrame, WorkbenchHrefs, view_for_route};
use crate::app::{DevicesPage, ProjectOpeningFrame, ProjectsPage};
use crate::core::PaneView;
use crate::router::ProjectView;

/// Which gallery page the shell renders when the view has no open
/// editor (P09 split): the route picks it — `#/` = Devices,
/// `#/projects` = Projects. Lens routes leave the default; they only
/// see the gallery in transient detach windows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShellGallery {
    #[default]
    Devices,
    Projects,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StudioShell(
    view: UiStudioView,
    running: bool,
    /// Fixed clock for home-gallery stories; `None` uses the platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    /// The gallery page for no-editor renders (see [`ShellGallery`]).
    #[props(default)]
    gallery: ShellGallery,
    /// The route says a project but the view hasn't reached it yet: render
    /// the project-shaped opening frame instead of the gallery (the URL's
    /// intent picks the frame — no gallery flash on a project reload).
    #[props(default = false)]
    opening_frame: bool,
    /// Play mode (`docs/design/panel.md` P12): the root module's panel and
    /// nothing else — no pane column, no workspace, no device card. Set by
    /// the `/play` route suffix; a session whose root wears no module face
    /// falls through to the normal layout rather than showing an empty
    /// surface.
    #[props(default = false)]
    play: bool,
    /// Which project view the route addresses (the `/patch` and
    /// `/mapping` suffixes; `Workspace` is the suffix-less Nodes view).
    /// Play arrives through the separate `play` flag because a DEVICE
    /// lens has a play zoom too but no project-view suffixes.
    #[props(default)]
    project_view: ProjectView,
    /// The workbench view tabs' hrefs, one slot per view-table row; a
    /// `None` slot hides its tab (a device lens has no mapping address
    /// yet). Stories default to inert fragments.
    #[props(default)]
    workbench_hrefs: Option<WorkbenchHrefs>,
    on_action: EventHandler<UiAction>,
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
        // consumed by the web shell's URL sync (transient sessions stay on
        // their bare example address) and the leave-discards-work confirm
        open_project_transient: _,
        open_transient_example: _,
        transient_fork_generation: _,
        // the chrome renders the header session·project control (web_app
        // builds it from the editor pane's own view and this field)
        open_project_name: _,
        lens_card,
        // the header session·project control's session, for the chrome
        session: _,
        // the chrome renders the settings surface (web_app owns both)
        settings: _,
        // consumed by the web shell's unload gate; the project pane
        // computes its own dirty affordances from the editor view
        dirty: _,
    } = view;

    if opening_frame && panes.is_empty() {
        // The frame polls the open pipeline itself (its module explains
        // why); all it needs from here is somewhere for Retry to go.
        return rsx! {
            div { class: "tw:grid tw:gap-7", ProjectOpeningFrame { on_action } }
        };
    }

    if let Some(home) = home {
        return match gallery {
            ShellGallery::Devices => rsx! {
                div { class: "tw:grid tw:gap-7",
                    DevicesPage { home: *home, on_action }
                }
            },
            ShellGallery::Projects => rsx! {
                div { class: "tw:grid tw:gap-7",
                    ProjectsPage { home: *home, now_secs, on_action }
                }
            },
        };
    }

    let main = panes;
    let project_editor = project_editor_view(&main);

    // Play mode: the root module's panel, full width, single column (P12).
    // It renders panels ONLY — no faces, no children, no wiring, no device
    // card — so it deliberately short-circuits the whole editor layout
    // below. `PlayModeSurface` wraps its own controls, which is what makes
    // the same mount usable on a phone.
    // (Patch no longer short-circuits: it is a workbench view — the R5
    // patching pass. The interim full-page surface is retired.)
    if play && let Some(face) = play_mode_face(project_editor.as_ref()) {
        return rsx! {
            div { class: "tw:grid tw:min-w-0 tw:grid-cols-1",
                PlayModeSurface {
                    panel: face.panel,
                    preview: face.preview,
                    auto_save: face.auto_save,
                    on_panel: panel_gesture_actions(on_action),
                    on_action,
                }
            }
        };
    }

    // The workbench (PanelDock chrome): every project-editor render in a
    // workbench view. Play returned above; the states below
    // (no editor yet, bare panes) keep the legacy grid.
    if let Some(project_editor) = project_editor {
        let view = view_for_route(project_view);
        // Stories mount without a route; inert fragment hrefs keep the
        // tabs drawable without navigation.
        let hrefs = workbench_hrefs.unwrap_or_else(WorkbenchHrefs::inert_default);
        return rsx! {
            WorkbenchFrame {
                view,
                panes: main,
                project_editor,
                lens_card: lens_card.map(|card| *card),
                running,
                now_secs,
                hrefs,
                on_action,
            }
        };
    }

    let layout_class = if main.is_empty() {
        "tw:grid tw:grid-cols-1 tw:gap-3.5"
    } else {
        "tw:grid tw:grid-cols-[minmax(0,1fr)_minmax(300px,380px)] tw:gap-3.5 tw:max-[880px]:grid-cols-1"
    };
    rsx! {
        section { class: "{layout_class}",
            if !main.is_empty() {
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
                    // editor's ONLY runtime surface. It is present
                    // whenever panes render (pinned in core by
                    // `panes_never_render_without_a_lens_card`). The
                    // retired step-stack device pane that used to
                    // backstop this branch is gone.
                    crate::app::home::sim_card::SimCard {
                        pane: true,
                        card: *card,
                        on_action,
                    }
                }
            }
        }
    }
}

fn project_editor_view(panes: &[UiPaneView]) -> Option<lpa_studio_core::ProjectEditorView> {
    panes.iter().find_map(|pane| match &pane.body {
        UiViewContent::ProjectEditor(editor) => Some((**editor).clone()),
        _ => None,
    })
}

/// The module face play mode renders: the workspace ROOT's, since the flat
/// root is the single top-level card and its scope is the project's own
/// (R8). Anything else — no editor yet, or a root wearing another kind's
/// face — has no play surface, and the caller falls back.
fn play_mode_face(
    editor: Option<&lpa_studio_core::ProjectEditorView>,
) -> Option<lpa_studio_core::UiModuleFace> {
    match editor?.nodes.first()?.face.as_ref()? {
        UiNodeFace::Module(face) => Some(face.clone()),
        _ => None,
    }
}
