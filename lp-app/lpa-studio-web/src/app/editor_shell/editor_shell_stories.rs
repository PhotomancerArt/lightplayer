//! Editor-shell stories on the one project canvas: the fixture view at
//! its three honesty levels (loaded geometry, placeholder blocks, peach
//! strips), the IN-PLACE dive, and the morphing toolbar's two states.
//! Real mini-dome map2d bytes from the embedded example feed the loaded
//! fixtures — the same resolver the device runs.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lpa_mapping_editor::{Map2dDoc, MapEditorSession, MapTool, ShapePath};
use lpa_studio_web_story_macros::story;

use super::arrange::{DiveHost, ProjectCanvasHost};
use super::toolbar::ToolbarStrip;
use crate::app::patch::patch_story_fixtures::{mini_dome_surface, peach_surface};
use lpa_studio_core::{
    ArtifactLocation, NodeId, UiArrangeFootprint, UiArrangeMeta, UiArrangeTransform,
    UiPatchSurface, UiPatchTarget, UiSelection,
};

/// The mini-dome surface with real map2d bodies attached: fixture
/// artifacts stamped, bodies pulled from the embedded example, both
/// fixtures ARRANGED (dome at origin, doors beside it, tilted).
fn dome_canvas_inputs() -> (UiPatchSurface, BTreeMap<ArtifactLocation, String>) {
    let example = lpa_studio_core::app::home::embedded_example("examples/mini-dome")
        .expect("mini-dome embedded");
    let body = |name: &str| {
        let bytes = example
            .files
            .iter()
            .find(|(path, _)| *path == name)
            .map(|(_, bytes)| *bytes)
            .expect("example file");
        String::from_utf8(bytes.to_vec()).expect("utf8 map2d")
    };
    let mut surface = mini_dome_surface(false);
    let mut bodies = BTreeMap::new();
    let dome_artifact = ArtifactLocation::file("/dome/dome.map2d.json");
    let doors_artifact = ArtifactLocation::file("/doors/doors.map2d.json");
    bodies.insert(dome_artifact.clone(), body("dome/dome.map2d.json"));
    bodies.insert(doors_artifact.clone(), body("doors/doors.map2d.json"));
    surface.fixtures[0].mapping_artifact = Some(dome_artifact);
    surface.fixtures[0].arrange = Some(UiArrangeMeta {
        arranged: true,
        transform: UiArrangeTransform::default(),
        footprint: None,
    });
    surface.fixtures[1].mapping_artifact = Some(doors_artifact);
    surface.fixtures[1].arrange = Some(UiArrangeMeta {
        arranged: true,
        transform: UiArrangeTransform {
            t: [165.0, 30.0],
            r: 15.0,
            s: 1.0,
        },
        footprint: None,
    });
    (surface, bodies)
}

fn canvas_frame(body: Element) -> Element {
    rsx! {
        div { class: "tw:flex tw:h-[560px] tw:flex-col tw:overflow-hidden tw:rounded-md tw:border tw:border-border-strong",
            {body}
        }
    }
}

#[story(
    description = "The fixture view on the one canvas, mini-dome loaded and arranged: each fixture's own doc-space lamp geometry (the device's resolver, real example bytes) placed by its editor.json transform in one project space — the doors tilted 15° to show rotation. Name tags ride above solid frames (arranged), lamps wear the object colour, and sector 2's lamps are ringed by the shared selection."
)]
fn arrange_canvas_mini_dome() -> Element {
    let (surface, bodies) = dome_canvas_inputs();
    canvas_frame(rsx! {
        ProjectCanvasHost {
            surface,
            bodies,
            selection: lpa_studio_core::UiSelection::one(UiPatchTarget::Instance {
                node: lpa_studio_core::NodeId::new(2),
                path: "/sector/2".to_string(),
            }),
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "Fixture MULTI-SELECT on the Mapping canvas (unified-selection P3): both mini-dome fixtures shift-set into one sibling selection — each sprite wears its own ring, and ONE shared dashed box spans the set with corner scale handles (`transform_handles`, the Mapping view's transform furniture; Patching multi-selects without it). A corner drag scales the whole set about the opposite corner as one `SetMany` undo step; this capture pins the resting furniture."
)]
fn arrange_canvas_multi_select() -> Element {
    let (surface, bodies) = dome_canvas_inputs();
    let mut selection = UiSelection::empty();
    selection.set_siblings(vec![
        UiPatchTarget::Fixture {
            node: NodeId::new(2),
        },
        UiPatchTarget::Fixture {
            node: NodeId::new(3),
        },
    ]);
    canvas_frame(rsx! {
        ProjectCanvasHost {
            surface,
            bodies,
            selection,
            transform_handles: true,
            on_action: move |_| {},
        }
    })
}

/// The dived story's harness: the workbench-owned session seeded with the
/// dome's real document, handed to the SAME canvas as layer state.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DiveInPlaceStory() -> Element {
    let (mut surface, bodies) = dome_canvas_inputs();
    // A non-identity focused placement (the seam is the point), with the
    // doors close enough that the focused-fixture fit keeps the dimmed
    // neighbour in frame.
    surface.fixtures[0].arrange = Some(UiArrangeMeta {
        arranged: true,
        transform: UiArrangeTransform {
            t: [10.0, 20.0],
            r: -8.0,
            s: 0.9,
        },
        footprint: None,
    });
    surface.fixtures[1].arrange = Some(UiArrangeMeta {
        arranged: true,
        transform: UiArrangeTransform {
            t: [150.0, 120.0],
            r: 15.0,
            s: 1.0,
        },
        footprint: None,
    });
    let dome_node = surface.fixtures[0].node;
    let dome_artifact = surface.fixtures[0]
        .mapping_artifact
        .clone()
        .expect("dome artifact");
    let dome_doc = Map2dDoc::from_json(&bodies[&dome_artifact]).expect("dome parses");
    let session = use_signal(move || MapEditorSession::new(dome_doc.clone()));
    canvas_frame(rsx! {
        ProjectCanvasHost {
            surface,
            bodies,
            selection: lpa_studio_core::UiSelection::empty(),
            dive: Some(DiveHost {
                node: dome_node,
                session,
                editable: true,
            }),
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "The DIVE in place (one canvas, camera continuity): the dome's live document renders editable through its placement on the same project canvas — wiring, tools, hint, zoom and help floats — while the doors stay visible as a dimmed sprite at their true arranged tilt. No component swap, no chrome change."
)]
fn editor_shell_focused_mapping() -> Element {
    rsx! {
        DiveInPlaceStory {}
    }
}

/// The SCOPE-DERIVED dive harness (unified-selection P4): one
/// `UiSelection` is the only input — the story computes the dive from
/// `selection.entered()` exactly as the workbench does, so these captures
/// pin the states derivation produces rather than a posed layer. The
/// session's own selection mirrors what the coordinator's seed would set
/// for the core targets (`selection.rs`: instance targets → their object
/// roots).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DerivedDiveStory(selection: UiSelection, session_roots: Vec<usize>) -> Element {
    let (surface, bodies) = dome_canvas_inputs();
    let dome_artifact = surface.fixtures[0]
        .mapping_artifact
        .clone()
        .expect("dome artifact");
    let dome_doc = Map2dDoc::from_json(&bodies[&dome_artifact]).expect("dome parses");
    let session = use_signal(move || {
        let mut session = MapEditorSession::new(dome_doc.clone());
        for root in &session_roots {
            session.selection.insert_path(ShapePath::root(*root));
        }
        session
    });
    let dive = selection.entered().map(|node| DiveHost {
        node,
        session,
        editable: true,
    });
    canvas_frame(rsx! {
        ProjectCanvasHost {
            surface,
            bodies,
            selection,
            transform_handles: true,
            dive,
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "ENTERED WITH AN OBJECT SELECTED — the derived dive (unified-selection P4): the core selection holds one object-grain target (the dome's `/sector/2`), and the entered fixture is COMPUTED from it — no independent dive state to drift. The canvas renders the scope as the dive (Mapping's activity): the dome's live document editable in place, the sector object selected in the session (the coordinator's seed), the doors dimmed as the neighbour sprite. Esc ascends the one ladder: object → its fixture at project level."
)]
fn editor_shell_entered_selected() -> Element {
    rsx! {
        DerivedDiveStory {
            selection: UiSelection::one(UiPatchTarget::Instance {
                node: NodeId::new(2),
                path: "/sector/2".to_string(),
            }),
            session_roots: vec![0],
        }
    }
}

#[story(
    description = "ENTERED EMPTY — the one place scope outlives derivation (unified-selection P2/P4): `UiSelection::enter` holds the dome entered with NO targets, the state pure derivation cannot represent and drawing the first object needs (an empty document has nothing to select on the way in). The dive renders with nothing selected inside — no object rings, the tools ready — and esc's clear rung leaves the fixture itself selected at project level."
)]
fn editor_shell_entered_empty() -> Element {
    let mut selection = UiSelection::empty();
    selection.enter(NodeId::new(2));
    rsx! {
        DerivedDiveStory { selection, session_roots: vec![] }
    }
}

/// The peach-2d surface with its REAL map2d bytes: the scale-extreme
/// regression case (pitch ≈ 39 own units at sample_diameter 26 — ~5×
/// coarser than every other example). Both docs share one authored
/// canvas, so identity transforms recreate the real overlay.
fn peach_canvas_inputs() -> (UiPatchSurface, BTreeMap<ArtifactLocation, String>) {
    let mut surface = peach_surface();
    let mut bodies = BTreeMap::new();
    let body_artifact = ArtifactLocation::file("/body/peach_body.map2d.json");
    let leaf_artifact = ArtifactLocation::file("/leaf/peach_leaf.map2d.json");
    bodies.insert(
        body_artifact.clone(),
        include_str!("../../../../../examples/peach-2d/body/peach_body.map2d.json").to_string(),
    );
    bodies.insert(
        leaf_artifact.clone(),
        include_str!("../../../../../examples/peach-2d/leaf/peach_leaf.map2d.json").to_string(),
    );
    surface.fixtures[0].mapping_artifact = Some(body_artifact);
    surface.fixtures[1].mapping_artifact = Some(leaf_artifact);
    // The peach docs are id-less (format 1): the real surface builder still
    // derives one RANGE-grain instance per object span (empty path), and
    // the canvas draws one body per instance — the story mirrors that.
    surface.fixtures[0].instances = vec![
        instance_range("body-leg-1", 0, 22),
        instance_range("body-leg-2", 22, 22),
    ];
    surface.fixtures[1].instances = vec![instance_range("leaves", 0, 12)];
    for fixture in &mut surface.fixtures {
        fixture.arrange = Some(UiArrangeMeta {
            arranged: true,
            transform: UiArrangeTransform::default(),
            footprint: None,
        });
    }
    (surface, bodies)
}

/// A range-grain instance (id-less object): empty path, stride 1.
fn instance_range(label: &str, start: u32, lamps: u32) -> lpa_studio_core::UiPatchInstance {
    lpa_studio_core::UiPatchInstance {
        path: String::new(),
        label: label.to_string(),
        start,
        lamps,
        stride: 1,
        placed: true,
    }
}

#[story(
    description = "The peach's real map2d bytes on the canvas — the scale-extreme case: authored ~5× coarser than other docs, sample_diameter 26 at pitch ≈ 39. Cell and outline sizing derive from the doc's own numbers (median pitch, footprint floor), so the body strands read as a touching voronoi mesh at ANY authoring scale — the G1 regression that absolute clamps once broke."
)]
fn arrange_canvas_peach_scale() -> Element {
    let (surface, bodies) = peach_canvas_inputs();
    canvas_frame(rsx! {
        ProjectCanvasHost {
            surface,
            bodies,
            selection: lpa_studio_core::UiSelection::empty(),
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "The canvas's three honesty levels in one space: the loaded dome (real geometry), the doors as a PLACEHOLDER block (body not loaded — the cached footprint's size and lamp count, clearly a block), and the peach fixtures as dashed range strips (no map2d at all). Everything unarranged auto-packs into the bottom row with dashed frames until first dragged."
)]
fn arrange_canvas_mixed_states() -> Element {
    let (mut surface, mut bodies) = dome_canvas_inputs();
    // The doors' body is "not loaded": drop the bytes, keep a footprint.
    if let Some(artifact) = &surface.fixtures[1].mapping_artifact {
        bodies.remove(artifact);
    }
    surface.fixtures[1].arrange = Some(UiArrangeMeta {
        arranged: true,
        transform: UiArrangeTransform {
            t: [170.0, 20.0],
            r: 0.0,
            s: 1.0,
        },
        footprint: Some(UiArrangeFootprint {
            bbox: [0.0, 0.0, 96.0, 92.0],
            lamps: 27,
        }),
    });
    // The peach fixtures join the same project, unarranged (auto-pack).
    let peach = peach_surface();
    surface.fixtures.extend(peach.fixtures);
    canvas_frame(rsx! {
        ProjectCanvasHost {
            surface,
            bodies,
            selection: lpa_studio_core::UiSelection::empty(),
            on_action: move |_| {},
        }
    })
}

/// A dirty synthetic asset editor for the toolbar's save cluster.
fn story_asset_editor() -> lpa_studio_core::UiAssetEditor {
    lpa_studio_core::UiAssetEditor {
        artifact: ArtifactLocation::file("/dome/dome.map2d.json"),
        kind: lpa_studio_core::UiAssetEditorKind::Map2d,
        source: "dome.map2d.json".to_string(),
        content: Some(lpa_studio_core::UiAssetContent::from_bytes(
            br#"{"format":1,"objects":[]}"#,
            true,
            0,
        )),
        in_flight: false,
        failure: None,
        shader_error: None,
        uniforms: Vec::new(),
        agent: None,
    }
}

#[story(
    description = "The one toolbar row, fixture state: breadcrumb root, arrange verbs for the selection, history, and the counts readout — a data-driven item list rendered by the one strip."
)]
fn editor_toolbar_fixture() -> Element {
    rsx! {
        ToolbarStrip {
            groups: super::fixture_toolbar(true, true, 4, 2),
            on_item: move |_| {},
        }
    }
}

#[story(
    description = "The same strip, dived state (the morph is a data swap): breadcrumb back to the project, V/G/R/P tools, view toggles, and the save cluster with an unsaved edit."
)]
fn editor_toolbar_dived() -> Element {
    rsx! {
        ToolbarStrip {
            groups: super::dive_toolbar(
                &MapTool::Select,
                Default::default(),
                &story_asset_editor(),
                true,
                None,
            ),
            on_item: move |_| {},
        }
    }
}
