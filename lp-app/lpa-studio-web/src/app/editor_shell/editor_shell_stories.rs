//! Arrange-canvas stories: the conceptual project space at its three
//! honesty levels (loaded geometry, placeholder blocks, peach strips) and
//! the auto-pack row. Real mini-dome map2d bytes from the embedded
//! example feed the loaded fixtures — the same resolver the device runs.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use super::arrange_canvas::ArrangeCanvas;
use crate::app::patch::patch_surface_stories::{mini_dome_surface, peach_surface};
use lpa_studio_core::{
    ArtifactLocation, UiArrangeFootprint, UiArrangeMeta, UiArrangeTransform, UiPatchSurface,
    UiPatchTarget,
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
    description = "The Arrange canvas on the mini-dome, both fixtures loaded and arranged: each fixture's own doc-space lamp geometry (the device's resolver, real example bytes) placed by its editor.json transform in one conceptual space — the doors tilted 15° to show rotation. Name tags ride above solid frames (arranged), lamps wear the object colour, and sector 2's lamps are ringed by the shared selection."
)]
fn arrange_canvas_mini_dome() -> Element {
    let (surface, bodies) = dome_canvas_inputs();
    canvas_frame(rsx! {
        ArrangeCanvas {
            surface,
            bodies,
            selection: Some(UiPatchTarget::Instance {
                node: lpa_studio_core::NodeId::new(2),
                path: "/sector/2".to_string(),
            }),
            on_action: move |_| {},
        }
    })
}

#[story(
    description = "The DIVE (in-place, no separate screen): the dome focused for mapping edits — its own MapEditor with tools, ⌘Z scoped to the session — while the doors render dimmed INSIDE the canvas at their true arranged position (transformed into the dome's doc space, 15° tilt and all). The breadcrumb strip leads back to the arranged space."
)]
fn editor_shell_focused_mapping() -> Element {
    let (surface, bodies) = dome_canvas_inputs();
    let dome_artifact = surface.fixtures[0]
        .mapping_artifact
        .clone()
        .expect("dome artifact");
    let editor = lpa_studio_core::UiAssetEditor {
        artifact: dome_artifact.clone(),
        kind: lpa_studio_core::UiAssetEditorKind::Map2d,
        source: "dome.map2d.json".to_string(),
        content: Some(lpa_studio_core::UiAssetContent::from_bytes(
            bodies[&dome_artifact].as_bytes(),
            false,
            0,
        )),
        in_flight: false,
        failure: None,
        shader_error: None,
        uniforms: Vec::new(),
        agent: None,
    };
    let context =
        super::arrange_canvas::dive_context(
        &surface,
        &bodies,
        &super::arrange_canvas::PackSlots::new(),
        lpa_studio_core::NodeId::new(2),
    );
    canvas_frame(rsx! {
        div { class: "tw:flex tw:min-h-[30px] tw:flex-none tw:items-center tw:gap-2 tw:border-b tw:border-border-subtle tw:bg-card-muted tw:px-2.5",
            button { class: "tw:cursor-pointer tw:border-none tw:bg-transparent tw:p-0 tw:text-xs tw:text-selection-border",
                "‹ Arrange"
            }
            span { class: "tw:text-[10px] tw:font-semibold tw:uppercase tw:tracking-[0.13em] tw:text-muted-foreground",
                "dome · mapping"
            }
        }
        div { class: "tw:min-h-0 tw:flex-1 tw:overflow-hidden",
            super::mapping_session::MappingSessionHost { editor, context }
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
        ArrangeCanvas {
            surface,
            bodies,
            selection: None,
            on_action: move |_| {},
        }
    })
}
