//! The editor canvas: doc-space SVG rendering with camera pan/zoom and the
//! full editing grammar (P3).
//!
//! Layers (bottom → top): dot-grid background, the authored canvas rect,
//! the fit-preview overlay, wiring arrows, lamps, wiring numbers, the
//! selection outline + corner resize handles, path vertex handles, the
//! path-draft preview, and the marquee. The canvas renders **doc space**;
//! the camera maps doc units to CSS pixels.
//!
//! Every mutation flows through `MapEditorSession` ops: drags run
//! `begin_gesture` → `*_from_gesture` (totals, no drift) → `commit_gesture`
//! at pointer-up, so one drag is one undo step; `on_committed` fires after
//! every committed change so the host can persist.

use dioxus::prelude::*;
use lpc_mapping::{Bounds2d, Map2dShape, ResolvedMap2d, Rotation2d, bounds_of_points, resolve};

use crate::editor_core::camera::Camera;
use crate::editor_core::editor_session::{MapEditorSession, editable_path};
use crate::editor_core::map_tool::MapTool;
use crate::editor_core::view_geometry::{ArrowInput, wiring_arrows};
use crate::view::map_editor::EditorViewOptions;

/// Object fill palette (wiring-order cycling; matches the UX spike).
const OBJECT_COLORS: &[&str] = &[
    "#5aa9e6", "#3fd68e", "#e4c065", "#c792ea", "#f0913b", "#64d8cb",
];

/// Selection accent (provisional spike blue; violet is reserved for
/// bound-state semantics elsewhere in Studio).
const SELECTION_COLOR: &str = "#4c9ffe";

#[must_use]
pub fn object_color(object_index: usize) -> &'static str {
    OBJECT_COLORS[object_index % OBJECT_COLORS.len()]
}

/// Lamp display radius in doc units: a fraction of the median consecutive
/// lamp spacing, so dense grids stay dots and sparse strips stay visible.
#[must_use]
pub fn lamp_display_radius(resolved: &ResolvedMap2d) -> f32 {
    let mut gaps: Vec<f32> = Vec::new();
    for span in &resolved.spans {
        let start = span.start as usize;
        let end = start + span.count as usize;
        for index in start..end.saturating_sub(1) {
            let a = resolved.lamps[index].pos;
            let b = resolved.lamps[index + 1].pos;
            let gap = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            if gap > f32::EPSILON {
                gaps.push(gap);
            }
        }
    }
    if gaps.is_empty() {
        return 7.0;
    }
    gaps.sort_by(|a, b| a.partial_cmp(b).expect("finite gaps"));
    (gaps[gaps.len() / 2] * 0.30).clamp(1.5, 22.0)
}

/// The doc-space region a `target_aspect` render texture covers after
/// aspect-fit: the smallest rect with the target aspect that contains
/// `frame`, centered.
#[must_use]
pub fn fit_region(frame: Bounds2d, target_aspect: f32) -> Bounds2d {
    let frame_aspect = frame.width / frame.height.max(1e-6);
    let (width, height) = if frame_aspect >= target_aspect {
        (frame.width, frame.width / target_aspect)
    } else {
        (frame.height * target_aspect, frame.height)
    };
    Bounds2d {
        min_x: frame.min_x - (width - frame.width) / 2.0,
        min_y: frame.min_y - (height - frame.height) / 2.0,
        width,
        height,
    }
}

/// An in-flight canvas drag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasDrag {
    Pan {
        last: [f32; 2],
    },
    /// Group move: totals from the gesture-start doc point.
    Move {
        start: [f32; 2],
        moved: bool,
        /// Collapse multi-selection to this object on a no-move click.
        collapse: Option<usize>,
    },
    /// Path vertex drag.
    Vertex {
        index: usize,
        moved: bool,
    },
    /// Corner resize about the fixed opposite corner.
    Resize {
        anchor: [f32; 2],
        start: [f32; 2],
        moved: bool,
    },
    Marquee {
        start: [f32; 2],
        current: [f32; 2],
        additive: bool,
    },
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn EditorCanvas(
    session: Signal<MapEditorSession>,
    camera: Signal<Camera>,
    view_opts: Signal<EditorViewOptions>,
    viewport: Signal<Option<[f32; 2]>>,
    drag: Signal<Option<CanvasDrag>>,
    /// Live lamp colors indexed by wiring index (host feed, written by
    /// [`super::map_editor::MapEditor`]). A color frame must NOT re-render
    /// this component: the VDOM owns each lamp's palette `fill` attribute,
    /// and a post-render effect overrides via inline `style` — per-frame
    /// colors are direct DOM writes, never a 1500-node diff.
    live_feed: Signal<Vec<[u8; 3]>>,
    /// Fired after any committed (undoable) change.
    on_committed: EventHandler<()>,
    /// Host-owned reference image, rendered doc-space at the origin between
    /// the dot grid and the authored canvas rect when `view_opts.reference`.
    #[props(default)]
    reference: Option<crate::view::reference::ReferenceImage>,
    /// Neighbour fixtures rendered dimmed under the document (points
    /// already in THIS doc's space) — the dive's "others still visible".
    #[props(default)]
    context: Vec<crate::view::context_layer::ContextFixture>,
) -> Element {
    // Pointer/wheel math anchors to the mounted svg's live rect, and the
    // measured size feeds the host's viewport signal (fit needs real
    // dimensions — the embed is nothing like a full window).
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(unused_mut, reason = "only the wasm mount handler writes it")
    )]
    let mut anchor = use_signal(CanvasAnchor::default);
    let mut viewport = viewport;
    let mut measure = move || {
        if let Some(size) = anchor.peek().size()
            && viewport.peek().as_ref() != Some(&size)
        {
            viewport.set(Some(size));
        }
    };
    // Element `onresize` does not fire for the svg in this stack, so a real
    // ResizeObserver keeps `viewport` tracking the box (rail dock,
    // full-page expand). Held for the canvas's lifetime; disconnects on
    // drop (the popover panel-observer precedent). The observer routes
    // through a Dioxus [`Callback`], NOT a bare closure: a signal write
    // from a raw JS callback has no runtime context, so subscribers (the
    // fit effect) would never be notified.
    let measure_cb = use_callback(move |()| measure());
    #[cfg(target_arch = "wasm32")]
    let resize_observer =
        use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(None::<CanvasResizeObserver>)));
    // One id per mounted canvas so the live-color effect scopes its DOM
    // writes to THIS editor (face + page can mount simultaneously).
    let canvas_dom_id = use_hook(|| {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("lpme-canvas-{id}")
    });

    // Live colors as direct DOM writes (P2 of the view/edit-split plan):
    // subscribe to the feed and the session (a doc edit rebuilds the lamp
    // nodes, so the override must re-apply after that render), then write
    // inline `style` fills. The VDOM owns the `fill` ATTRIBUTE (palette);
    // this effect owns inline style ONLY — one writer per
    // surface, so the two never fight.
    {
        let canvas_dom_id = canvas_dom_id.clone();
        use_effect(move || {
            let colors = live_feed();
            let live_on = view_opts().live;
            // Subscribe to session so a doc-edit re-render re-applies the
            // overrides to the rebuilt nodes.
            let _revision_witness = session.read().doc().objects.len();
            apply_live_fills(&canvas_dom_id, live_on, &colors);
        });
    }

    let cam = camera();
    let opts = view_opts();
    let session_read = session.read();
    let doc = session_read.doc();
    let tool_is_select = matches!(session_read.tool, MapTool::Select);
    let tool = session_read.tool.clone();
    let selection = session_read.selection.clone();
    // Authored polylines, repeats included: a repeated path's editable
    // geometry is instance 0 — the unrotated inner path — so that is what
    // takes clicks and shows handles.
    let path_objects: Vec<(usize, Vec<[f32; 2]>)> = doc
        .objects
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            editable_path(&object.shape).map(|path| (index, path.points.clone()))
        })
        .collect();
    // Inert (jumper) segments carry no lamps, so nothing else on the canvas
    // shows them: draw the wire itself, dashed and dimmed. Gap indices always
    // name authored segments — `reversed` mirrors them in the resolver so the
    // same physical run stays inert — so no direction handling is needed here.
    // Under a repeat this draws instance 0's jumpers; the other instances are
    // the same wire turned, and their lamps already show the shape.
    let gap_segments: Vec<[[f32; 2]; 2]> = doc
        .objects
        .iter()
        .filter_map(|object| editable_path(&object.shape))
        .flat_map(|path| {
            path.gaps.iter().filter_map(|gap| {
                let start = path.points.get(*gap as usize)?;
                let end = path.points.get(*gap as usize + 1)?;
                Some([*start, *end])
            })
        })
        .collect();
    // Repeat affordances for the selected object: the point it turns about,
    // and — for a repeated polyline — where the other instances of that
    // polyline run, which lamps alone leave to inference.
    let repeat_center: Option<[f32; 2]> = selection
        .single()
        .and_then(|path| path.resolve(doc))
        .and_then(|shape| match shape {
            Map2dShape::Repeat(repeat) => Some(repeat.center),
            _ => None,
        });
    let ghost_outlines: Vec<Vec<[f32; 2]>> = selection
        .single()
        .and_then(|path| path.resolve(doc))
        .and_then(|shape| match shape {
            Map2dShape::Repeat(repeat) => {
                let path = editable_path(&repeat.shape)?;
                Some(
                    (1..repeat.count)
                        .map(|instance| {
                            let rotation =
                                Rotation2d::about(repeat.center, repeat.instance_degrees(instance));
                            path.points.iter().map(|p| rotation.apply(*p)).collect()
                        })
                        .collect(),
                )
            }
            _ => None,
        })
        .unwrap_or_default();
    let resolved = resolve(doc).unwrap_or(ResolvedMap2d {
        lamps: Vec::new(),
        spans: Vec::new(),
    });
    // Tessellation context (selection/tree ADR): a selected or scoped
    // repeat renders instance-by-instance — a distinct hue per span so the
    // tessellation reads at a glance — and while DESCENDED, non-primary
    // instances are inert live previews (the primary is the only handle).
    let tessellating: std::collections::BTreeSet<usize> = selection
        .paths()
        .map(|path| path.object)
        .filter(|object| {
            matches!(
                doc.objects.get(*object).map(|o| &o.shape),
                Some(Map2dShape::Repeat(_))
            )
        })
        .collect();
    let scoped_object: Option<usize> = selection.scope().map(|scope| scope.object);
    // Per-span instance ordinals for the tessellating objects (spans are in
    // wiring order, so the ordinal among an object's spans IS the instance).
    let span_instances: Vec<(u32, u32, usize)> = {
        let mut ordinals = std::collections::BTreeMap::<u32, usize>::new();
        resolved
            .spans
            .iter()
            .filter(|span| tessellating.contains(&(span.object as usize)))
            .map(|span| {
                let ordinal = ordinals.entry(span.object).or_insert(0);
                let instance = *ordinal;
                *ordinal += 1;
                (span.start, span.count, instance)
            })
            .collect()
    };
    let instance_of = |lamp_index: u32| -> Option<usize> {
        let slot = span_instances.partition_point(|(start, count, _)| start + count <= lamp_index);
        span_instances
            .get(slot)
            .filter(|(start, _, _)| *start <= lamp_index)
            .map(|(_, _, instance)| *instance)
    };
    // Lamp screen size is CAPPED: proportional at fit zoom, but circles stop
    // growing past ~11px screen radius as you zoom in — near-coincident
    // lamps (out-and-back wiring runs) separate visually instead of staying
    // stacked at every zoom. A floor keeps dots visible when zoomed far out
    // and keeps sparse layouts (big gaps → tiny proportional dots)
    // readable and clickable.
    let radius = {
        let base = lamp_display_radius(&resolved);
        (base * cam.scale).clamp(5.0, 11.0) / cam.scale
    };
    // Pointer targets never shrink below ~9px screen radius: sparse lamps
    // get an invisible hit ring so clicking doesn't demand pixel aim.
    let hit_radius = (9.0 / cam.scale).max(radius);
    let canvas_rect = doc.canvas_bounds();
    let fit_rect = opts.fit_preview.then(|| {
        let frame = canvas_rect
            .or_else(|| bounds_of_points(&resolved.positions()))
            .unwrap_or(Bounds2d {
                min_x: 0.0,
                min_y: 0.0,
                width: 100.0,
                height: 100.0,
            });
        fit_region(frame, 1.0)
    });
    let spans: Vec<(u32, u32)> = resolved
        .spans
        .iter()
        .map(|span| (span.start, span.count))
        .collect();
    let positions = resolved.positions();
    let arrows = opts.arrows.then(|| {
        wiring_arrows(&ArrowInput {
            positions: &positions,
            spans: &spans,
            view_width: 0.0,
            view_height: 0.0,
            end_gap: radius * 1.05,
            min_len: radius * 2.3,
        })
    });
    let show_numbers = opts.numbers && cam.scale * radius >= 5.0;

    // Selection visuals: bbox of the selected objects' lamps.
    let selection_bounds = (!selection.is_empty())
        .then(|| {
            let sel_positions: Vec<[f32; 2]> = resolved
                .lamps
                .iter()
                .filter(|lamp| selection.object_selected(lamp.object as usize))
                .map(|lamp| lamp.pos)
                .collect();
            bounds_of_points(&sel_positions)
        })
        .flatten();
    let handle_half = 5.0 / cam.scale;
    let selection_margin = radius + 8.0 / cam.scale;

    // Vertex handles for a single selected path — the inner path through a
    // repeat, whose other instances follow the drag on the next resolve.
    let vertex_points: Vec<[f32; 2]> = selection
        .single()
        .and_then(|path| path.resolve(doc))
        .filter(|_| tool_is_select)
        .and_then(editable_path)
        .map(|path| path.points.clone())
        .unwrap_or_default();
    let selected_vertex = selection.vertex;

    // Path draft preview: draft vertices + resolved ghost lamps + the chain
    // link from the previous object's last lamp.
    let draft_points: Vec<[f32; 2]> = match &tool {
        MapTool::Path { draft } => draft.clone(),
        _ => Vec::new(),
    };
    let draft_ghosts: Vec<[f32; 2]> = if draft_points.len() >= 2 {
        let length: f32 = draft_points
            .windows(2)
            .map(|pair| {
                ((pair[1][0] - pair[0][0]).powi(2) + (pair[1][1] - pair[0][1]).powi(2)).sqrt()
            })
            .sum();
        let count = ((length / 26.0).round() as u32).max(2);
        let ghost_doc = lpc_mapping::Map2dDoc {
            objects: vec![lpc_mapping::Map2dObject {
                name: String::new(),
                id: None,
                stride: None,
                shape: Map2dShape::Path(lpc_mapping::PathShape {
                    points: draft_points.clone(),
                    count,
                    reversed: false,
                    gaps: Vec::new(),
                }),
            }],
            ..lpc_mapping::Map2dDoc::new()
        };
        resolve(&ghost_doc)
            .map(|resolved| resolved.positions())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let chain_from = (!draft_points.is_empty())
        .then(|| positions.last().copied())
        .flatten();

    let marquee_rect = match drag() {
        Some(CanvasDrag::Marquee { start, current, .. }) => {
            let min = [start[0].min(current[0]), start[1].min(current[1])];
            let max = [start[0].max(current[0]), start[1].max(current[1])];
            Some((min, [max[0] - min[0], max[1] - min[1]]))
        }
        _ => None,
    };
    drop(session_read);

    let commit_drag = move |mut session: Signal<MapEditorSession>| {
        session.write().commit_gesture();
        on_committed.call(());
    };

    rsx! {
        svg {
            id: "{canvas_dom_id}",
            class: if tool_is_select { "lpme-canvas" } else { "lpme-canvas lpme-canvas-tool" },
            onmounted: move |evt| {
                #[cfg(target_arch = "wasm32")]
                if let Some(element) = evt.data().downcast::<web_sys::Element>() {
                    anchor.set(CanvasAnchor {
                        element: Some(element.clone()),
                    });
                    *resize_observer.borrow_mut() =
                        CanvasResizeObserver::install(element, move || measure_cb.call(()));
                }
                #[cfg(not(target_arch = "wasm32"))]
                let _ = (&evt, &measure_cb);
                measure();
            },
            // Right-drag pans regardless of tool (the context menu is
            // suppressed below); left button keeps the tool grammar.
            onpointerdown: move |evt| {
                if secondary_button(&evt) {
                    capture_pointer(&evt);
                    drag.set(Some(CanvasDrag::Pan {
                        last: event_view_point(&anchor, &evt),
                    }));
                    return;
                }
                capture_pointer(&evt);
                let doc_point = event_doc_point(&anchor, &camera, &evt);
                let shift = evt.data().modifiers().shift();
                let tool_now = session.read().tool.clone();
                match tool_now {
                    MapTool::Select => {
                        if !shift {
                            session.write().selection.clear();
                        }
                        drag.set(Some(CanvasDrag::Marquee {
                            start: doc_point,
                            current: doc_point,
                            additive: shift,
                        }));
                    }
                    MapTool::Grid => {
                        session.write().create_default_grid(doc_point);
                        on_committed.call(());
                    }
                    MapTool::Ring => {
                        session.write().create_default_ring(doc_point);
                        on_committed.call(());
                    }
                    MapTool::Path { .. } => {
                        session.write().path_add_point(doc_point);
                    }
                }
            },
            onpointermove: move |evt| {
                let Some(current_drag) = drag() else {
                    return;
                };
                let doc_point = event_doc_point(&anchor, &camera, &evt);
                match current_drag {
                    CanvasDrag::Pan { last } => {
                        let view_point = event_view_point(&anchor, &evt);
                        camera.write().pan(view_point[0] - last[0], view_point[1] - last[1]);
                        drag.set(Some(CanvasDrag::Pan { last: view_point }));
                    }
                    CanvasDrag::Move { start, collapse, .. } => {
                        session
                            .write()
                            .move_selected_from_gesture(doc_point[0] - start[0], doc_point[1] - start[1]);
                        drag.set(Some(CanvasDrag::Move { start, moved: true, collapse }));
                    }
                    CanvasDrag::Vertex { index, .. } => {
                        session.write().move_vertex_from_gesture(index, doc_point);
                        drag.set(Some(CanvasDrag::Vertex { index, moved: true }));
                    }
                    CanvasDrag::Resize { anchor, start, .. } => {
                        let start_dist =
                            ((start[0] - anchor[0]).powi(2) + (start[1] - anchor[1]).powi(2)).sqrt();
                        if start_dist > 1e-3 {
                            let now_dist = ((doc_point[0] - anchor[0]).powi(2)
                                + (doc_point[1] - anchor[1]).powi(2))
                            .sqrt();
                            session
                                .write()
                                .scale_selected_from_gesture(anchor, now_dist / start_dist);
                        }
                        drag.set(Some(CanvasDrag::Resize { anchor, start, moved: true }));
                    }
                    CanvasDrag::Marquee { start, additive, .. } => {
                        drag.set(Some(CanvasDrag::Marquee {
                            start,
                            current: doc_point,
                            additive,
                        }));
                    }
                }
            },
            onpointerup: move |_| {
                let Some(current_drag) = drag() else {
                    return;
                };
                drag.set(None);
                match current_drag {
                    CanvasDrag::Pan { .. } => {}
                    CanvasDrag::Move { moved, collapse, .. } => {
                        if moved {
                            commit_drag(session);
                        } else {
                            let mut s = session.write();
                            s.commit_gesture();
                            if let Some(index) = collapse {
                                s.selection.select_only(index);
                            }
                        }
                    }
                    CanvasDrag::Vertex { moved, .. } | CanvasDrag::Resize { moved, .. } => {
                        if moved {
                            commit_drag(session);
                        } else {
                            session.write().commit_gesture();
                        }
                    }
                    CanvasDrag::Marquee { start, current, additive } => {
                        let min = [start[0].min(current[0]), start[1].min(current[1])];
                        let max = [start[0].max(current[0]), start[1].max(current[1])];
                        let scale = camera().scale;
                        if (max[0] - min[0]) * scale > 4.0 || (max[1] - min[1]) * scale > 4.0 {
                            session.write().marquee_select(min, max, additive);
                        }
                    }
                }
            },
            onpointerleave: move |_| {
                if matches!(
                    drag(),
                    Some(CanvasDrag::Pan { .. }) | Some(CanvasDrag::Marquee { .. })
                ) {
                    drag.set(None);
                }
            },
            onpointercancel: move |_| drag.set(None),
            oncontextmenu: move |evt| evt.prevent_default(),
            ondoubleclick: move |_| {
                if matches!(session.read().tool, MapTool::Path { .. })
                    && session.write().path_finish().is_some()
                {
                    on_committed.call(());
                }
            },
            onwheel: move |evt| {
                evt.prevent_default();
                // The house wheel grammar, shared with the arrange level
                // (one control scheme via one code).
                match crate::view::wheel::wheel_gesture(&evt) {
                    crate::view::wheel::WheelGesture::Zoom { factor } => {
                        let view_point = event_view_point_wheel(&anchor, &evt);
                        camera.write().zoom_at(view_point, factor);
                    }
                    crate::view::wheel::WheelGesture::Pan { dx, dy } => {
                        camera.write().pan(dx, dy);
                    }
                }
            },
            defs {
                pattern {
                    id: "lpme-dots",
                    width: "28",
                    height: "28",
                    pattern_units: "userSpaceOnUse",
                    circle { cx: "1", cy: "1", r: "1", fill: "rgba(255, 255, 255, 0.06)" }
                }
                marker {
                    id: "lpme-arrow-head",
                    view_box: "0 0 8 8",
                    ref_x: "7",
                    ref_y: "4",
                    marker_width: "4",
                    marker_height: "4",
                    orient: "auto-start-reverse",
                    // Opaque fill: the translucent line ends under the head,
                    // and alpha-stacking there reads as a glitch.
                    path { d: "M0,0.8 L7.4,4 L0,7.2 z", fill: "#c6ccd4" }
                }
                marker {
                    id: "lpme-arrow-head-chain",
                    view_box: "0 0 8 8",
                    ref_x: "7",
                    ref_y: "4",
                    marker_width: "4",
                    marker_height: "4",
                    orient: "auto-start-reverse",
                    path { d: "M0,0.8 L7.4,4 L0,7.2 z", fill: "#e4c065" }
                }
            }
            g {
                transform: "translate({cam.x},{cam.y}) scale({cam.scale})",
                rect {
                    x: "-100000",
                    y: "-100000",
                    width: "200000",
                    height: "200000",
                    fill: "url(#lpme-dots)",
                }
                // Neighbour fixtures, dimmed, under everything authored:
                // context for the dive, never targets (no pointer events).
                for (context_index, neighbour) in context.iter().enumerate() {
                    g {
                        key: "ctx-{context_index}",
                        opacity: "0.3",
                        "pointer-events": "none",
                        for (point_index, point) in neighbour.points.iter().enumerate() {
                            circle {
                                key: "{point_index}",
                                cx: "{point[0]}",
                                cy: "{point[1]}",
                                r: "{radius * 0.8}",
                                fill: "{neighbour.color}",
                            }
                        }
                    }
                }
                // Tracing layer: under everything authored, over the grid.
                // Explicit width/height — a viewBox-only SVG has no usable
                // intrinsic size (see `ReferenceImage::size`). `<image>`
                // executes no scripts; foreign SVG stays out of the DOM.
                if let Some(image) = reference.as_ref().filter(|_| opts.reference) {
                    // dioxus-html's svg `image` element carries no typed
                    // attributes; quoted names emit them verbatim.
                    image {
                        "href": "{image.data_url}",
                        "x": "0",
                        "y": "0",
                        "width": "{image.size[0]}",
                        "height": "{image.size[1]}",
                        "opacity": "{image.opacity}",
                        "pointer-events": "none",
                    }
                }
                if let Some(rect) = canvas_rect {
                    rect {
                        class: "lpme-canvas-rect",
                        x: "{rect.min_x}",
                        y: "{rect.min_y}",
                        width: "{rect.width}",
                        height: "{rect.height}",
                        stroke_width: "{1.5 / cam.scale}",
                    }
                }
                if let Some(rect) = fit_rect {
                    rect {
                        class: "lpme-fit-rect",
                        x: "{rect.min_x}",
                        y: "{rect.min_y}",
                        width: "{rect.width}",
                        height: "{rect.height}",
                        stroke_width: "{2.0 / cam.scale}",
                    }
                    text {
                        class: "lpme-fit-label",
                        x: "{rect.min_x + 6.0 / cam.scale}",
                        y: "{rect.min_y - 8.0 / cam.scale}",
                        font_size: "{12.0 / cam.scale}",
                        "texture frame (square target)"
                    }
                    text {
                        class: "lpme-fit-label",
                        x: "{rect.min_x + 6.0 / cam.scale}",
                        y: "{rect.min_y + 16.0 / cam.scale}",
                        font_size: "{11.0 / cam.scale}",
                        "0,0"
                    }
                    text {
                        class: "lpme-fit-label",
                        x: "{rect.min_x + rect.width - 30.0 / cam.scale}",
                        y: "{rect.min_y + 16.0 / cam.scale}",
                        font_size: "{11.0 / cam.scale}",
                        "1,0"
                    }
                    text {
                        class: "lpme-fit-label",
                        x: "{rect.min_x + 6.0 / cam.scale}",
                        y: "{rect.min_y + rect.height - 8.0 / cam.scale}",
                        font_size: "{11.0 / cam.scale}",
                        "0,1"
                    }
                    text {
                        class: "lpme-fit-label",
                        x: "{rect.min_x + rect.width - 30.0 / cam.scale}",
                        y: "{rect.min_y + rect.height - 8.0 / cam.scale}",
                        font_size: "{11.0 / cam.scale}",
                        "1,1"
                    }
                }
                if let Some(overlay) = arrows {
                    for (index, seg) in overlay.segs.iter().enumerate() {
                        line {
                            key: "{index}",
                            class: if seg.chain { "lpme-arrow-chain" } else { "lpme-arrow-wire" },
                            x1: "{seg.x1}",
                            y1: "{seg.y1}",
                            x2: "{seg.x2}",
                            y2: "{seg.y2}",
                            stroke_width: "{(radius * 0.14).clamp(0.4, 1.5)}",
                            marker_end: if seg.chain { "url(#lpme-arrow-head-chain)" } else { "url(#lpme-arrow-head)" },
                        }
                    }
                }
                // Where the other instances of a selected repeated path run.
                for (instance, outline) in ghost_outlines.iter().enumerate() {
                    polyline {
                        key: "ghost{instance}",
                        class: "lpme-ghost",
                        points: outline.iter().map(|p| format!("{},{}", p[0], p[1])).collect::<Vec<_>>().join(" "),
                        stroke_width: "{1.2 / cam.scale}",
                    }
                }
                // Jumper wire: the segments an author marked inert.
                for (index, segment) in gap_segments.iter().enumerate() {
                    line {
                        key: "gap{index}",
                        class: "lpme-gapline",
                        x1: "{segment[0][0]}",
                        y1: "{segment[0][1]}",
                        x2: "{segment[1][0]}",
                        y2: "{segment[1][1]}",
                        stroke_width: "{(1.6 / cam.scale).max(radius * 0.22)}",
                    }
                }
                // Wide invisible hit lines make skinny paths clickable.
                if tool_is_select {
                    for (object_index, points) in path_objects.iter() {
                        {
                            let object_index = *object_index;
                            rsx! {
                                polyline {
                                    key: "hit{object_index}",
                                    class: "lpme-hitline",
                                    points: points.iter().map(|p| format!("{},{}", p[0], p[1])).collect::<Vec<_>>().join(" "),
                                    stroke_width: "{14.0 / cam.scale}",
                                    onpointerdown: move |evt| {
                                        if secondary_button(&evt) {
                                            return;
                                        }
                                        evt.stop_propagation();
                                        select_and_start_move(session, drag, anchor, camera, object_index, &evt);
                                    },
                                }
                            }
                        }
                    }
                }
                for lamp in resolved
                    .lamps
                    .iter()
                    .filter(|lamp| !selection.object_selected(lamp.object as usize))
                    .chain(
                        resolved
                            .lamps
                            .iter()
                            .filter(|lamp| selection.object_selected(lamp.object as usize)),
                    )
                {
                    {
                        let object_index = lamp.object as usize;
                        let selected = selection.object_selected(object_index);
                        // Instance-by-instance rendering for a selected /
                        // scoped repeat; while scoped, instance 0 (the
                        // authored primary) is the only interactive geometry
                        // — the rest are inert live previews.
                        let instance = tessellating
                            .contains(&object_index)
                            .then(|| instance_of(lamp.index))
                            .flatten();
                        let inert =
                            scoped_object == Some(object_index) && instance.is_some_and(|i| i > 0);
                        // The ATTRIBUTE fill is the non-live look (instance
                        // hue > object palette); live output
                        // rides an inline-style override written by the
                        // live-color effect, so color frames never re-render
                        // this tree.
                        let fill = if let Some(instance) = instance {
                            OBJECT_COLORS[(object_index + instance) % OBJECT_COLORS.len()]
                                .to_string()
                        } else {
                            object_color(object_index).to_string()
                        };
                        rsx! {
                            circle {
                                key: "l{lamp.index}",
                                "data-lamp": "{lamp.index}",
                                cx: "{lamp.pos[0]}",
                                cy: "{lamp.pos[1]}",
                                r: "{radius}",
                                fill,
                                opacity: if inert { "0.6" } else { "1" },
                                pointer_events: if inert { "none" } else { "auto" },
                                stroke: if selected && !inert { SELECTION_COLOR } else { "#000" },
                                stroke_width: if selected && !inert {
                                    "{(radius * 0.28).clamp(0.6, 2.4)}"
                                } else {
                                    "{(radius * 0.12).clamp(0.2, 1.5)}"
                                },
                                cursor: if inert { "default" } else if tool_is_select { "move" } else { "crosshair" },
                                onpointerdown: move |evt| {
                                    if secondary_button(&evt) {
                                        return;
                                    }
                                    if matches!(session.read().tool, MapTool::Select) {
                                        evt.stop_propagation();
                                        select_and_start_move(session, drag, anchor, camera, object_index, &evt);
                                    }
                                },
                                ondoubleclick: move |evt| {
                                    // Double-click descends into the group
                                    // under the cursor (selection/tree ADR);
                                    // a leaf object no-ops.
                                    evt.stop_propagation();
                                    let mut s = session.write();
                                    if !s.selection.object_selected(object_index) {
                                        s.selection.select_only(object_index);
                                    }
                                    s.descend();
                                },
                            }
                            if hit_radius > radius * 1.15 && !inert {
                                circle {
                                    key: "lh{lamp.index}",
                                    class: "lpme-lamp-hit",
                                    cx: "{lamp.pos[0]}",
                                    cy: "{lamp.pos[1]}",
                                    r: "{hit_radius}",
                                    cursor: if tool_is_select { "move" } else { "crosshair" },
                                    onpointerdown: move |evt| {
                                        if secondary_button(&evt) {
                                            return;
                                        }
                                        if matches!(session.read().tool, MapTool::Select) {
                                            evt.stop_propagation();
                                            select_and_start_move(session, drag, anchor, camera, object_index, &evt);
                                        }
                                    },
                                    ondoubleclick: move |evt| {
                                        evt.stop_propagation();
                                        let mut s = session.write();
                                        if !s.selection.object_selected(object_index) {
                                            s.selection.select_only(object_index);
                                        }
                                        s.descend();
                                    },
                                }
                            }
                        }
                    }
                }
                if show_numbers {
                    for lamp in &resolved.lamps {
                        {
                            let font = if lamp.index + 1 >= 100 {
                                radius * 0.62
                            } else {
                                radius * 0.85
                            };
                            rsx! {
                                text {
                                    key: "n{lamp.index}",
                                    class: "lpme-lamp-num",
                                    x: "{lamp.pos[0]}",
                                    y: "{lamp.pos[1] + font * 0.34}",
                                    font_size: "{font}",
                                    stroke_width: "{font * 0.16}",
                                    text_anchor: "middle",
                                    "{lamp.index + 1}"
                                }
                            }
                        }
                    }
                }
                if let Some(bounds) = selection_bounds {
                    rect {
                        class: "lpme-sel-outline",
                        x: "{bounds.min_x - selection_margin}",
                        y: "{bounds.min_y - selection_margin}",
                        width: "{bounds.width + 2.0 * selection_margin}",
                        height: "{bounds.height + 2.0 * selection_margin}",
                        rx: "{6.0 / cam.scale}",
                        stroke_width: "{1.5 / cam.scale}",
                    }
                    for (name, corner_x, corner_y, anchor_x, anchor_y) in [
                        (
                            "tl",
                            bounds.min_x - selection_margin,
                            bounds.min_y - selection_margin,
                            bounds.min_x + bounds.width + selection_margin,
                            bounds.min_y + bounds.height + selection_margin,
                        ),
                        (
                            "tr",
                            bounds.min_x + bounds.width + selection_margin,
                            bounds.min_y - selection_margin,
                            bounds.min_x - selection_margin,
                            bounds.min_y + bounds.height + selection_margin,
                        ),
                        (
                            "bl",
                            bounds.min_x - selection_margin,
                            bounds.min_y + bounds.height + selection_margin,
                            bounds.min_x + bounds.width + selection_margin,
                            bounds.min_y - selection_margin,
                        ),
                        (
                            "br",
                            bounds.min_x + bounds.width + selection_margin,
                            bounds.min_y + bounds.height + selection_margin,
                            bounds.min_x - selection_margin,
                            bounds.min_y - selection_margin,
                        ),
                    ] {
                        rect {
                            key: "h{name}",
                            class: "lpme-handle",
                            x: "{corner_x - handle_half}",
                            y: "{corner_y - handle_half}",
                            width: "{2.0 * handle_half}",
                            height: "{2.0 * handle_half}",
                            stroke_width: "{1.4 / cam.scale}",
                            onpointerdown: move |evt| {
                                if secondary_button(&evt) {
                                    return;
                                }
                                evt.stop_propagation();
                                capture_pointer(&evt);
                                let doc_point = event_doc_point(&anchor, &camera, &evt);
                                session.write().begin_gesture();
                                drag.set(Some(CanvasDrag::Resize {
                                    anchor: [anchor_x, anchor_y],
                                    start: doc_point,
                                    moved: false,
                                }));
                            },
                        }
                    }
                }
                // The point a selected repeat turns about: a crosshair, not a
                // handle — it moves with the object (drag) and by its own
                // number fields, and a draggable dot here would collide with
                // the marquee.
                if let Some(center) = repeat_center {
                    {
                        let arm = 9.0 / cam.scale;
                        let stroke = 1.4 / cam.scale;
                        rsx! {
                            circle {
                                class: "lpme-repeat-center",
                                cx: "{center[0]}",
                                cy: "{center[1]}",
                                r: "{arm * 0.55}",
                                stroke_width: "{stroke}",
                            }
                            line {
                                class: "lpme-repeat-center",
                                x1: "{center[0] - arm}",
                                y1: "{center[1]}",
                                x2: "{center[0] + arm}",
                                y2: "{center[1]}",
                                stroke_width: "{stroke}",
                            }
                            line {
                                class: "lpme-repeat-center",
                                x1: "{center[0]}",
                                y1: "{center[1] - arm}",
                                x2: "{center[0]}",
                                y2: "{center[1] + arm}",
                                stroke_width: "{stroke}",
                            }
                        }
                    }
                }
                for (vertex_index, point) in vertex_points.iter().enumerate() {
                    {
                        let hot = selected_vertex == Some(vertex_index);
                        let half = 4.5 / cam.scale;
                        rsx! {
                            rect {
                                key: "v{vertex_index}",
                                class: if hot { "lpme-vertex lpme-vertex-hot" } else { "lpme-vertex" },
                                x: "{point[0] - half}",
                                y: "{point[1] - half}",
                                width: "{2.0 * half}",
                                height: "{2.0 * half}",
                                stroke_width: "{1.4 / cam.scale}",
                                onpointerdown: move |evt| {
                                    if secondary_button(&evt) {
                                        return;
                                    }
                                    evt.stop_propagation();
                                    capture_pointer(&evt);
                                    {
                                        let mut s = session.write();
                                        s.selection.vertex = Some(vertex_index);
                                        s.begin_gesture();
                                    }
                                    drag.set(Some(CanvasDrag::Vertex {
                                        index: vertex_index,
                                        moved: false,
                                    }));
                                },
                            }
                        }
                    }
                }
                if let (Some(from), Some(first)) = (chain_from, draft_points.first()) {
                    line {
                        class: "lpme-arrow-chain",
                        x1: "{from[0]}",
                        y1: "{from[1]}",
                        x2: "{first[0]}",
                        y2: "{first[1]}",
                        stroke_width: "{(radius * 0.14).clamp(0.4, 1.5)}",
                    }
                }
                if draft_points.len() >= 2 {
                    polyline {
                        class: "lpme-draft",
                        points: draft_points.iter().map(|p| format!("{},{}", p[0], p[1])).collect::<Vec<_>>().join(" "),
                        stroke_width: "{1.5 / cam.scale}",
                    }
                }
                for (index, ghost) in draft_ghosts.iter().enumerate() {
                    circle {
                        key: "g{index}",
                        cx: "{ghost[0]}",
                        cy: "{ghost[1]}",
                        r: "{radius}",
                        fill: SELECTION_COLOR,
                        opacity: "0.35",
                    }
                }
                for (index, point) in draft_points.iter().enumerate() {
                    circle {
                        key: "dp{index}",
                        cx: "{point[0]}",
                        cy: "{point[1]}",
                        r: "{3.5 / cam.scale}",
                        fill: SELECTION_COLOR,
                    }
                }
                if let Some((origin, size)) = marquee_rect {
                    rect {
                        class: "lpme-marquee",
                        x: "{origin[0]}",
                        y: "{origin[1]}",
                        width: "{size[0]}",
                        height: "{size[1]}",
                        stroke_width: "{1.2 / cam.scale}",
                    }
                }
            }
        }
    }
}

/// True for the secondary (right) button — those pointerdowns fall through
/// to the canvas pan instead of tool/selection actions.
fn secondary_button(evt: &Event<PointerData>) -> bool {
    evt.data().trigger_button() == Some(dioxus::html::input_data::MouseButton::Secondary)
}

/// Select `object_index` (respecting shift-toggle) and arm a move drag.
fn select_and_start_move(
    mut session: Signal<MapEditorSession>,
    mut drag: Signal<Option<CanvasDrag>>,
    anchor: Signal<CanvasAnchor>,
    camera: Signal<Camera>,
    object_index: usize,
    evt: &Event<PointerData>,
) {
    capture_pointer(evt);
    let doc_point = event_doc_point(&anchor, &camera, evt);
    let shift = evt.data().modifiers().shift();
    let mut s = session.write();
    if shift {
        s.selection.toggle(object_index);
        return;
    }
    // Scoped editing: while descended inside this object's group, a click
    // on the primary geometry keeps the descended selection (grabbing the
    // sub-object must not pop the scope) and just arms the write-through
    // move.
    let scoped_here = s
        .selection
        .single()
        .is_some_and(|path| !path.is_root() && path.object == object_index);
    let mut collapse = None;
    if !scoped_here {
        if !s.selection.object_selected(object_index) {
            s.selection.select_only(object_index);
        } else if s.selection.len() > 1 {
            collapse = Some(object_index);
        }
    }
    s.selection.vertex = None;
    s.begin_gesture();
    drop(s);
    drag.set(Some(CanvasDrag::Move {
        start: doc_point,
        moved: false,
        collapse,
    }));
}

/// Host-side fallback offset for pointer math when no mounted element is
/// available (tests, SSR): the standalone page's header height. In the
/// browser the mounted svg's live bounding rect anchors coordinates instead
/// — the editor can sit anywhere in a scrolling page (the fixture-face
/// embed), so a fixed offset cannot work there.
const HEADER_OFFSET: f32 = 49.0;

/// Handle to the mounted canvas svg. Pointer and wheel coordinates anchor to
/// its live bounding rect (queried per event — scroll-proof), and the host
/// viewport size is measured from the same rect.
#[derive(Clone, Default, PartialEq)]
pub struct CanvasAnchor {
    #[cfg(target_arch = "wasm32")]
    element: Option<web_sys::Element>,
}

impl CanvasAnchor {
    /// Anchor to a mounted element (the host's `onmounted` handler).
    #[cfg(target_arch = "wasm32")]
    pub fn from_element(element: web_sys::Element) -> Self {
        Self {
            element: Some(element),
        }
    }

    /// Top-left of the canvas in client coordinates.
    pub fn origin(&self) -> [f32; 2] {
        #[cfg(target_arch = "wasm32")]
        if let Some(element) = &self.element {
            let rect = element.get_bounding_client_rect();
            return [rect.left() as f32, rect.top() as f32];
        }
        [0.0, HEADER_OFFSET]
    }

    /// Measured canvas size in CSS pixels, when mounted.
    pub fn size(&self) -> Option<[f32; 2]> {
        #[cfg(target_arch = "wasm32")]
        if let Some(element) = &self.element {
            let rect = element.get_bounding_client_rect();
            let size = [rect.width() as f32, rect.height() as f32];
            if size[0] > 0.0 && size[1] > 0.0 {
                return Some(size);
            }
        }
        None
    }
}

/// Owns the canvas's ResizeObserver and its JS callback; disconnects on
/// drop. The svg's `onresize` attribute never fires in this stack, so this
/// is what keeps the measured viewport current across embed-box changes.
#[cfg(target_arch = "wasm32")]
struct CanvasResizeObserver {
    observer: web_sys::ResizeObserver,
    _callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::js_sys::Array)>,
}

#[cfg(target_arch = "wasm32")]
impl CanvasResizeObserver {
    fn install(element: &web_sys::Element, mut measure: impl FnMut() + 'static) -> Option<Self> {
        use wasm_bindgen::JsCast;
        let callback = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::js_sys::Array)>::new(
            move |_entries: web_sys::js_sys::Array| measure(),
        );
        let observer = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()).ok()?;
        observer.observe(element);
        Some(Self {
            observer,
            _callback: callback,
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for CanvasResizeObserver {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
}

/// Route the rest of this pointer stream to the pressed element even when
/// the cursor crosses overlays (rail, popover) or leaves the window — drags
/// must not die at the first overlay edge.
pub fn capture_pointer(evt: &Event<PointerData>) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(web_event) = evt.data().downcast::<web_sys::PointerEvent>()
            && let Some(target) = web_event.target()
            && let Ok(element) = target.dyn_into::<web_sys::Element>()
        {
            let _ = element.set_pointer_capture(web_event.pointer_id());
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = evt;
    }
}

fn event_view_point(anchor: &Signal<CanvasAnchor>, evt: &Event<PointerData>) -> [f32; 2] {
    let point = evt.data().client_coordinates();
    let origin = anchor.peek().origin();
    [point.x as f32 - origin[0], point.y as f32 - origin[1]]
}

fn event_doc_point(
    anchor: &Signal<CanvasAnchor>,
    camera: &Signal<Camera>,
    evt: &Event<PointerData>,
) -> [f32; 2] {
    let view = event_view_point(anchor, evt);
    camera.peek().view_to_doc(view)
}

/// Write (or clear) per-lamp live-color overrides as inline styles on the
/// mounted canvas's `[data-lamp]` circles. Direct DOM only — the whole
/// point is that a 60Hz color feed costs zero VDOM work. Inline style beats
/// the `fill` attribute per SVG presentation rules; clearing the style
/// restores the palette without this code knowing what the palette was.
fn apply_live_fills(canvas_dom_id: &str, live_on: bool, colors: &[[u8; 3]]) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Ok(lamps) = document.query_selector_all(&format!("#{canvas_dom_id} [data-lamp]"))
        else {
            return;
        };
        use wasm_bindgen::JsCast;
        for slot in 0..lamps.length() {
            let Some(element) = lamps
                .item(slot)
                .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
            else {
                continue;
            };
            let color = (live_on && !colors.is_empty())
                .then(|| {
                    element
                        .get_attribute("data-lamp")
                        .and_then(|index| index.parse::<usize>().ok())
                        .and_then(|index| colors.get(index))
                })
                .flatten();
            match color {
                Some([r, g, b]) => {
                    let _ = element.set_attribute("style", &format!("fill: rgb({r} {g} {b})"));
                }
                None => {
                    let _ = element.remove_attribute("style");
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (canvas_dom_id, live_on, colors);
    }
}

fn event_view_point_wheel(anchor: &Signal<CanvasAnchor>, evt: &Event<WheelData>) -> [f32; 2] {
    let point = evt.data().client_coordinates();
    let origin = anchor.peek().origin();
    [point.x as f32 - origin[0], point.y as f32 - origin[1]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_region_pads_the_short_axis_and_centers() {
        let frame = Bounds2d {
            min_x: 0.0,
            min_y: 0.0,
            width: 200.0,
            height: 50.0,
        };
        let region = fit_region(frame, 1.0);
        assert!((region.width - 200.0).abs() < 1e-3);
        assert!((region.height - 200.0).abs() < 1e-3);
        assert!((region.min_y - (-75.0)).abs() < 1e-3);
        let tall = Bounds2d {
            min_x: 10.0,
            min_y: 0.0,
            width: 50.0,
            height: 100.0,
        };
        let region = fit_region(tall, 2.0);
        assert!((region.width - 200.0).abs() < 1e-3);
        assert!((region.min_x - (10.0 - 75.0)).abs() < 1e-3);
    }

    #[test]
    fn lamp_radius_tracks_median_spacing() {
        let doc = lpc_mapping::corpus::panel_16x16();
        let resolved = resolve(&doc).unwrap();
        let radius = lamp_display_radius(&resolved);
        assert!((radius - 26.0 * 0.30).abs() < 0.5, "radius {radius}");
    }
}
