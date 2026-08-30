//! Stories for the mapping-editor crate — the canvas/tool visuals the
//! unified editor composes (there is no wrapper editor component: the
//! canvas + floats + hint ARE the editor). Mount states are pinned via
//! deterministic session seeding; framing comes from the same doc-fit
//! math the shell uses.

use dioxus::prelude::*;
use lpa_mapping_editor::{
    Camera, CanvasDrag, EditorCanvas, EditorViewOptions, FitReconcile, FitStale, FixtureBody,
    FixtureSprite, HelpFloat, Map2dDoc, MapEditorSession, MapTool, Placement, PolygonMode,
    ReferenceImage, ShapePath, ZoomFloat, display_inset_padding, doc_fit_bounds, tool_hint,
};
use lpa_studio_web_story_macros::story;

use crate::app::node::face_story_fixtures::fyeah_presentable_doc;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EditorCanvasFrame(children: Element) -> Element {
    rsx! {
        div { style: "height: 620px; display: flex; flex-direction: column; border: 1px solid var(--studio-color-border-strong); border-radius: 10px; overflow: hidden;",
            {children}
        }
    }
}

/// The composed editor, as the unified center mounts it: canvas + hint +
/// zoom/help floats over one deterministically seeded session, framed by
/// the shared doc-fit math once the canvas measures itself.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ComposedEditorStory(
    doc: Map2dDoc,
    #[props(default)] initial_selection: Vec<usize>,
    #[props(default = false)] initial_descend: bool,
    #[props(default)] initial_draft: Vec<[f32; 2]>,
    /// Seed the draft into the POLYGON tool in this population instead of
    /// the path tool — the lattice-preview stories' one knob.
    #[props(default)]
    draft_polygon: Option<PolygonMode>,
    #[props(default)] initial_view: Option<EditorViewOptions>,
    #[props(default)] reference: Option<ReferenceImage>,
) -> Element {
    let session = use_signal(|| {
        let mut session = MapEditorSession::new(doc.clone());
        for index in &initial_selection {
            session.selection.insert_path(ShapePath::root(*index));
        }
        if initial_descend {
            session.descend();
        }
        if !initial_draft.is_empty() {
            session.tool = match draft_polygon {
                Some(mode) => MapTool::Polygon {
                    draft: initial_draft.clone(),
                    mode,
                },
                None => MapTool::Path {
                    draft: initial_draft.clone(),
                },
            };
        }
        session
    });
    let camera = use_signal(Camera::new);
    let view_opts = use_signal(move || initial_view.unwrap_or_default());
    let viewport = use_signal(|| None::<[f32; 2]>);
    let mut fit_pending = use_signal(|| true);
    let mut fit_done = use_signal(FitReconcile::default);
    let drag = use_signal(|| None::<CanvasDrag>);
    let live_feed = use_signal(Vec::<[u8; 3]>::new);
    // Deterministic framing: fit once the canvas measures itself, with
    // the same bounds + inset the display mode uses — and re-fit if the
    // measurement settles AFTER the fit while the camera is untouched
    // (the story-baseline churner class:
    // docs/debt/story-capture-pipeline.md).
    {
        let bounds = doc_fit_bounds(session.peek().doc());
        let bounds_key =
            bounds.map(|bounds| [bounds.min_x, bounds.min_y, bounds.width, bounds.height]);
        let viewport_now = *viewport.read();
        // A bare canvas story frames its document unconditionally — there
        // is no dive here to decline, and captures want determinism.
        if let Some([width, height]) = viewport_now
            && (*fit_pending.read()
                || fit_done
                    .read()
                    .staleness([width, height], &camera.peek(), bounds_key)
                    != FitStale::Settled)
        {
            let mut camera = camera;
            if let Some(bounds) = bounds {
                let padding = display_inset_padding(bounds, width, height);
                camera.write().fit(bounds, width, height, padding);
                if *fit_pending.peek() {
                    fit_pending.set(false);
                }
            }
            let mut next = *fit_done.peek();
            next.record([width, height], *camera.peek(), bounds_key);
            if *fit_done.peek() != next {
                fit_done.set(next);
            }
        }
    }
    let hint = tool_hint(&session.read());
    rsx! {
        EditorCanvasFrame {
            div {
                class: "lpme-canvas-wrap",
                "data-fit-viewport": fit_done.read().guard_attr(),
                EditorCanvas {
                    session,
                    camera,
                    view_opts,
                    viewport,
                    drag,
                    live_feed,
                    on_committed: move |()| {},
                    reference,
                }
                div { class: "lpme-hint", "{hint}" }
                ZoomFloat { camera, viewport, fit_pending }
                HelpFloat {}
            }
        }
    }
}

/// Synthetic sprites for the fixture-layer stories: a loaded panel
/// (rotated + scaled, selected, instance-ringed), a scaled halo, a dashed
/// unarranged placeholder, and a range strip — the three honest bodies
/// plus every placement quirk the seam must survive.
fn fixture_story_sprites() -> Vec<FixtureSprite> {
    let resolved =
        lpc_mapping::resolve(&lpc_mapping::corpus::panel_16x16()).expect("corpus resolves");
    let panel_points = resolved.positions();
    let panel_total = panel_points.len() as u32;
    let panel_bounds = lpc_mapping::bounds_of_points(&panel_points)
        .map(|b| {
            [
                f64::from(b.min_x),
                f64::from(b.min_y),
                f64::from(b.width),
                f64::from(b.height),
            ]
        })
        .expect("panel has lamps");
    let halo_points: Vec<[f32; 2]> = (0..24)
        .map(|slot| {
            let angle = (slot as f32) / 24.0 * std::f32::consts::TAU;
            [angle.cos() * 40.0, angle.sin() * 40.0]
        })
        .collect();
    vec![
        FixtureSprite {
            objects: Vec::new(),
            key: "panel".to_string(),
            label: "dome panel".to_string(),
            color: "#5aa9e6".to_string(),
            placement: Placement {
                t: [60.0, 50.0],
                r: 25.0,
                s: 0.5,
            },
            bounds: panel_bounds,
            body: FixtureBody::Lamps {
                points: panel_points,
                total: panel_total,
            },
            arranged: true,
            selected: true,
            selected_range: Some((32, 16)),
        },
        FixtureSprite {
            objects: Vec::new(),
            key: "halo".to_string(),
            label: "halo".to_string(),
            color: "#3fd68e".to_string(),
            placement: Placement {
                t: [360.0, 90.0],
                r: -10.0,
                s: 1.4,
            },
            bounds: [-40.0, -40.0, 80.0, 80.0],
            body: FixtureBody::Lamps {
                points: halo_points,
                total: 24,
            },
            arranged: true,
            selected: false,
            selected_range: None,
        },
        FixtureSprite {
            objects: Vec::new(),
            key: "block".to_string(),
            label: "matrix (pending)".to_string(),
            color: "#e4c065".to_string(),
            placement: Placement {
                t: [70.0, 320.0],
                r: 0.0,
                s: 1.0,
            },
            bounds: [0.0, 0.0, 120.0, 74.0],
            body: FixtureBody::Placeholder { lamps: 256 },
            arranged: false,
            selected: false,
            selected_range: None,
        },
        FixtureSprite {
            objects: Vec::new(),
            key: "strip".to_string(),
            label: "shelf strip".to_string(),
            color: "#c792ea".to_string(),
            placement: Placement {
                t: [250.0, 330.0],
                r: 0.0,
                s: 1.0,
            },
            bounds: [0.0, 0.0, 180.0, 10.0],
            body: FixtureBody::Strip { lamps: 60 },
            arranged: true,
            selected: false,
            selected_range: None,
        },
    ]
}

/// Mount the one canvas directly with deterministic signals (fixed camera,
/// no fit effect) — the fixture-layer stories' harness.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FixtureCanvasStory(focused: Option<String>, dived: bool) -> Element {
    let sprites = fixture_story_sprites();
    let placement = if dived {
        sprites
            .iter()
            .find(|sprite| Some(sprite.key.as_str()) == focused.as_deref())
            .map(|sprite| sprite.placement)
            .unwrap_or_default()
    } else {
        Placement::default()
    };
    let session = use_signal(|| {
        MapEditorSession::new(if dived {
            lpc_mapping::corpus::panel_16x16()
        } else {
            lpc_mapping::Map2dDoc::new()
        })
    });
    let camera = use_signal(|| Camera {
        x: 30.0,
        y: 40.0,
        scale: 1.1,
    });
    let view_opts = use_signal(EditorViewOptions::default);
    let viewport = use_signal(|| None::<[f32; 2]>);
    let drag = use_signal(|| None::<CanvasDrag>);
    let live_feed = use_signal(Vec::<[u8; 3]>::new);
    rsx! {
        EditorCanvasFrame {
            EditorCanvas {
                session,
                camera,
                view_opts,
                viewport,
                drag,
                live_feed,
                on_committed: move |()| {},
                placement,
                fixtures: sprites,
                focused,
                on_fixture: move |_| {},
            }
        }
    }
}

#[story(
    description = "The fixture view on the one canvas: arranged sprites rendered from data in project space — a rotated+scaled panel (selected, instance ring), a scaled halo, a dashed unarranged placeholder, and a range strip."
)]
pub(crate) fn fixture_view_arrangement() -> Element {
    rsx! {
        FixtureCanvasStory { focused: None, dived: false }
    }
}

#[story(
    description = "Dived in place: the focused panel's live document renders through its rotated+scaled placement on the same canvas — same camera — while neighbour sprites dim to context."
)]
pub(crate) fn fixture_dive_neighbours() -> Element {
    rsx! {
        FixtureCanvasStory { focused: Some("panel".to_string()), dived: true }
    }
}

#[story(
    description = "The 16×16 snake panel in the editor: wiring numbers + arrows at fit zoom — the editor's default view."
)]
pub(crate) fn editor_panel_wiring() -> Element {
    rsx! {
        ComposedEditorStory { doc: lpc_mapping::corpus::panel_16x16() }
    }
}

#[story(
    description = "Single selection with the anchored properties popover: the cat-ears headband path (count, direction, reorder, expand, delete)."
)]
pub(crate) fn editor_selection_popover() -> Element {
    rsx! {
        ComposedEditorStory {
            doc: lpc_mapping::corpus::cat_ears(),
            initial_selection: vec![2],
        }
    }
}

#[story(
    description = "Group selection across the whole sign import: bbox outline + corner resize handles + multi-select popover."
)]
pub(crate) fn editor_group_selected() -> Element {
    rsx! {
        ComposedEditorStory {
            doc: fyeah_presentable_doc(),
            initial_selection: vec![0, 1, 2, 3, 4, 5, 6],
        }
    }
}

#[story(
    description = "Path drawing in progress: draft vertices, resolved ghost lamps, and the gold chain link from the previous object."
)]
pub(crate) fn editor_path_draft() -> Element {
    rsx! {
        ComposedEditorStory {
            doc: lpc_mapping::corpus::cat_ears(),
            initial_draft: vec![[460.0, 320.0], [520.0, 240.0], [580.0, 320.0]],
        }
    }
}

/// The polygon tool's two populations over ONE authored outline: a shaped
/// matrix (lattice inside a chevron) beside the same outline populated on
/// its perimeter. Built from the shapes rather than JSON so the story reads
/// as the document it is.
fn polygon_populations_doc() -> Map2dDoc {
    let chevron = |dx: f32| -> Vec<[f32; 2]> {
        vec![
            [dx + 10.0, 10.0],
            [dx + 130.0, 10.0],
            [dx + 130.0, 150.0],
            [dx + 70.0, 100.0],
            [dx + 10.0, 150.0],
        ]
    };
    Map2dDoc {
        canvas: Some([0.0, 0.0, 320.0, 170.0]),
        objects: vec![
            lpc_mapping::Map2dObject {
                name: "sign face".to_string(),
                id: None,
                stride: None,
                shape: lpc_mapping::Map2dShape::FilledPolygon(lpc_mapping::FilledPolygonShape {
                    points: chevron(0.0),
                    pitch: 18.0,
                    angle_deg: 0.0,
                    origin: [0.0, 0.0],
                    routing: lpc_mapping::GridRouting::Snake,
                    start_corner: lpc_mapping::GridCorner::Tl,
                }),
            },
            lpc_mapping::Map2dObject {
                name: "sign edge".to_string(),
                id: None,
                stride: None,
                shape: lpc_mapping::Map2dShape::Polygon(lpc_mapping::PolygonShape {
                    points: chevron(170.0),
                    count: 26,
                    align: lpc_mapping::PathAlign::On,
                }),
            },
        ],
        ..Map2dDoc::new()
    }
}

#[story(
    description = "One authored outline, two populations (the polygon tool's whole point): the shaped matrix on the left is SELECTED — its authored silhouette promotes to a solid accent line and vertex handles sit on its corners, while its lamps stay a FIELD inside the neutral band its wiring sweeps — beside the same chevron populated along its perimeter, where the lamp band IS the shape."
)]
pub(crate) fn editor_filled_polygon_selected() -> Element {
    rsx! {
        ComposedEditorStory {
            doc: polygon_populations_doc(),
            initial_selection: vec![0],
        }
    }
}

#[story(
    description = "Polygon drawing in filled mode, mid-draft: the placed corners, the dashed IMPLICIT closing edge back to the first vertex, that vertex ringed as the close target, and the live lattice ghosts — resolved by the real resolver, so the preview is cell-for-cell what closing the outline commits."
)]
pub(crate) fn editor_polygon_lattice_draft() -> Element {
    rsx! {
        ComposedEditorStory {
            doc: lpc_mapping::corpus::cat_ears(),
            // Inside the cat-ears doc bounds (x 80–420, y 72–330) so the
            // camera's doc fit keeps the whole draft — ghosts included —
            // in frame.
            initial_draft: vec![[170.0, 235.0], [330.0, 235.0], [330.0, 330.0], [250.0, 290.0]],
            draft_polygon: Some(PolygonMode::Filled),
        }
    }
}

#[story(
    description = "Texture-frame fit preview on the sign import: the square render target vs the doc canvas — why the sign sat corner-pinned."
)]
pub(crate) fn editor_fit_preview() -> Element {
    rsx! {
        ComposedEditorStory {
            doc: fyeah_presentable_doc(),
            initial_view: Some(EditorViewOptions {
                numbers: false,
                arrows: true,
                live: false,
                fit_preview: true,
                reference: true,
            }),
        }
    }
}

#[story(
    description = "A rotational repeat selected: one authored sector path with a gap, five ghosted instances, and the repeat popover (count, center, unwrap, expand)."
)]
pub(crate) fn editor_repeated_sector() -> Element {
    rsx! {
        ComposedEditorStory {
            doc: lpc_mapping::corpus::repeated_sector(),
            initial_selection: vec![0],
        }
    }
}

#[story(
    description = "Scoped tessellation authoring: descended into the repeat, the authored sub-object is the interactive primary while the other instances render inert and span-colored; the Props stack carries the scope as ancestor cards."
)]
pub(crate) fn editor_repeat_scoped() -> Element {
    rsx! {
        ComposedEditorStory {
            doc: lpc_mapping::corpus::repeated_sector(),
            initial_selection: vec![0],
            initial_descend: true,
        }
    }
}

#[story(
    description = "Reference image under the authored geometry: a traceable background at half opacity, dot grid visible through it — the sketch-to-mapping flow."
)]
pub(crate) fn editor_reference_trace() -> Element {
    // Inline utf8 data URL: deterministic, no asset fetch, nothing the
    // capture pipeline has to wait for. Single quotes and rgb() colors keep
    // it URL-safe.
    let reference = ReferenceImage {
        data_url: "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' \
                   viewBox='0 0 400 400'><circle cx='200' cy='200' r='150' fill='none' \
                   stroke='rgb(120,140,255)' stroke-width='6'/>\
                   <path d='M200,200 L200,50 A150,150 0 0 1 342,153 z' \
                   fill='rgb(80,90,140)'/></svg>"
            .to_string(),
        opacity: 0.5,
        size: [400.0, 400.0],
    };
    rsx! {
        ComposedEditorStory {
            doc: lpc_mapping::corpus::repeated_sector(),
            reference: Some(reference),
        }
    }
}
