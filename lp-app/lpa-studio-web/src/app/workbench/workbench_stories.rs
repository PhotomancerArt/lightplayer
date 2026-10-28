//! Workbench chrome stories: the PanelDock frame with the re-housed
//! project pane, workspace, and device card. Fixture data is the shared
//! project-ready view; these pin the CHROME (strips, docks, tabs,
//! placeholder center) — panel content keeps its own stories.

use dioxus::prelude::*;
use lpa_mapping_editor::{Map2dDoc, MapEditorSession, ShapePath};
use lpa_studio_web_story_macros::story;

use super::panels::{FixturesPanel, OutputsPanel, PropsPanel};
use super::workbench_story_surfaces::small_dome_surface;
use super::{DockState, PanelMemory, WorkbenchFrame, WorkbenchHrefs, WorkbenchView};
use crate::app::StudioShell;
use crate::app::editor_shell::map_canvas_sample_documents::{SMALL_DOME_DOME, SMALL_DOME_DOORS};
use crate::app::story_fixtures::{
    project_editor_fixture, project_ready_view, project_synced_pane_view, simulator_lens_card,
    simulator_lens_offers,
};
use crate::router::ProjectView;
use lpa_studio_core::{
    ArtifactLocation, NodeId, ProjectSyncPhase, UiArrangeMeta, UiArrangeTransform, UiPatchSurface,
    UiPatchTarget, UiStudioView, UiViewContent,
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

/// The small-dome surface with fixture map2d ARTIFACTS stamped (the shared
/// builder leaves them `None`), plus the bodies map the authored grain
/// reads — the Mapping tree's undived source. The bodies are the
/// small-dome's documents as they were before map2d format 6, which the
/// Map view reads no later than.
fn small_dome_with_bodies() -> (
    UiPatchSurface,
    std::rc::Rc<std::collections::BTreeMap<ArtifactLocation, String>>,
) {
    let mut surface = labelled(small_dome_surface(false));
    let dome = ArtifactLocation::file("/dome/dome.map2d.json");
    let doors = ArtifactLocation::file("/doors/doors.map2d.json");
    let mut bodies = std::collections::BTreeMap::new();
    bodies.insert(dome.clone(), SMALL_DOME_DOME.to_string());
    bodies.insert(doors.clone(), SMALL_DOME_DOORS.to_string());
    for fixture in &mut surface.fixtures {
        fixture.mapping_artifact = Some(if fixture.label == "dome" {
            dome.clone()
        } else {
            doors.clone()
        });
    }
    (surface, std::rc::Rc::new(bodies))
}

/// The ready-project studio view with the small-dome patch surface on its
/// editor pane, so the workbench's Fixtures/Outputs docks render real.
fn view_with_surface(selection: Option<UiPatchTarget>) -> UiStudioView {
    let mut view = project_ready_view();
    for pane in &mut view.panes {
        if let UiViewContent::ProjectEditor(editor) = &mut pane.body {
            editor.patch_surface = Some(labelled(small_dome_surface(false)));
            editor.patch_selection = lpa_studio_core::UiSelection::from_option(selection.clone());
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
    description = "The Tree panel UNDIVED (the Map view's tree): every fixture shows its authored structure from its loaded body — objects with repeat interiors as static rows, no wire chips. The shared patch selection /rim-a/2 highlights the rim-a object by its sticky id — the derived authored↔resolved bridge, not a second selection."
)]
fn fixtures_panel_mapping_authored() -> Element {
    let (surface, bodies) = small_dome_with_bodies();
    dock_frame(rsx! {
        FixturesPanel {
            surface: Some(surface),
            selection: lpa_studio_core::UiSelection::one(UiPatchTarget::Instance {
                node: NodeId::new(2),
                path: "/rim-a/2".to_string(),
            }),
            bodies,
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "The Tree panel while DIVED into the dome fixture (G1): the module row is the tree level above the fixtures, and the dived fixture grows its FULL shape tree — the rim-a repeat group (×5) with its inner polygon as a nested child row, the inner item selected by its exact ShapePath. Rows select through the shared editor session; the undived doors fixture shows its own authored structure statically from its body."
)]
fn fixtures_panel_dived_dome_tree() -> Element {
    let doc = dome_doc();
    let session = use_signal(move || {
        let mut session = MapEditorSession::new(doc.clone());
        // The repeat's inner path — the row the flat tree used to hide.
        session
            .selection
            .select_only_path(ShapePath::root(0).child(0));
        session
    });
    let (surface, bodies) = small_dome_with_bodies();
    dock_frame(rsx! {
        FixturesPanel {
            surface: Some(surface),
            selection: lpa_studio_core::UiSelection::empty(),
            bodies,
            dive: Some((NodeId::new(2), session)),
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "The Outputs panel on the small-dome: every box open at once (the default) as a flat stack of slim header rows — chevron · name · occupancy — with one-line ports and neutral producer-labelled wire-window cells indented under each. A header press collapses just its own box. The selected cell wears the selection blue."
)]
fn outputs_panel_small_dome() -> Element {
    dock_frame(rsx! {
        OutputsPanel {
            surface: Some(labelled(small_dome_surface(false))),
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
    let view = view_with_surface(None).with_lens_reconnecting(Some(
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
    description = "The workbench's Map view, mounted directly (its tab is hidden until M4's Graph replaces it, so no route reaches it): the SAME Tree panel shows the fixture tree (one panel, one ROLE — the view supplies the content, D10) with its summary footer pinned at the dock bottom (D12); the right roster reads Props · Outputs · Device with Props attached. The unified editor's coordinator is the center."
)]
fn workbench_mapping_view() -> Element {
    with_lens_offers(rsx! {
        div { class: "tw:flex tw:h-[720px] tw:flex-col",
            WorkbenchFrame {
                view: WorkbenchView::Mapping,
                panes: vec![project_synced_pane_view()],
                project_editor: project_editor_fixture(ProjectSyncPhase::Ready)
                    .with_patch_surface(
                        Some(labelled(small_dome_surface(false))),
                        lpa_studio_core::UiSelection::one(UiPatchTarget::Instance {
                            node: NodeId::new(2),
                            path: "/sector/2".to_string(),
                        }),
                    ),
                lens_card: Some(simulator_lens_card()),
                running: true,
                hrefs: WorkbenchHrefs::inert_all(),
                on_action: move |_| {},
            }
        }
    })
}

#[story(
    description = "The fold (≤820px — the G1 ruling moved it down from 960 so md widths keep real docks) with a panel summoned: the summon strip carries the view switch plus the view's ROSTERED panel toggles. At sm the summoned Outputs panel replaces the main view under a back header (phone); at md it is the tablet sheet — dock width, anchored to its home side (right), dock fill + hairline + shadow, the canvas live beside it, dismissed by ✕. At lg the same mount shows the band and docks."
)]
fn workbench_mobile_outputs_summoned() -> Element {
    with_lens_offers(rsx! {
        div { class: "tw:flex tw:h-[640px] tw:flex-col",
            WorkbenchFrame {
                view: WorkbenchView::Mapping,
                panes: vec![project_synced_pane_view()],
                project_editor: project_editor_fixture(ProjectSyncPhase::Ready)
                    .with_patch_surface(
                        Some(labelled(small_dome_surface(false))),
                        lpa_studio_core::UiSelection::one(UiPatchTarget::Instance {
                            node: NodeId::new(2),
                            path: "/sector/2".to_string(),
                        }),
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
    description = "The tablet sheet's LEFT anchor: the Tree summoned on the Map view at md — the sheet slides from the panel's home side (Tree lives in the left dock), so the fixture tree comes up on the left at its dock's width with the canvas beside it, mirroring the right-side sheet's grammar. At sm the same summon is the full phone replace; at lg the ordinary docks."
)]
fn workbench_tablet_tree_summoned() -> Element {
    with_lens_offers(rsx! {
        div { class: "tw:flex tw:h-[640px] tw:flex-col",
            WorkbenchFrame {
                view: WorkbenchView::Mapping,
                panes: vec![project_synced_pane_view()],
                project_editor: project_editor_fixture(ProjectSyncPhase::Ready)
                    .with_patch_surface(
                        Some(labelled(small_dome_surface(false))),
                        lpa_studio_core::UiSelection::one(UiPatchTarget::Instance {
                            node: NodeId::new(2),
                            path: "/sector/2".to_string(),
                        }),
                    ),
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
    workbench_memory_story(
        PanelMemory::default()
            .with(
                WorkbenchView::Nodes,
                DockState {
                    left: None,
                    right: None,
                },
            )
            .with(
                WorkbenchView::Mapping,
                DockState {
                    left: None,
                    right: None,
                },
            ),
    )
}

#[story(
    description = "The two side treatments in one band: the left side collapsed (its TREE tab persists, inactive, with the band hairline running unbroken beneath it), the right side open (Device's attached tab sharing the dock fill). The comparison is the point — one grammar names both states, and the panel tabs stay lighter than the view tabs."
)]
fn workbench_mixed_dock_states() -> Element {
    workbench_memory_story(
        PanelMemory::default()
            .with(
                WorkbenchView::Nodes,
                DockState {
                    left: None,
                    right: Some(super::PanelId::Device),
                },
            )
            .with(
                WorkbenchView::Mapping,
                DockState {
                    left: None,
                    right: Some(super::PanelId::Outputs),
                },
            ),
    )
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

/// The props-stack stories' surface: the small-dome with the dome fixture's
/// arrange facts stamped (address, artifact, a placed transform) so the
/// PLACEMENT card renders its editable fields rather than the unarranged
/// meta.
fn props_stack_surface() -> UiPatchSurface {
    let mut surface = labelled(small_dome_surface(false));
    surface.fixtures[0].address = Some("/dome".to_string());
    surface.fixtures[0].mapping_artifact = Some(ArtifactLocation::file("/dome/dome.map2d.json"));
    surface.fixtures[0].arrange = Some(UiArrangeMeta {
        arranged: true,
        transform: UiArrangeTransform {
            t: [12.0, 4.5],
            r: 0.0,
            s: 1.0,
        },
        footprint: None,
    });
    surface
}

/// The dome fixture's real document, as it was before map2d format 6 (the
/// same resolver the device ran).
fn dome_doc() -> Map2dDoc {
    Map2dDoc::from_json(SMALL_DOME_DOME).expect("dome parses")
}

/// The Props panel at dock width with deterministic dive state — the
/// props-stack stories' shared mount.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PropsStackStory(
    doc: Map2dDoc,
    surface: UiPatchSurface,
    #[props(default)] select: Option<ShapePath>,
    #[props(default)] multi_roots: Vec<usize>,
    #[props(default = true)] dived: bool,
    #[props(default)] selection: lpa_studio_core::UiSelection,
) -> Element {
    let session = use_signal(move || {
        let mut session = MapEditorSession::new(doc.clone());
        if let Some(path) = &select {
            session.selection.select_only_path(path.clone());
        }
        for index in &multi_roots {
            session.selection.insert_path(ShapePath::root(*index));
        }
        session
    });
    let dive_focused = dived.then(|| NodeId::new(2));
    let dive_commits = use_signal(|| 0u64);
    dock_frame(rsx! {
        PropsPanel {
            surface: Some(surface),
            selection,
            dive_focused,
            dive_session: session,
            dive_commits,
            workspace_href: "#".to_string(),
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "The props STACK (B′), dived into the dome with the rim-a repeat's inner polygon selected: the selection is the TOP card (path, selection blue), the repeat unwinds beneath it with its instances field editable in place — the edit-the-repeat-while-the-inner-item-stays-selected workflow the stack exists for — then the object-level actions on the root card, the fixture's PLACEMENT card (shell composition: editor.json x/y/rotation/scale), and the module chain as the muted context strip pointing at the Nodes view."
)]
fn props_stack_dived_descended() -> Element {
    rsx! {
        PropsStackStory {
            doc: dome_doc(),
            surface: props_stack_surface(),
            select: ShapePath::root(0).child(0),
        }
    }
}

#[story(
    description = "The props stack on a SHAPED MATRIX (filled polygon): the population toggle leads the card because it decides the fields under it, then the lattice — pitch, turn, phase origin, wiring routing and start corner — and a read-only derived count, the first shape whose lamp count is computed rather than authored."
)]
fn props_stack_filled_polygon() -> Element {
    rsx! {
        PropsStackStory {
            doc: filled_polygon_doc(),
            surface: props_stack_surface(),
            select: ShapePath::root(0),
        }
    }
}

/// A one-object document whose object is a shaped matrix — the props stack's
/// filled-polygon fixture.
fn filled_polygon_doc() -> Map2dDoc {
    Map2dDoc {
        canvas: Some([0.0, 0.0, 160.0, 160.0]),
        objects: vec![lpc_mapping::Map2dObject {
            name: "sign face".to_string(),
            id: None,
            stride: None,
            shape: lpc_mapping::Map2dShape::FilledPolygon(lpc_mapping::FilledPolygonShape {
                points: vec![
                    [10.0, 10.0],
                    [130.0, 10.0],
                    [130.0, 150.0],
                    [70.0, 100.0],
                    [10.0, 150.0],
                ],
                pitch: 18.0,
                angle_deg: 0.0,
                origin: [0.0, 0.0],
                routing: lpc_mapping::GridRouting::Snake,
                start_corner: lpc_mapping::GridCorner::Tl,
            }),
        }],
        ..Map2dDoc::new()
    }
}

#[story(
    description = "The props stack at multi-select: sibling roots share one 'N objects' leaf card (lamp total, delete all) over the placement card — shared-ancestor cards would stack between them if the siblings were descended, but sibling-level multi-select at today's arity means the fixture is the only shared level."
)]
fn props_stack_multi_select() -> Element {
    let doc = Map2dDoc::from_json(
        r#"{"format":3,"sample_diameter":2.0,"canvas":[0.0,0.0,100.0,100.0],"objects":[
            {"name":"left run","id":"a","shape":{"path":{"points":[[10.0,20.0],[90.0,20.0]],"count":24,"reversed":false,"gaps":[]}}},
            {"name":"right run","id":"b","shape":{"path":{"points":[[10.0,60.0],[90.0,60.0]],"count":24,"reversed":false,"gaps":[]}}}
        ]}"#,
    )
    .expect("story doc parses");
    rsx! {
        PropsStackStory {
            doc,
            surface: props_stack_surface(),
            multi_roots: vec![0, 1],
        }
    }
}

#[story(
    description = "The props stack with the dive's selection EMPTY: the fixture's placement card stands alone as the top card (selection blue — the level esc's clear-rung leaves selected), never the old 'select an object' placeholder. Esc from here leaves the dive."
)]
fn props_stack_empty_dived() -> Element {
    rsx! {
        PropsStackStory { doc: dome_doc(), surface: props_stack_surface() }
    }
}

#[story(
    description = "The props stack NOT dived, a fixture selected at the arranged level: the same placement card alone — editable x/y/rotation/scale committing one arrange undo step each, and the 'edit mapping' action that enters the dive — over the module context strip. One card serving both states is the point of the shell composition."
)]
fn props_stack_fixture_selected() -> Element {
    rsx! {
        PropsStackStory {
            doc: Map2dDoc::new(),
            surface: props_stack_surface(),
            dived: false,
            selection: lpa_studio_core::UiSelection::one(UiPatchTarget::Fixture { node: NodeId::new(2) }),
        }
    }
}

#[story(
    description = "The props stack with N FIXTURES selected (unified-selection P2/P3 — the canvas multi-select's panel answer): a multi selection has no one placement to edit, so the stack states the count honestly instead of showing one member's card — the gesture (drag, corner scale) acts on the whole set as one undo step, and per-member edits come from selecting one. The fixture-grain sibling class spans fixtures; the dived 'N objects' card is this same posture one level down."
)]
fn props_stack_multi_fixtures() -> Element {
    let mut selection = lpa_studio_core::UiSelection::empty();
    selection.set_siblings(vec![
        UiPatchTarget::Fixture {
            node: NodeId::new(2),
        },
        UiPatchTarget::Fixture {
            node: NodeId::new(3),
        },
    ]);
    rsx! {
        PropsStackStory {
            doc: Map2dDoc::new(),
            surface: labelled(small_dome_surface(false)),
            dived: false,
            selection,
        }
    }
}

#[story(
    description = "The props stack with a PORT selected (the Outputs panel's wire leaves, B′ deepest-first): the port's readout card on top — 1-based wire span, used/free, cell count, next free lamp — its OUTPUT card unwinding beneath, module strip at the bottom. Readout cards: the verbs act on selections, never card fields."
)]
fn props_stack_port_selected() -> Element {
    rsx! {
        PropsStackStory {
            doc: Map2dDoc::new(),
            surface: labelled(small_dome_surface(false)),
            dived: false,
            selection: lpa_studio_core::UiSelection::one(UiPatchTarget::Port {
                node: NodeId::new(10),
                port: 0,
            }),
        }
    }
}

#[story(
    description = "The props stack with a CELL selected: the wire-window card on top (producer path, port-named wire span, source span, reversed flag when set) over its port card. The contested treatment lives in the Outputs panel's bars; here the card states it plainly."
)]
fn props_stack_cell_selected() -> Element {
    rsx! {
        PropsStackStory {
            doc: Map2dDoc::new(),
            surface: labelled(small_dome_surface(false)),
            dived: false,
            selection: lpa_studio_core::UiSelection::one(UiPatchTarget::Cell {
                id: "dome:0:60:0".to_string(),
            }),
        }
    }
}

#[story(
    description = "The awkward cases the stack must survive: an UNNAMED object shows its honest '(unnamed)' placeholder on the root card, and the loaner rig's absurd fixture label truncates in the unarranged placement card's header rather than widening the dock."
)]
fn props_stack_awkward_names() -> Element {
    let doc = Map2dDoc::from_json(
        r#"{"format":3,"sample_diameter":2.0,"canvas":[0.0,0.0,100.0,100.0],"objects":[
            {"name":"","id":"u","shape":{"path":{"points":[[10.0,80.0],[90.0,80.0]],"count":8,"reversed":true,"gaps":[]}}}
        ]}"#,
    )
    .expect("story doc parses");
    let mut surface = labelled(small_dome_surface(false));
    surface.fixtures[0].label = "left_wing_underside_strip_final_v3_ACTUAL".to_string();
    rsx! {
        PropsStackStory {
            doc,
            surface,
            select: ShapePath::root(0),
        }
    }
}
