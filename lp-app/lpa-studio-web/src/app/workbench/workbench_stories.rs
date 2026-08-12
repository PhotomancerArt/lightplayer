//! Workbench chrome stories: the PanelDock frame with the re-housed
//! project pane, workspace, and device card. Fixture data is the shared
//! project-ready view; these pin the CHROME (strips, docks, tabs,
//! placeholder center) — panel content keeps its own stories.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use super::panels::{FixturesPanel, OutputsPanel};
use super::{DockState, PanelMemory, WorkbenchFrame, WorkbenchView};
use crate::app::StudioShell;
use crate::app::patch::patch_surface_stories::{mini_dome_surface, peach_surface};
use crate::app::story_fixtures::{
    project_editor_fixture, project_ready_view, project_synced_pane_view, simulator_lens_card,
};
use crate::router::ProjectView;
use lpa_studio_core::{
    NodeId, ProjectSyncPhase, UiPatchSurface, UiPatchTarget, UiStudioView, UiViewContent,
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
    description = "The Outputs panel on the mini-dome: box 1 expanded (one at a time — the radiance rule), one-line ports with occupancy, neutral producer-labelled wire-window cells; box 2 collapsed to its occupancy row. The selected cell wears the selection blue."
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
    description = "The workbench's Nodes view: edge strips flanking the frame, the project pane docked left as the Nodes panel, the node workspace in the center under the view tabs, and the device card docked right. The default panel set for this view."
)]
fn workbench_nodes_view() -> Element {
    workbench_story(ProjectView::Workspace)
}

#[story(
    description = "The workbench's Mapping view: the honest placeholder center (the arrange canvas is the unified-editor plan's mount), with the view's default docks — Fixtures left, Outputs right — still placeholder bodies until the panels land."
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
    description = "Both docks collapsed (radio re-click): the edge strips are all that remain of the sides, and the center takes the full width. The strips stay clickable — the collapsed state's handle."
)]
fn workbench_docks_collapsed() -> Element {
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
                initial_memory: Some(PanelMemory {
                    nodes: DockState {
                        left: None,
                        right: None,
                    },
                    mapping: DockState {
                        left: None,
                        right: None,
                    },
                }),
                on_action: move |_| {},
            }
        }
    }
}
