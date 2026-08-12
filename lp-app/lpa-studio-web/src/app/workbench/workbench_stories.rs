//! Workbench chrome stories: the PanelDock frame with the re-housed
//! project pane, workspace, and device card. Fixture data is the shared
//! project-ready view; these pin the CHROME (strips, docks, tabs,
//! placeholder center) — panel content keeps its own stories.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use super::{DockState, PanelMemory, WorkbenchFrame, WorkbenchView};
use crate::app::StudioShell;
use crate::app::story_fixtures::{
    project_editor_fixture, project_ready_view, project_synced_pane_view, simulator_lens_card,
};
use crate::router::ProjectView;
use lpa_studio_core::ProjectSyncPhase;

/// Through the shell, like production: route-view in, workbench out.
fn workbench_story(project_view: ProjectView) -> Element {
    rsx! {
        div { class: "tw:flex tw:h-[720px] tw:flex-col",
            StudioShell {
                view: project_ready_view(),
                running: true,
                project_view,
                workbench_hrefs: Some(("#".to_string(), Some("#".to_string()))),
                on_action: move |_| {},
            }
        }
    }
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
