//! Workbench chrome stories: the PanelDock frame with the re-housed
//! project pane, workspace, and device card. Fixture data is the shared
//! project-ready view; these pin the CHROME (band, docks, tabs, the fold's
//! summon strip) — panel content keeps its own stories.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use super::panels::OutputsPanel;
use super::workbench_story_surfaces::small_dome_surface;
use super::{DockState, PanelMemory, WorkbenchFrame, WorkbenchHrefs, WorkbenchView};
use crate::app::StudioShell;
use crate::app::story_fixtures::{
    project_editor_fixture, project_ready_view, project_synced_pane_view, simulator_lens_card,
    simulator_lens_offers,
};
use crate::router::ProjectView;
use lpa_studio_core::{ProjectSyncPhase, UiPatchTarget, UiStudioView, UiViewContent};

/// The ready-project studio view with the small-dome patch surface on its
/// editor pane, so the workbench's Outputs dock renders real.
fn view_with_surface() -> UiStudioView {
    let mut view = project_ready_view();
    for pane in &mut view.panes {
        if let UiViewContent::ProjectEditor(editor) = &mut pane.body {
            editor.patch_surface = Some(small_dome_surface());
        }
    }
    // `StudioShell` always re-publishes the offer context from
    // `view.offers` (never an ancestor's), so the docked lens card needs
    // its verbs folded in here (devices-as-offers) or it draws with none.
    if view.lens_card.is_some() {
        view.offers = simulator_lens_offers();
    }
    view
}

/// The lens device's offer tree, the way `web_app.rs` provides it in
/// production (devices-as-offers): every bare `WorkbenchFrame` mount in
/// this file docks `simulator_lens_card()` as its runtime surface, so a
/// card drawn inside `body` needs the same tree — without it the card
/// draws no buttons. (A `StudioShell` mount instead folds
/// this into its `UiStudioView::offers`, in [`view_with_surface`]: wrapping
/// the shell from outside has no effect, since it always re-publishes its
/// own.)
fn with_lens_offers(body: Element) -> Element {
    let offers = simulator_lens_offers();
    rsx! {
        crate::core::OffersProvider { offers, {body} }
    }
}

/// Through the shell, like production: route-view in, workbench out.
fn workbench_story(project_view: ProjectView) -> Element {
    rsx! {
        div { class: "tw:flex tw:h-[720px] tw:flex-col",
            StudioShell {
                view: view_with_surface(),
                running: true,
                project_view,
                workbench_hrefs: Some(WorkbenchHrefs::inert_all()),
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
    description = "The Outputs panel on the small-dome: every box open at once (the default) as a flat stack of slim header rows — chevron · name · occupancy — with one-line ports and neutral producer-labelled wire-window cells indented under each. A header press collapses just its own box. The selected cell wears the selection blue."
)]
fn outputs_panel_small_dome() -> Element {
    dock_frame(rsx! {
        OutputsPanel {
            surface: Some(small_dome_surface()),
            selection: lpa_studio_core::UiSelection::one(UiPatchTarget::Cell {
                id: "dome:0:60:0".to_string(),
            }),
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "The workbench's Nodes view under the ONE band (D7): the Tree's attached tab over the left dock, Nodes · Graph centered, Device · Outputs over the right — the active panel tab shares its dock's fill and breaks the band's bottom hairline, so it reads as the panel's own. The Tree carries the embedded project tree (the Debug chip only, while overrides are set); Save/Revert and the project popup moved to the header session·project control (single-session policy, P4 retirement)."
)]
fn workbench_nodes_view() -> Element {
    workbench_story(ProjectView::Workspace)
}

#[story(
    description = "The project page while the board's link is away: the same Nodes view, untouched and unmoved, under a dim curtain that holds it still, with the Reconnecting card floating at the top. It comes for a link that stalls, resets (plan D13) or drops and is expected back (defect 2026-10-02: a Bluetooth drop or a re-seated cable used to send the page to Devices), and goes the moment the board is back. A board that does not come back within the grace still closes the editor as before."
)]
fn workbench_link_reconnecting() -> Element {
    let view = view_with_surface().with_lens_reconnecting(Some(
        lpa_studio_core::UiLensReconnecting::link_lost("Porch sign"),
    ));
    rsx! {
        div { class: "tw:flex tw:h-[720px] tw:flex-col",
            StudioShell {
                view,
                running: true,
                project_view: ProjectView::Workspace,
                workbench_hrefs: Some(WorkbenchHrefs::inert_all()),
                on_action: move |_| {},
            }
        }
    }
}

#[story(
    description = "The fold (≤820px — the G1 ruling moved it down from 960 so md widths keep real docks) with a panel summoned on the Nodes view: the summon strip carries the view switch plus the view's ROSTERED panel toggles. At sm the summoned Outputs panel replaces the main view under a back header (phone); at md it is the tablet sheet — dock width, anchored to its home side (right), dock fill + hairline + shadow, the node workspace live beside it, dismissed by ✕. At lg the same mount shows the band and docks."
)]
fn workbench_mobile_outputs_summoned() -> Element {
    with_lens_offers(rsx! {
        div { class: "tw:flex tw:h-[640px] tw:flex-col",
            WorkbenchFrame {
                view: WorkbenchView::Nodes,
                panes: vec![project_synced_pane_view()],
                project_editor: project_editor_fixture(ProjectSyncPhase::Ready)
                    .with_patch_surface(
                        Some(small_dome_surface()),
                        lpa_studio_core::UiSelection::empty(),
                    ),
                lens_card: Some(simulator_lens_card()),
                running: true,
                hrefs: WorkbenchHrefs::inert_all(),
                initial_summoned: Some(super::PanelId::Outputs),
                on_action: move |_| {},
            }
        }
    })
}

#[story(
    description = "The tablet sheet's LEFT anchor: the Tree summoned on the Nodes view at md — the sheet slides from the panel's home side (Tree lives in the left dock), so the project tree comes up on the left at its dock's width with the node workspace beside it, mirroring the right-side sheet's grammar. At sm the same summon is the full phone replace; at lg the ordinary docks."
)]
fn workbench_tablet_tree_summoned() -> Element {
    with_lens_offers(rsx! {
        div { class: "tw:flex tw:h-[640px] tw:flex-col",
            WorkbenchFrame {
                view: WorkbenchView::Nodes,
                panes: vec![project_synced_pane_view()],
                project_editor: project_editor_fixture(ProjectSyncPhase::Ready),
                lens_card: Some(simulator_lens_card()),
                running: true,
                hrefs: WorkbenchHrefs::inert_all(),
                initial_summoned: Some(super::PanelId::Tree),
                on_action: move |_| {},
            }
        }
    })
}

#[story(
    description = "Both docks collapsed (a press on each active band tab): the tab rows PERSIST on the band with no active tab — the persistent row IS the reopen affordance (D11; edge strips and chevrons are gone) — and the center takes the full width under an unbroken band hairline."
)]
fn workbench_docks_collapsed() -> Element {
    workbench_memory_story(PanelMemory::default().with(
        WorkbenchView::Nodes,
        DockState {
            left: None,
            right: None,
        },
    ))
}

#[story(
    description = "The two side treatments in one band: the left side collapsed (its TREE tab persists, inactive, with the band hairline running unbroken beneath it), the right side open (Device's attached tab sharing the dock fill). The comparison is the point — one grammar names both states, and the panel tabs stay lighter than the view tabs."
)]
fn workbench_mixed_dock_states() -> Element {
    workbench_memory_story(PanelMemory::default().with(
        WorkbenchView::Nodes,
        DockState {
            left: None,
            right: Some(super::PanelId::Device),
        },
    ))
}

/// The Nodes-view frame with preset dock memory — the strip/tab stories'
/// shared mount.
fn workbench_memory_story(memory: PanelMemory) -> Element {
    with_lens_offers(rsx! {
        div { class: "tw:flex tw:h-[560px] tw:flex-col",
            WorkbenchFrame {
                view: WorkbenchView::Nodes,
                panes: vec![project_synced_pane_view()],
                project_editor: project_editor_fixture(ProjectSyncPhase::Ready),
                lens_card: Some(simulator_lens_card()),
                running: true,
                hrefs: WorkbenchHrefs::inert_all(),
                initial_memory: Some(memory),
                on_action: move |_| {},
            }
        }
    })
}
