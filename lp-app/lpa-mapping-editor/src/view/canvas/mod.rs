//! The editor canvas: SVG rendering with camera pan/zoom, a document
//! placement seam, and the full editing grammar.
//!
//! Space model: the camera group maps **project space** to CSS pixels; the
//! nested placement group places **doc space** inside it (translate ∘
//! rotate ∘ uniform scale — identity when the canvas edits an unplaced
//! document). The dot grid renders in project space; every doc layer
//! renders inside the placement, and pointer math routes through the
//! placement inverse. Screen-constant sizing divides by the EFFECTIVE
//! scale (`camera.scale × placement.s`).
//!
//! Layers (bottom → top): dot-grid background, dimmed context fixtures,
//! the authored canvas rect, the fit-preview overlay, wiring arrows,
//! lamps, wiring numbers, the selection outline + corner resize handles,
//! path vertex handles, the path-draft preview, and the marquee.
//!
//! Every mutation flows through `MapEditorSession` ops: drags run
//! `begin_gesture` → `*_from_gesture` (totals, no drift) → `commit_gesture`
//! at pointer-up, so one drag is one undo step; `on_committed` fires after
//! every committed change so the host can persist.

pub(crate) mod canvas_anchor;
pub(crate) mod lamp_metrics;
pub(crate) mod layers;
pub(crate) mod live_fills;
pub(crate) mod palette;

use dioxus::prelude::*;
use lpc_mapping::{Bounds2d, Map2dShape, ResolvedMap2d, Rotation2d, bounds_of_points, resolve};

use crate::editor_core::camera::Camera;
use crate::editor_core::editor_session::{MapEditorSession, editable_path};
use crate::editor_core::map_tool::MapTool;
use crate::editor_core::placement::Placement;
use crate::editor_core::view_geometry::{ArrowInput, wiring_arrows};
use crate::view::map_editor::EditorViewOptions;

pub use canvas_anchor::{CanvasAnchor, capture_pointer};
pub use lamp_metrics::{fit_region, lamp_display_radius};
pub use palette::object_color;

use layers::doc::{DocLayersInput, doc_layers};
use layers::draft::{DraftLayerInput, draft_layer};
use layers::marquee::marquee_layer;
use layers::selection::{SelectionLayerInput, selection_layer};
use live_fills::apply_live_fills;

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

/// The Copy bundle event handlers close over: the canvas's signals plus
/// the active placement, so element-level handlers anywhere in the layer
/// tree share one pointer-math pipeline.
#[derive(Clone, Copy)]
pub(crate) struct CanvasInteract {
    pub(crate) session: Signal<MapEditorSession>,
    pub(crate) camera: Signal<Camera>,
    pub(crate) drag: Signal<Option<CanvasDrag>>,
    pub(crate) anchor: Signal<CanvasAnchor>,
    pub(crate) placement: Placement,
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
    /// Where the edited document sits in project space. Identity when the
    /// canvas edits an unplaced document — behavior is then exactly the
    /// pre-seam canvas. The session and document never see it: rendering
    /// nests inside the placement group and pointer math inverts it.
    #[props(default)]
    placement: Placement,
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
    let resize_observer = use_hook(|| {
        std::rc::Rc::new(std::cell::RefCell::new(
            None::<canvas_anchor::CanvasResizeObserver>,
        ))
    });
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

    let interact = CanvasInteract {
        session,
        camera,
        drag,
        anchor,
        placement,
    };
    let cam = camera();
    // Screen-constant sizing folds in the placement scale: a doc unit
    // reaches the screen through camera × placement.
    let eff = cam.scale * placement.scale_f32();
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
    // Lamp screen size is CAPPED: proportional at fit zoom, but circles stop
    // growing past ~11px screen radius as you zoom in — near-coincident
    // lamps (out-and-back wiring runs) separate visually instead of staying
    // stacked at every zoom. A floor keeps dots visible when zoomed far out
    // and keeps sparse layouts (big gaps → tiny proportional dots)
    // readable and clickable.
    let radius = {
        let base = lamp_display_radius(&resolved);
        (base * eff).clamp(5.0, 11.0) / eff
    };
    // Pointer targets never shrink below ~9px screen radius: sparse lamps
    // get an invisible hit ring so clicking doesn't demand pixel aim.
    let hit_radius = (9.0 / eff).max(radius);
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
    let show_numbers = opts.numbers && eff * radius >= 5.0;

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
    let handle_half = 5.0 / eff;
    let selection_margin = radius + 8.0 / eff;

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
                    anchor.set(CanvasAnchor::from_element(element.clone()));
                    *resize_observer.borrow_mut() =
                        canvas_anchor::CanvasResizeObserver::install(element, move || measure_cb.call(()));
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
                let doc_point = event_doc_point(&interact, &evt);
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
                let doc_point = event_doc_point(&interact, &evt);
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
                        // Threshold in SCREEN pixels: doc extent × effective
                        // scale (camera × placement).
                        let scale = camera().scale * placement.scale_f32();
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
                // Project space: the dot grid never rotates or scales with a
                // placed document.
                rect {
                    x: "-100000",
                    y: "-100000",
                    width: "200000",
                    height: "200000",
                    fill: "url(#lpme-dots)",
                }
                g {
                    transform: "{placement.svg_transform()}",
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
                    {doc_layers(&DocLayersInput {
                        interact,
                        opts,
                        reference: reference.as_ref(),
                        canvas_rect,
                        fit_rect,
                        arrows: arrows.as_ref(),
                        ghost_outlines: &ghost_outlines,
                        gap_segments: &gap_segments,
                        path_objects: &path_objects,
                        resolved: &resolved,
                        selection: &selection,
                        tessellating: &tessellating,
                        scoped_object,
                        span_instances: &span_instances,
                        tool_is_select,
                        eff,
                        radius,
                        hit_radius,
                        show_numbers,
                    })}
                    {selection_layer(&SelectionLayerInput {
                        interact,
                        eff,
                        selection_bounds,
                        selection_margin,
                        handle_half,
                        repeat_center,
                        vertex_points: &vertex_points,
                        selected_vertex,
                    })}
                    {draft_layer(&DraftLayerInput {
                        eff,
                        radius,
                        chain_from,
                        draft_points: &draft_points,
                        draft_ghosts: &draft_ghosts,
                    })}
                    {marquee_layer(eff, marquee_rect)}
                }
            }
        }
    }
}

/// True for the secondary (right) button — those pointerdowns fall through
/// to the canvas pan instead of tool/selection actions.
pub(crate) fn secondary_button(evt: &Event<PointerData>) -> bool {
    evt.data().trigger_button() == Some(dioxus::html::input_data::MouseButton::Secondary)
}

/// Select `object_index` (respecting shift-toggle) and arm a move drag.
pub(crate) fn select_and_start_move(
    interact: CanvasInteract,
    object_index: usize,
    evt: &Event<PointerData>,
) {
    let mut session = interact.session;
    let mut drag = interact.drag;
    capture_pointer(evt);
    let doc_point = event_doc_point(&interact, evt);
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

fn event_view_point(anchor: &Signal<CanvasAnchor>, evt: &Event<PointerData>) -> [f32; 2] {
    let point = evt.data().client_coordinates();
    let origin = anchor.peek().origin();
    [point.x as f32 - origin[0], point.y as f32 - origin[1]]
}

/// Event position in doc space: view → project through the camera, then
/// project → doc through the placement inverse.
pub(crate) fn event_doc_point(interact: &CanvasInteract, evt: &Event<PointerData>) -> [f32; 2] {
    let view = event_view_point(&interact.anchor, evt);
    interact
        .placement
        .inverse_f32(interact.camera.peek().view_to_doc(view))
}

fn event_view_point_wheel(anchor: &Signal<CanvasAnchor>, evt: &Event<WheelData>) -> [f32; 2] {
    let point = evt.data().client_coordinates();
    let origin = anchor.peek().origin();
    [point.x as f32 - origin[0], point.y as f32 - origin[1]]
}
