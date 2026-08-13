//! The **Arrange canvas** (unified-editor P4, spike §1 concept D): every
//! fixture's OWN doc-space geometry placed by its `editor.json` transform
//! in one conceptual project space — subway-map honesty, drag-to-arrange,
//! and NEVER a sampling input (the engine does not know this space
//! exists).
//!
//! Three honest bodies per fixture:
//! - **loaded** — the map2d body is in the snapshot: real lamp dots (the
//!   same resolver the device runs), in the fixture's object colour;
//! - **placeholder** — body not loaded: a named block at the cached
//!   footprint's size (clearly a block, not fake geometry);
//! - **peach** — no map2d at all: a dashed range strip with lamp dots.
//!
//! Unarranged fixtures auto-pack into a bottom row (stable tree order)
//! until first dragged; the first drag writes their `editor.json` entry
//! through one [`lpa_studio_core::EditorMetaOp`] per gesture (one undo
//! step, gesture-coalesced like the mapping editor).
//!
//! Camera: a viewBox window over the conceptual space — fit-all until the
//! user pans/zooms, then their window sticks (per mount; `editor.json`
//! never stores cameras).

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lpa_mapping_editor::{CanvasAnchor, capture_pointer, object_color};
use lpa_studio_core::{
    ArtifactLocation, EditorMetaFixture, EditorMetaOp, EditorMetaVerb, NodeId, ProjectController,
    ProjectEditorOp, UiAction, UiArrangeTransform, UiPatchSurface, UiPatchTarget,
};

/// Display cap: a fixture with more lamps than this renders every k-th
/// lamp (display subsampling only — dome-scale fixtures must not melt the
/// SVG; the resolver still ran the full document).
const MAX_DISPLAY_LAMPS: usize = 2000;

/// Movement threshold in CSS pixels before a press becomes a drag — under
/// it, pointer-up is a SELECT, and no stray write ever happens.
const DRAG_THRESHOLD_PX: f64 = 4.0;

/// Gap between auto-packed fixtures, in doc units.
const PACK_GAP: f64 = 24.0;

/// One fixture's render-ready facts.
#[derive(Clone, PartialEq)]
struct FixtureRender {
    node: NodeId,
    /// The `editor.json` key (authored address path).
    key: String,
    label: String,
    color: &'static str,
    /// Effective placement: the arranged transform, or the auto-pack slot.
    transform: UiArrangeTransform,
    arranged: bool,
    /// Own-space bounds the frame is drawn around.
    bounds: [f64; 4],
    body: FixtureBody,
    /// `(path, start, lamps)` for instance selection rings.
    instances: Vec<(String, u32, u32)>,
}

#[derive(Clone, PartialEq)]
enum FixtureBody {
    /// Doc-space lamp points (possibly display-subsampled).
    Lamps { points: Vec<[f32; 2]>, total: u32 },
    /// Footprint block: the body is not loaded yet.
    Placeholder { lamps: u32 },
    /// The shape-less strip: a range bar with lamp dots.
    Strip { lamps: u32 },
}

/// What a pointer stream is doing. Panning is NOT here: under the house
/// wheel grammar (scroll pans, ⌘scroll zooms) a background drag is only a
/// deselect tap in waiting.
#[derive(Clone, PartialEq)]
enum CanvasDrag {
    /// Maybe-drag on a fixture: becomes a real drag past the threshold.
    Fixture {
        key: String,
        node: NodeId,
        start_client: [f64; 2],
        original: UiArrangeTransform,
        moved: bool,
    },
    /// Background press: pointer-up without movement deselects.
    Tap { start_client: [f64; 2], moved: bool },
}

/// A committed drag held on screen until the snapshot confirms it: the
/// override survives pointer-up so the fixture never snaps back while the
/// write round-trips (the gate's jump-back bug).
#[derive(Clone, PartialEq)]
struct DragOverride {
    key: String,
    transform: UiArrangeTransform,
    /// True after pointer-up: the override retires once the surface
    /// carries (approximately — the kernel quantizes) this transform.
    committed: bool,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ArrangeCanvas(
    surface: UiPatchSurface,
    /// map2d bodies by artifact (extracted from the snapshot's node views;
    /// stories inject embedded-example bytes directly).
    bodies: BTreeMap<ArtifactLocation, String>,
    selection: Option<UiPatchTarget>,
    /// Sticky auto-pack slots (shell-owned; see [`PackSlots`]). Stories
    /// omit it and get the ad-hoc packing.
    #[props(default)]
    pack: PackSlots,
    /// Double-click on a fixture: the shell focuses it for mapping edits
    /// (P5). Absent = the canvas is arrange-only (stories).
    #[props(default)]
    on_focus: Option<EventHandler<NodeId>>,
    on_action: EventHandler<UiAction>,
) -> Element {
    // Geometry is derived per (surface, bodies) change — resolver runs are
    // cheap at fixture grain and the memo keeps camera work off that path.
    let memo_surface = surface.clone();
    let memo_bodies = bodies.clone();
    let memo_pack = pack.clone();
    let renders = use_memo(use_reactive!(|(memo_surface, memo_bodies, memo_pack)| {
        build_renders(&memo_surface, &memo_bodies, &memo_pack)
    }));
    // The camera window: seeded from fit-all when content first appears,
    // then FROZEN — arranging a fixture must never move the camera (the
    // gate's drift bug). The fit button re-frames on demand.
    let mut view_box = use_signal(|| None::<[f64; 4]>);
    let mut drag = use_signal(|| None::<CanvasDrag>);
    // Live drag override, held until the snapshot confirms the write.
    let mut drag_override = use_signal(|| None::<DragOverride>);
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(unused_mut, reason = "only the wasm mount handler writes it")
    )]
    let mut anchor = use_signal(CanvasAnchor::default);

    let renders_now = renders.read().clone();
    let fit_box = fit_view_box(&renders_now);
    // Seed once; never follow content afterwards.
    if view_box.peek().is_none()
        && let Some(fit) = fit_box
    {
        view_box.set(Some(fit));
    }
    // Retire a committed override once the surface caught up (the kernel
    // quantizes to 4 decimals, so compare loosely).
    let retire_override = {
        let over = drag_override.peek().clone();
        matches!(over, Some(over) if over.committed
        && renders_now.iter().any(|render| {
            render.key == over.key && transforms_close(&render.transform, &over.transform)
        }))
    };
    if retire_override {
        drag_override.set(None);
    }
    let vb = view_box.read().unwrap_or([0.0, 0.0, 100.0, 62.5]);
    // CSS px → doc units, from the mounted rect (fallback: 1000px wide).
    let units_per_px = {
        let size = anchor.peek().size().unwrap_or([1000.0, 625.0]);
        let scale = (f64::from(size[0]) / vb[2])
            .min(f64::from(size[1]) / vb[3])
            .max(1e-6);
        1.0 / scale
    };

    let editor_artifact = surface.editor_meta_artifact.clone();
    let meta_fixtures: Vec<EditorMetaFixture> = surface
        .fixtures
        .iter()
        .filter_map(|fixture| {
            Some(EditorMetaFixture {
                node_key: fixture.address.clone()?,
                mapping_artifact: fixture.mapping_artifact.clone(),
            })
        })
        .collect();
    let dispatch_set = {
        let editor_artifact = editor_artifact.clone();
        let meta_fixtures = meta_fixtures.clone();
        move |on_action: &EventHandler<UiAction>,
              key: &str,
              node: NodeId,
              transform: UiArrangeTransform| {
            let Some(artifact) = editor_artifact.clone() else {
                return;
            };
            on_action.call(UiAction::from_op(
                ProjectController::NODE_ID,
                EditorMetaOp {
                    artifact,
                    fixtures: meta_fixtures.clone(),
                    verb: EditorMetaVerb::Set {
                        node_key: key.to_string(),
                        node: Some(node),
                        transform,
                    },
                },
            ));
        }
    };

    let select = move |target: Option<UiPatchTarget>| {
        on_action.call(UiAction::from_op(
            lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
            ProjectEditorOp::PatchSelect { target },
        ));
    };

    rsx! {
        div { class: "tw:relative tw:min-h-0 tw:flex-1",
        button {
            class: "tw:absolute tw:right-2 tw:top-2 tw:z-10 tw:cursor-pointer tw:rounded tw:border tw:border-border-strong tw:bg-card-subtle tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:text-subtle-foreground tw:hover:text-strong-foreground",
            title: "Fit everything in view",
            onclick: move |_| {
                if let Some(fit) = fit_view_box(&renders.peek()) {
                    view_box.set(Some(fit));
                }
            },
            "⤢ fit"
        }
        svg {
            class: "tw:h-full tw:w-full tw:touch-none tw:select-none tw:bg-background",
            view_box: "{vb[0]} {vb[1]} {vb[2]} {vb[3]}",
            preserve_aspect_ratio: "xMidYMid meet",
            onmounted: move |evt| {
                #[cfg(target_arch = "wasm32")]
                if let Some(element) = evt.data().downcast::<web_sys::Element>() {
                    anchor.set(CanvasAnchor::from_element(element.clone()));
                }
                #[cfg(not(target_arch = "wasm32"))]
                let _ = &evt;
            },
            onpointerdown: move |evt| {
                // ONE hit-test at the canvas level decides fixture-drag vs
                // pan — SVG child-event propagation proved unreliable under
                // Dioxus's delegation, and the canvas already knows every
                // fixture's transformed frame anyway.
                capture_pointer(&evt);
                let point = evt.data().client_coordinates();
                let client = [point.x, point.y];
                let hit = event_doc_point(&anchor.peek(), vb, client)
                    .and_then(|doc| hit_fixture(&renders.peek(), doc).cloned());
                match hit {
                    Some(render) => {
                        let transform = drag_override
                            .peek()
                            .as_ref()
                            .filter(|over| over.key == render.key)
                            .map(|over| over.transform)
                            .unwrap_or(render.transform);
                        drag.set(Some(CanvasDrag::Fixture {
                            key: render.key,
                            node: render.node,
                            start_client: client,
                            original: transform,
                            moved: false,
                        }));
                    }
                    None => {
                        drag.set(Some(CanvasDrag::Tap {
                            start_client: client,
                            moved: false,
                        }));
                    }
                }
            },
            onpointermove: move |evt| {
                let Some(state) = drag.peek().clone() else { return };
                let point = evt.data().client_coordinates();
                match state {
                    CanvasDrag::Tap {
                        start_client,
                        moved,
                    } => {
                        let dx = point.x - start_client[0];
                        let dy = point.y - start_client[1];
                        if !moved && (dx.abs() > DRAG_THRESHOLD_PX || dy.abs() > DRAG_THRESHOLD_PX) {
                            drag.set(Some(CanvasDrag::Tap {
                                start_client,
                                moved: true,
                            }));
                        }
                    }
                    CanvasDrag::Fixture {
                        key,
                        node,
                        start_client,
                        original,
                        moved,
                    } => {
                        let dx = point.x - start_client[0];
                        let dy = point.y - start_client[1];
                        let now_moved =
                            moved || dx.abs() > DRAG_THRESHOLD_PX || dy.abs() > DRAG_THRESHOLD_PX;
                        if now_moved {
                            let mut dragged = original;
                            dragged.t[0] += dx * units_per_px;
                            dragged.t[1] += dy * units_per_px;
                            drag_override.set(Some(DragOverride {
                                key: key.clone(),
                                transform: dragged,
                                committed: false,
                            }));
                        }
                        drag.set(Some(CanvasDrag::Fixture {
                            key,
                            node,
                            start_client,
                            original,
                            moved: now_moved,
                        }));
                    }
                }
            },
            onpointerup: move |_| {
                let state = drag.peek().clone();
                drag.set(None);
                match state {
                    Some(CanvasDrag::Fixture {
                        key, node, moved, ..
                    }) => {
                        if moved {
                            // One gesture = one op = one undo step. The
                            // override stays up (committed) until the
                            // snapshot echoes the write — no snap-back.
                            let over = drag_override.peek().clone();
                            if let Some(mut over) = over
                                && over.key == key
                            {
                                over.committed = true;
                                let transform = over.transform;
                                drag_override.set(Some(over));
                                dispatch_set(&on_action, &key, node, transform);
                            }
                        } else {
                            drag_override.set(None);
                            select(Some(UiPatchTarget::Fixture { node }));
                        }
                    }
                    // A background tap that never moved deselects.
                    Some(CanvasDrag::Tap { moved: false, .. }) => select(None),
                    Some(CanvasDrag::Tap { moved: true, .. }) | None => {}
                }
            },
            onpointercancel: move |_| {
                drag.set(None);
                drag_override.set(None);
            },
            ondoubleclick: move |evt| {
                let point = evt.data().client_coordinates();
                if let Some(doc) = event_doc_point(&anchor.peek(), vb, [point.x, point.y])
                    && let Some(render) = hit_fixture(&renders.peek(), doc)
                    && let Some(on_focus) = &on_focus
                {
                    on_focus.call(render.node);
                }
            },
            onwheel: move |evt| {
                evt.prevent_default();
                // The house wheel grammar (shared code with the mapping
                // editor): scroll pans, ⌘/Ctrl-scroll zooms at the pointer.
                let point = evt.data().client_coordinates();
                let mut next = view_box.peek().unwrap_or(vb);
                match lpa_mapping_editor::wheel_gesture(&evt) {
                    lpa_mapping_editor::WheelGesture::Pan { dx, dy } => {
                        next[0] -= f64::from(dx) * units_per_px;
                        next[1] -= f64::from(dy) * units_per_px;
                    }
                    lpa_mapping_editor::WheelGesture::Zoom { factor } => {
                        let anchor_doc =
                            event_doc_point(&anchor.peek(), next, [point.x, point.y])
                                .unwrap_or([next[0] + next[2] / 2.0, next[1] + next[3] / 2.0]);
                        let factor = f64::from(factor).clamp(0.5, 2.0);
                        let width = (next[2] / factor).clamp(1.0, 1_000_000.0);
                        let height = (next[3] / factor).clamp(1.0, 1_000_000.0);
                        // Keep the doc point under the pointer fixed.
                        let fx = (anchor_doc[0] - next[0]) / next[2];
                        let fy = (anchor_doc[1] - next[1]) / next[3];
                        next = [
                            anchor_doc[0] - fx * width,
                            anchor_doc[1] - fy * height,
                            width,
                            height,
                        ];
                    }
                }
                view_box.set(Some(next));
            },
            for render in renders_now.iter() {
                FixtureGroup {
                    key: "{render.key}",
                    render: render.clone(),
                    transform: drag_override
                        .read()
                        .as_ref()
                        .filter(|over| over.key == render.key)
                        .map(|over| over.transform)
                        .unwrap_or(render.transform),
                    selected: selection_touches(&selection, render.node),
                    selected_instance: selected_instance_range(&selection, render),
                    units_per_px,
                }
            }
        }
        }
    }
}

/// Loose transform equality at the kernel's canonical precision (writes
/// quantize to 4 decimals).
fn transforms_close(a: &UiArrangeTransform, b: &UiArrangeTransform) -> bool {
    let close = |x: f64, y: f64| (x - y).abs() < 1e-3;
    close(a.t[0], b.t[0]) && close(a.t[1], b.t[1]) && close(a.r, b.r) && close(a.s, b.s)
}

/// Client coordinates → conceptual-space coordinates, through the mounted
/// rect and the viewBox (`xMidYMid meet`: uniform scale, centered).
fn event_doc_point(anchor: &CanvasAnchor, vb: [f64; 4], client: [f64; 2]) -> Option<[f64; 2]> {
    let origin = anchor.origin();
    let size = anchor.size()?;
    let scale = (f64::from(size[0]) / vb[2]).min(f64::from(size[1]) / vb[3]);
    if scale <= 0.0 {
        return None;
    }
    let offset = [
        (f64::from(size[0]) - vb[2] * scale) / 2.0,
        (f64::from(size[1]) - vb[3] * scale) / 2.0,
    ];
    Some([
        vb[0] + (client[0] - f64::from(origin[0]) - offset[0]) / scale,
        vb[1] + (client[1] - f64::from(origin[1]) - offset[1]) / scale,
    ])
}

/// Topmost fixture whose (padded) transformed frame contains the point —
/// the inverse of the group transform, tested in the fixture's own space.
fn hit_fixture(renders: &[FixtureRender], doc: [f64; 2]) -> Option<&FixtureRender> {
    const HIT_PAD: f64 = 8.0;
    renders.iter().rev().find(|render| {
        let [lx, ly] = inverse_transform_point(&render.transform, doc);
        let [bx, by, bw, bh] = render.bounds;
        lx >= bx - HIT_PAD
            && lx <= bx + bw + HIT_PAD
            && ly >= by - HIT_PAD
            && ly <= by + bh + HIT_PAD
    })
}

/// One fixture on the canvas: name-tag frame above the content (labels
/// never overlap geometry — the round-2 collision fix), body under it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FixtureGroup(
    render: FixtureRender,
    transform: UiArrangeTransform,
    selected: bool,
    /// Selected instance's `(start, lamps)` window, for lamp rings.
    selected_instance: Option<(u32, u32)>,
    units_per_px: f64,
) -> Element {
    let [bx, by, bw, bh] = render.bounds;
    let group_transform = format!(
        "translate({} {}) rotate({}) scale({})",
        transform.t[0], transform.t[1], transform.r, transform.s
    );
    // Screen-constant strokes/text inside a scaled group: counter the
    // group scale on top of the viewBox scale.
    let upp = units_per_px / transform.s.max(1e-6);
    let tag_font = 11.0 * upp;
    let lamp_r = (3.0 * upp).min(bw.max(bh) / 18.0).max(0.35 * upp);
    let frame_stroke = if selected {
        format!("stroke:#4c9ffe;stroke-width:{}", 1.5 * upp)
    } else {
        format!(
            "stroke:var(--color-border-strong);stroke-width:{}",
            1.0 * upp
        )
    };
    let frame_pad = 6.0 * upp;
    let tag_y = by - frame_pad - 4.0 * upp;
    rsx! {
        // Purely visual: the canvas-level hit-test owns every gesture.
        g {
            transform: "{group_transform}",
            style: "cursor: grab; pointer-events: none;",
            // The frame: hit target + selection highlight.
            rect {
                x: "{bx - frame_pad}",
                y: "{by - frame_pad}",
                width: "{bw + 2.0 * frame_pad}",
                height: "{bh + 2.0 * frame_pad}",
                rx: "{4.0 * upp}",
                fill: if selected { "rgba(76,159,254,0.06)" } else { "transparent" },
                style: "{frame_stroke}",
                stroke_dasharray: if render.arranged { "" } else { "3 3" },
            }
            // The name tag, above the content.
            text {
                x: "{bx - frame_pad}",
                y: "{tag_y}",
                font_size: "{tag_font}",
                fill: if selected { "#4c9ffe" } else { "var(--color-muted-foreground)" },
                style: "font-weight: 600; user-select: none;",
                "{render.label}"
            }
            match &render.body {
                FixtureBody::Lamps { points, total } => rsx! {
                    for (index, point) in points.iter().enumerate() {
                        circle {
                            key: "{index}",
                            cx: "{point[0]}",
                            cy: "{point[1]}",
                            r: "{lamp_r}",
                            fill: "{render.color}",
                            fill_opacity: "0.9",
                        }
                    }
                    if let Some((start, lamps)) = selected_instance {
                        // Ring the selected instance's lamps (subsample-aware:
                        // ring what is drawn).
                        {
                            let stride = (*total as usize).div_ceil(points.len().max(1)).max(1);
                            rsx! {
                                for (index, point) in points.iter().enumerate() {
                                    if (index * stride) as u32 >= start && ((index * stride) as u32) < start + lamps {
                                        circle {
                                            key: "ring-{index}",
                                            cx: "{point[0]}",
                                            cy: "{point[1]}",
                                            r: "{lamp_r + 1.2 * upp}",
                                            fill: "none",
                                            stroke: "#4c9ffe",
                                            stroke_width: "{0.9 * upp}",
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                FixtureBody::Placeholder { lamps } => rsx! {
                    rect {
                        x: "{bx}",
                        y: "{by}",
                        width: "{bw}",
                        height: "{bh}",
                        rx: "{3.0 * upp}",
                        fill: "var(--color-card-muted)",
                        stroke: "var(--color-border-strong)",
                        stroke_width: "{1.0 * upp}",
                    }
                    text {
                        x: "{bx + bw / 2.0}",
                        y: "{by + bh / 2.0}",
                        text_anchor: "middle",
                        dominant_baseline: "middle",
                        font_size: "{10.0 * upp}",
                        fill: "var(--color-dim-foreground)",
                        style: "user-select: none;",
                        "{lamps} lamps · not loaded"
                    }
                },
                FixtureBody::Strip { lamps } => rsx! {
                    // The honest range ribbon: dashed bar + lamp dots.
                    rect {
                        x: "{bx}",
                        y: "{by}",
                        width: "{bw}",
                        height: "{bh}",
                        rx: "{bh / 2.0}",
                        fill: "none",
                        stroke: "{render.color}",
                        stroke_width: "{1.0 * upp}",
                        stroke_dasharray: "4 3",
                    }
                    {
                        let count = (*lamps).min(60) as usize;
                        let step = bw / count.max(1) as f64;
                        rsx! {
                            for index in 0..count {
                                circle {
                                    key: "{index}",
                                    cx: "{bx + step * (index as f64 + 0.5)}",
                                    cy: "{by + bh / 2.0}",
                                    r: "{lamp_r * 0.8}",
                                    fill: "{render.color}",
                                    fill_opacity: "0.85",
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// Does the selection concern this fixture (any grain)?
fn selection_touches(selection: &Option<UiPatchTarget>, node: NodeId) -> bool {
    match selection {
        Some(UiPatchTarget::Fixture { node: n })
        | Some(UiPatchTarget::Instance { node: n, .. })
        | Some(UiPatchTarget::Range { node: n, .. }) => *n == node,
        _ => false,
    }
}

/// The selected instance's lamp window on this fixture, when one is.
fn selected_instance_range(
    selection: &Option<UiPatchTarget>,
    render: &FixtureRender,
) -> Option<(u32, u32)> {
    match selection {
        Some(UiPatchTarget::Instance { node, path }) if *node == render.node => render
            .instances
            .iter()
            .find(|(p, _, _)| p == path)
            .map(|(_, start, lamps)| (*start, *lamps)),
        Some(UiPatchTarget::Range { node, start, count }) if *node == render.node => {
            Some((*start, count.unwrap_or(u32::MAX)))
        }
        _ => None,
    }
}

/// Sticky auto-pack slots for the UNARRANGED fixtures, keyed by editor
/// key. Owned by the shell and refreshed only when the unarranged SET
/// changes — dragging an arranged fixture must never move its neighbours
/// (the second gate's movement bug: the pack row used to follow the
/// arranged content every render).
pub(crate) type PackSlots = BTreeMap<String, UiArrangeTransform>;

/// The pack layout for the CURRENT unarranged set: `None` when the held
/// slots already cover exactly that set (keep them — stability is the
/// point), else a freshly packed row below the arranged content.
pub(crate) fn refresh_pack_slots(
    surface: &UiPatchSurface,
    bodies: &BTreeMap<ArtifactLocation, String>,
    held: &PackSlots,
) -> Option<PackSlots> {
    let renders = build_renders(surface, bodies, &PackSlots::new());
    let unarranged: Vec<&FixtureRender> =
        renders.iter().filter(|render| !render.arranged).collect();
    if unarranged.len() == held.len()
        && unarranged
            .iter()
            .all(|render| held.contains_key(&render.key))
    {
        return None;
    }
    Some(
        unarranged
            .into_iter()
            .map(|render| (render.key.clone(), render.transform))
            .collect(),
    )
}

/// Build every fixture's render facts: resolve loaded bodies, fall back to
/// footprint blocks and range strips, then auto-pack the unarranged into
/// the bottom row (held slots win — see [`PackSlots`]).
fn build_renders(
    surface: &UiPatchSurface,
    bodies: &BTreeMap<ArtifactLocation, String>,
    pack: &PackSlots,
) -> Vec<FixtureRender> {
    let mut renders: Vec<FixtureRender> = Vec::new();
    for (index, fixture) in surface.fixtures.iter().enumerate() {
        let Some(key) = fixture.address.clone() else {
            continue;
        };
        let arrange = fixture.arrange.clone().unwrap_or_default();
        let resolved = fixture
            .mapping_artifact
            .as_ref()
            .and_then(|artifact| bodies.get(artifact))
            .and_then(|text| {
                let doc = lpc_mapping::Map2dDoc::from_json(text).ok()?;
                let resolved = lpc_mapping::resolve(&doc).ok()?;
                Some(resolved)
            });
        let (body, bounds) = match resolved {
            Some(resolved) => {
                let total = resolved.lamps.len() as u32;
                let stride = resolved.lamps.len().div_ceil(MAX_DISPLAY_LAMPS).max(1);
                let points: Vec<[f32; 2]> = resolved
                    .lamps
                    .iter()
                    .step_by(stride)
                    .map(|lamp| lamp.pos)
                    .collect();
                let bounds = lpc_mapping::bounds_of_points(&points)
                    .map(|b| {
                        [
                            f64::from(b.min_x),
                            f64::from(b.min_y),
                            f64::from(b.width.max(1.0)),
                            f64::from(b.height.max(1.0)),
                        ]
                    })
                    .unwrap_or([0.0, 0.0, 40.0, 40.0]);
                (FixtureBody::Lamps { points, total }, bounds)
            }
            None if fixture.mapping_artifact.is_some() => {
                // A map2d exists but is not loaded: the footprint block.
                let lamps = fixture.patch.lamps;
                let bounds = arrange
                    .footprint
                    .map(|fp| fp.bbox)
                    .unwrap_or_else(|| placeholder_bounds(lamps));
                (FixtureBody::Placeholder { lamps }, bounds)
            }
            None => {
                // The peach: no map2d document at all — the range strip.
                let lamps = fixture.patch.lamps;
                let width = f64::from(lamps.max(8)) * 3.0;
                (FixtureBody::Strip { lamps }, [0.0, 0.0, width, 10.0])
            }
        };
        renders.push(FixtureRender {
            node: fixture.node,
            key,
            label: fixture.label.clone(),
            color: object_color(index),
            transform: arrange.transform,
            arranged: arrange.arranged,
            bounds,
            body,
            instances: fixture
                .instances
                .iter()
                .map(|instance| (instance.path.clone(), instance.start, instance.lamps))
                .collect(),
        });
    }
    for render in renders.iter_mut().filter(|render| !render.arranged) {
        if let Some(slot) = pack.get(&render.key) {
            render.transform = *slot;
        }
    }
    auto_pack(&mut renders, pack);
    renders
}

/// A square-ish block sized by lamp count, for footprint-less placeholders.
fn placeholder_bounds(lamps: u32) -> [f64; 4] {
    let side = (f64::from(lamps.max(1))).sqrt() * 8.0;
    [0.0, 0.0, side.max(24.0), (side * 0.62).max(16.0)]
}

/// Place every unarranged fixture WITHOUT a held slot in a bottom row
/// (stable order = surface order) under the arranged content. Ephemeral:
/// nothing is written until a fixture is first dragged.
fn auto_pack(renders: &mut [FixtureRender], held: &PackSlots) {
    let arranged_max_y = renders
        .iter()
        .filter(|render| render.arranged)
        .map(|render| {
            // Conservative world-space extent: transformed bounds corners.
            corners(render.bounds, &render.transform)
                .iter()
                .map(|point| point[1])
                .fold(f64::MIN, f64::max)
        })
        .fold(f64::MIN, f64::max);
    let row_y = if arranged_max_y == f64::MIN {
        0.0
    } else {
        arranged_max_y + PACK_GAP * 2.0
    };
    let mut cursor_x = 0.0;
    for render in renders
        .iter_mut()
        .filter(|render| !render.arranged && !held.contains_key(&render.key))
    {
        let [bx, by, bw, bh] = render.bounds;
        let _ = bh;
        render.transform = UiArrangeTransform {
            t: [cursor_x - bx, row_y - by],
            r: 0.0,
            s: 1.0,
        };
        cursor_x += bw + PACK_GAP;
    }
}

/// A doc-space point through translate ∘ rotate ∘ scale.
fn transform_point(transform: &UiArrangeTransform, point: [f64; 2]) -> [f64; 2] {
    let rad = transform.r.to_radians();
    let (sin, cos) = rad.sin_cos();
    let sx = point[0] * transform.s;
    let sy = point[1] * transform.s;
    [
        transform.t[0] + sx * cos - sy * sin,
        transform.t[1] + sx * sin + sy * cos,
    ]
}

/// The inverse of [`transform_point`].
fn inverse_transform_point(transform: &UiArrangeTransform, point: [f64; 2]) -> [f64; 2] {
    let rad = transform.r.to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = point[0] - transform.t[0];
    let dy = point[1] - transform.t[1];
    let scale = transform.s.max(1e-9);
    [
        (dx * cos + dy * sin) / scale,
        (-dx * sin + dy * cos) / scale,
    ]
}

/// The dive's context layer: every OTHER fixture's display points carried
/// into the focused fixture's doc space (inverse focused placement ∘ their
/// placement) — "others still visible", correctly placed. Placeholder and
/// strip bodies contribute nothing: context is honest geometry only.
pub(crate) fn dive_context(
    surface: &UiPatchSurface,
    bodies: &BTreeMap<ArtifactLocation, String>,
    pack: &PackSlots,
    focused: NodeId,
) -> Vec<lpa_mapping_editor::ContextFixture> {
    let renders = build_renders(surface, bodies, pack);
    let Some(focus) = renders.iter().find(|render| render.node == focused) else {
        return Vec::new();
    };
    let focus_transform = focus.transform;
    renders
        .iter()
        .filter(|render| render.node != focused)
        .filter_map(|render| {
            let FixtureBody::Lamps { points, .. } = &render.body else {
                return None;
            };
            let points = points
                .iter()
                .map(|point| {
                    let world = transform_point(
                        &render.transform,
                        [f64::from(point[0]), f64::from(point[1])],
                    );
                    let local = inverse_transform_point(&focus_transform, world);
                    [local[0] as f32, local[1] as f32]
                })
                .collect();
            Some(lpa_mapping_editor::ContextFixture {
                color: render.color.to_string(),
                points,
            })
        })
        .collect()
}

/// The four corners of `bounds` through translate·rotate·scale.
fn corners(bounds: [f64; 4], transform: &UiArrangeTransform) -> [[f64; 2]; 4] {
    let [bx, by, bw, bh] = bounds;
    [
        transform_point(transform, [bx, by]),
        transform_point(transform, [bx + bw, by]),
        transform_point(transform, [bx, by + bh]),
        transform_point(transform, [bx + bw, by + bh]),
    ]
}

/// Fit-all: the union of every fixture's transformed frame, padded.
fn fit_view_box(renders: &[FixtureRender]) -> Option<[f64; 4]> {
    let mut min = [f64::MAX, f64::MAX];
    let mut max = [f64::MIN, f64::MIN];
    for render in renders {
        for corner in corners(render.bounds, &render.transform) {
            min[0] = min[0].min(corner[0]);
            min[1] = min[1].min(corner[1]);
            max[0] = max[0].max(corner[0]);
            max[1] = max[1].max(corner[1]);
        }
    }
    if min[0] > max[0] {
        return None;
    }
    let width = (max[0] - min[0]).max(1.0);
    let height = (max[1] - min[1]).max(1.0);
    // Generous top padding: name tags sit above frames.
    let pad_x = width * 0.10 + 10.0;
    let pad_y = height * 0.14 + 14.0;
    Some([
        min[0] - pad_x,
        min[1] - pad_y,
        width + 2.0 * pad_x,
        height + 2.0 * pad_y,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(key: &str, arranged: bool, bounds: [f64; 4]) -> FixtureRender {
        FixtureRender {
            node: NodeId::new(1),
            key: key.to_string(),
            label: key.to_string(),
            color: "#fff",
            transform: if arranged {
                UiArrangeTransform {
                    t: [50.0, 60.0],
                    r: 0.0,
                    s: 1.0,
                }
            } else {
                UiArrangeTransform::default()
            },
            arranged,
            bounds,
            body: FixtureBody::Placeholder { lamps: 10 },
            instances: Vec::new(),
        }
    }

    #[test]
    fn auto_pack_rows_are_stable_and_below_arranged_content() {
        let mut renders = vec![
            render("a", true, [0.0, 0.0, 40.0, 40.0]),
            render("b", false, [10.0, 10.0, 30.0, 20.0]),
            render("c", false, [0.0, 0.0, 20.0, 20.0]),
        ];
        auto_pack(&mut renders, &PackSlots::new());
        // The arranged fixture is untouched.
        assert_eq!(renders[0].transform.t, [50.0, 60.0]);
        // Unarranged fixtures pack left-to-right in order, below the
        // arranged extent (arranged bottom = 100), tops aligned.
        let row_y = 100.0 + PACK_GAP * 2.0;
        assert_eq!(renders[1].transform.t, [-10.0, row_y - 10.0]);
        assert_eq!(renders[2].transform.t, [30.0 + PACK_GAP, row_y]);
        // Recomputing changes nothing (stable).
        let again = renders.clone();
        auto_pack(&mut renders, &PackSlots::new());
        assert_eq!(
            renders.iter().map(|r| r.transform).collect::<Vec<_>>(),
            again.iter().map(|r| r.transform).collect::<Vec<_>>(),
        );
    }

    /// The movement bug's regression test: held slots survive arranged
    /// content moving; only a CHANGED unarranged set re-packs.
    #[test]
    fn held_pack_slots_pin_unarranged_fixtures() {
        let mut renders = vec![
            render("a", true, [0.0, 0.0, 40.0, 40.0]),
            render("b", false, [0.0, 0.0, 20.0, 20.0]),
        ];
        let held: PackSlots = [(
            "b".to_string(),
            UiArrangeTransform {
                t: [7.0, 9.0],
                r: 0.0,
                s: 1.0,
            },
        )]
        .into_iter()
        .collect();
        for item in renders.iter_mut().filter(|item| !item.arranged) {
            if let Some(slot) = held.get(&item.key) {
                item.transform = *slot;
            }
        }
        auto_pack(&mut renders, &held);
        assert_eq!(
            renders[1].transform.t,
            [7.0, 9.0],
            "the held slot pins the fixture regardless of arranged bounds"
        );
    }

    #[test]
    fn corners_rotate_about_the_group_origin() {
        let transform = UiArrangeTransform {
            t: [10.0, 0.0],
            r: 90.0,
            s: 1.0,
        };
        let points = corners([0.0, 0.0, 4.0, 2.0], &transform);
        // (4, 0) rotates to (0, 4), then translates by (10, 0).
        assert!((points[1][0] - 10.0).abs() < 1e-9);
        assert!((points[1][1] - 4.0).abs() < 1e-9);
    }

    #[test]
    fn fit_view_box_covers_every_transformed_frame() {
        let renders = vec![
            render("a", true, [0.0, 0.0, 40.0, 40.0]),
            render("b", false, [0.0, 0.0, 20.0, 20.0]),
        ];
        let vb = fit_view_box(&renders).expect("bounds");
        assert!(vb[0] <= 0.0 && vb[1] <= 0.0);
        assert!(vb[0] + vb[2] >= 90.0, "{vb:?}");
    }
}
