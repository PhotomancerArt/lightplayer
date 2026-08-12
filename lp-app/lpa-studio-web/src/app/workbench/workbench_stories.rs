//! Workbench chrome stories: the PanelDock frame with the re-housed
//! project pane, workspace, and device card. Fixture data is the shared
//! project-ready view; these pin the CHROME (strips, docks, tabs,
//! placeholder center) — panel content keeps its own stories.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use super::panels::{FixturesPanel, OutputsPanel};
use super::{DockState, PanelMemory, WorkbenchFrame, WorkbenchView};
use crate::app::StudioShell;
use crate::app::module::module_fixtures::root_module_node_view;
use crate::app::node::NodeDetailPopover;
use crate::app::patch::patch_surface_stories::{mini_dome_surface, peach_surface};
use crate::app::project::ProjectDetailContent;
use crate::app::story_fixtures::{
    project_editor_fixture, project_ready_view, project_synced_pane_view, simulator_lens_card,
};
use crate::router::ProjectView;
use lpa_studio_core::{
    NodeId, ProjectSyncPhase, UiPatchSurface, UiPatchTarget, UiStatus, UiStudioView, UiViewContent,
};

/// Stamp port/output labels onto every cell by id join — what
/// `build_patch_surface` does in production; the shared patch-story
/// builders leave them empty (the interim page only tooltips them, but
/// the panels' chips RENDER them).
fn labelled(mut surface: UiPatchSurface) -> UiPatchSurface {
    let mut labels = std::collections::BTreeMap::new();
    for output in &surface.outputs {
        let output_label = output.display_name().to_string();
        for port in &output.bay.ports {
            for cell in &port.cells {
                labels.insert(
                    cell.id.clone(),
                    (port.pin_label.clone(), output_label.clone()),
                );
            }
        }
    }
    for output in &mut surface.outputs {
        for port in &mut output.bay.ports {
            for cell in &mut port.cells {
                if let Some((pin, out)) = labels.get(&cell.id) {
                    cell.port_label = pin.clone();
                    cell.output_label = out.clone();
                }
            }
        }
    }
    for fixture in &mut surface.fixtures {
        for cell in &mut fixture.patch.cells {
            if let Some((pin, out)) = labels.get(&cell.id) {
                cell.port_label = pin.clone();
                cell.output_label = out.clone();
            }
        }
    }
    surface
}

/// The ready-project studio view with the mini-dome patch surface on its
/// editor pane, so the workbench's Fixtures/Outputs docks render real.
fn view_with_surface(selection: Option<UiPatchTarget>) -> UiStudioView {
    let mut view = project_ready_view();
    for pane in &mut view.panes {
        if let UiViewContent::ProjectEditor(editor) = &mut pane.body {
            editor.patch_surface = Some(labelled(mini_dome_surface(false)));
            editor.patch_selection = selection.clone();
        }
    }
    view
}

/// Through the shell, like production: route-view in, workbench out.
fn workbench_story(project_view: ProjectView) -> Element {
    let selection = matches!(project_view, ProjectView::Mapping).then(|| UiPatchTarget::Instance {
        node: NodeId::new(2),
        path: "/sector/2".to_string(),
    });
    rsx! {
        div { class: "tw:flex tw:h-[720px] tw:flex-col",
            StudioShell {
                view: view_with_surface(selection),
                running: true,
                project_view,
                workbench_hrefs: Some(("#".to_string(), Some("#".to_string()))),
                on_action: move |_| {},
            }
        }
    }
}

/// One panel at dock width — the chrome around it is the frame stories'
/// job; these pin the panel bodies at density.
fn dock_frame(body: Element) -> Element {
    rsx! {
        div { class: "tw:flex tw:h-[520px] tw:w-[300px] tw:flex-col tw:overflow-y-auto tw:rounded-md tw:border tw:border-border-strong tw:bg-card-subtle tw:p-2.5",
            {body}
        }
    }
}

#[story(
    description = "The Fixtures panel on the mini-dome: fixture rows with object-colour swatches, instance rows with mapped dots and port-named channel chips (text + position — the one colour language leaves ports uncoloured). Sector 2 selected."
)]
fn fixtures_panel_mini_dome() -> Element {
    dock_frame(rsx! {
        FixturesPanel {
            surface: Some(labelled(mini_dome_surface(false))),
            selection: Some(UiPatchTarget::Instance {
                node: NodeId::new(2),
                path: "/sector/2".to_string(),
            }),
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "The Fixtures panel at range grain (the peach): no instance rows — one honest 0..N range row per fixture with its wire-window chips, the reversed half wearing ‹rev."
)]
fn fixtures_panel_peach_range() -> Element {
    dock_frame(rsx! {
        FixturesPanel {
            surface: Some(labelled(peach_surface())),
            selection: None,
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "The Outputs panel on the mini-dome: every box open at once (the default) as a flat stack of slim header rows — chevron · name · occupancy — with one-line ports and neutral producer-labelled wire-window cells indented under each. A header press collapses just its own box. The selected cell wears the selection blue."
)]
fn outputs_panel_mini_dome() -> Element {
    dock_frame(rsx! {
        OutputsPanel {
            surface: Some(labelled(mini_dome_surface(false))),
            selection: Some(UiPatchTarget::Cell {
                id: "dome:0:60:0".to_string(),
            }),
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "The workbench's Nodes view: both docks open, so each wears its own TAB ROW (Nodes · Fixtures left, Device · Outputs right) instead of an edge strip — the active tab is the open panel, and pressing it collapses the side. The project pane renders FLAT in the left dock (no card, no header, no [i] — that popup moved to the root node card's ⓘ in the center), and the center's view tabs sit visibly heavier than the dock tabs above it."
)]
fn workbench_nodes_view() -> Element {
    workbench_story(ProjectView::Workspace)
}

#[story(
    description = "The workbench's Mapping view: the honest placeholder center (the arrange canvas is the unified-editor plan's mount), with the view's default docks — Fixtures left, Outputs right — each under its own tab row."
)]
fn workbench_mapping_view() -> Element {
    workbench_story(ProjectView::Mapping)
}

#[story(
    description = "The mobile fold with a panel summoned: below the fold breakpoint the summon strip carries the view switch plus the four panel toggles (the edge strips, folded), and the summoned Outputs panel replaces the main view under a back header. The sm capture is the point — at lg the same mount shows the desktop docks."
)]
fn workbench_mobile_outputs_summoned() -> Element {
    rsx! {
        div { class: "tw:flex tw:h-[640px] tw:flex-col",
            WorkbenchFrame {
                view: WorkbenchView::Mapping,
                panes: vec![project_synced_pane_view()],
                project_editor: project_editor_fixture(ProjectSyncPhase::Ready)
                    .with_patch_surface(
                        Some(labelled(mini_dome_surface(false))),
                        Some(UiPatchTarget::Instance {
                            node: NodeId::new(2),
                            path: "/sector/2".to_string(),
                        }),
                    ),
                lens_card: Some(simulator_lens_card()),
                running: true,
                workspace_href: "#".to_string(),
                mapping_href: Some("#".to_string()),
                initial_summoned: Some(super::PanelId::Outputs),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "Both docks collapsed (a press on the active dock tab): the vertical edge strips are all that remain of the sides — the collapsed state's handle — and the center takes the full width. A strip button expands that panel, and the strip is replaced by the dock's tab row."
)]
fn workbench_docks_collapsed() -> Element {
    workbench_memory_story(PanelMemory {
        nodes: DockState {
            left: None,
            right: None,
        },
        mapping: DockState {
            left: None,
            right: None,
        },
    })
}

#[story(
    description = "The two side treatments in one frame: the left side collapsed to its vertical strip, the right side open under its Device · Outputs tab row. The comparison is the point — a side is named EITHER by a strip or by tabs, never both, and the dock tabs stay lighter than the center's view tabs."
)]
fn workbench_mixed_dock_states() -> Element {
    workbench_memory_story(PanelMemory {
        nodes: DockState {
            left: None,
            right: Some(super::PanelId::Device),
        },
        mapping: DockState {
            left: None,
            right: Some(super::PanelId::Outputs),
        },
    })
}

#[story(
    description = "The re-housed project popup (ruling 2): the workspace ROOT card's ⓘ now carries what the project pane's header used to — project identity with the status word, the Project settings rows, Share, pending edits, and the project stats — because the workbench's Nodes dock renders the project pane flat, with no header to hang a popup from. Same component, same ops; the pane's own [i] is untouched on every other route."
)]
fn workbench_root_card_project_popup() -> Element {
    let editor = project_editor_fixture(ProjectSyncPhase::Ready);
    let root = root_module_node_view();
    rsx! {
        div { class: "tw:flex tw:min-h-[760px] tw:items-start tw:justify-end",
            NodeDetailPopover {
                header: root.header,
                pending_edits: editor.pending_edits.clone(),
                project: Some(ProjectDetailContent::new(&editor, UiStatus::good("Ready"))),
                on_action: move |_| {},
                initially_open: true,
            }
        }
    }
}

/// The Nodes-view frame with preset dock memory — the strip/tab stories'
/// shared mount.
fn workbench_memory_story(memory: PanelMemory) -> Element {
    rsx! {
        div { class: "tw:flex tw:h-[560px] tw:flex-col",
            WorkbenchFrame {
                view: WorkbenchView::Nodes,
                panes: vec![project_synced_pane_view()],
                project_editor: project_editor_fixture(ProjectSyncPhase::Ready),
                lens_card: Some(simulator_lens_card()),
                running: true,
                workspace_href: "#".to_string(),
                mapping_href: Some("#".to_string()),
                initial_memory: Some(memory),
                on_action: move |_| {},
            }
        }
    }
}
