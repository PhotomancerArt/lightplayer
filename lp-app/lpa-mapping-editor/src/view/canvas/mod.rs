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
//! Layers (bottom → top): dot-grid background, fixture sprites (project
//! space; dived, the focused sprite's body yields to the live doc layers
//! and neighbours dim), the authored canvas rect, the fit-preview overlay,
//! wiring arrows, the shaped matrices' authored silhouettes, lamps, wiring
//! numbers, the selection outline + corner resize handles, vertex handles,
//! the drawing-tool draft preview (vertices, the polygon's implicit closing
//! edge and close target, and the ghost lamps the tool would commit), and
//! the marquee.
//!
//! Every mutation flows through `MapEditorSession` ops: drags run
//! `begin_gesture` → `*_from_gesture` (totals, no drift) → `commit_gesture`
//! at pointer-up, so one drag is one undo step; `on_committed` fires after
//! every committed change so the host can persist.

pub(crate) mod candidate_menu;
pub(crate) mod canvas_anchor;
pub(crate) mod lamp_metrics;
pub(crate) mod layers;
pub(crate) mod live_fills;
pub(crate) mod palette;

use dioxus::prelude::*;
use lpc_mapping::{Bounds2d, Map2dShape, ResolvedMap2d, Rotation2d, bounds_of_points, resolve};

use crate::editor_core::camera::Camera;
use crate::editor_core::editor_session::{
    DEFAULT_PITCH, MapEditorSession, editable_path, editable_vertices, polygon_draft_shape,
};
use crate::editor_core::map_tool::MapTool;
use crate::editor_core::placement::Placement;
use crate::editor_core::view_geometry::{ArrowInput, wiring_arrows};
use crate::view::view_options::EditorViewOptions;

pub use canvas_anchor::{CanvasAnchor, capture_pointer};
pub use lamp_metrics::{authored_spans, fit_region, lamp_display_radius};
pub use palette::object_color;

pub use layers::bodies::CellSeeding;
pub use layers::cells::{LampCell, lamp_cells, point_cells};
pub use layers::fixtures::{FixtureBody, FixtureEvent, FixturePick, FixtureSprite, SpriteObject};
pub use layers::outline::{aligned_outline, dist_to_loops, hit_body, point_in_loops};

use candidate_menu::CandidateMenu;
use layers::bodies::object_bodies;
use layers::doc::{DocLayersInput, doc_layers};
use layers::draft::{DraftLayerInput, draft_layer};
use layers::fixtures::{
    FixtureLayerInput, SELECT_SLOP_PX, dragged_placement, exceeds_drag_threshold, fixture_layer,
    hit_fixture, nearest_lamp, resolve_object_pick, within_cycle_radius,
};
use layers::marquee::marquee_layer;
use layers::selection::{SelectionLayerInput, selection_layer};
use live_fills::apply_live_fills;

/// An in-flight canvas drag.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasDrag {
    Pan {
        last: [f32; 2],
    },
    /// Maybe-drag on a fixture sprite (fixture grammar): becomes a real
    /// move past the CSS-pixel threshold; under it, pointer-up selects.
    FixturePress {
        key: String,
        /// The TRUE lamp index the press landed nearest, when the sprite
        /// draws lamps — what makes a tap name an OBJECT (Q10) rather than
        /// the whole fixture. Resolved at PRESS time, from the point the
        /// pointer actually went down on.
        lamp: Option<u32>,
        /// The object the press RESOLVED to ([`resolve_object_pick`]) —
        /// the round-3 "objects are THINGS" hit target, now decided out of
        /// the whole candidate stack (slop ring included) and cycled when
        /// the press repeats the last one. Resolved at PRESS time like the
        /// lamp, so what the pointer went down on is what gets selected.
        object: Option<usize>,
        start_client: [f64; 2],
        original: Placement,
        moved: bool,
    },
    /// Background press under the fixture grammar: pointer-up without
    /// movement deselects.
    FixtureTap {
        start_client: [f64; 2],
        moved: bool,
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

/// Where the last resolved fixture pick was pressed. The click-CYCLE needs
/// exactly this much memory: a second press near the same spot, on the same
/// sprite, advances through the stack instead of re-deciding.
///
/// Canvas-local on purpose — which of several overlapping objects a click
/// means is a VIEW policy, and the controller has no business holding it.
#[derive(Clone, Debug, PartialEq)]
struct LastPress {
    key: String,
    client: [f64; 2],
}

/// A right-button press in flight. Right-DRAG is the canvas pan, so the
/// context menu may only open for a right press that never moved.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RightPress {
    start_client: [f64; 2],
    moved: bool,
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
    /// Live lamp colors indexed by wiring index (host feed). A color frame
    /// must NOT re-render this component: the VDOM owns each lamp's
    /// palette `fill` attribute, and a post-render effect overrides via
    /// inline `style` — per-frame colors are direct DOM writes, never a
    /// 1500-node diff.
    live_feed: Signal<Vec<[u8; 3]>>,
    /// Per-SPRITE live lamp colors, keyed by sprite key and indexed by
    /// TRUE lamp index (the fixture layer display-subsamples; its
    /// `data-sprite-lamp` attributes carry the stride-corrected index).
    /// Same direct-DOM contract as `live_feed`; `None` = no sprite feed
    /// (the Mapping view), an absent/empty entry = that sprite's palette.
    #[props(default)]
    sprite_live_feed: Option<Signal<std::collections::BTreeMap<String, Vec<[u8; 3]>>>>,
    /// Fired after any committed (undoable) change.
    on_committed: EventHandler<()>,
    /// Where the edited document sits in project space. Identity when the
    /// canvas edits an unplaced document — behavior is then exactly the
    /// pre-seam canvas. The session and document never see it: rendering
    /// nests inside the placement group and pointer math inverts it.
    #[props(default)]
    placement: Placement,
    /// The project's fixtures, rendered as sprites in project space above
    /// the grid and below the doc layers. Empty = no fixture layer.
    #[props(default)]
    fixtures: Vec<FixtureSprite>,
    /// The dived fixture's sprite key: its body is hidden (the live doc
    /// layers replace it at `placement`), neighbours dim, and the fixture
    /// grammar goes inert except neighbour double-click (dive-switch).
    #[props(default)]
    focused: Option<String>,
    /// Fixture gesture events (select / move / dive). Present *and* not
    /// dived ⇒ svg-level pointerdown runs ONLY the fixture grammar — the
    /// editor grammar needs a dived session.
    #[props(default)]
    on_fixture: Option<EventHandler<FixtureEvent>>,
    /// Host-owned reference image, rendered doc-space at the origin between
    /// the dot grid and the authored canvas rect when `view_opts.reference`.
    #[props(default)]
    reference: Option<crate::view::reference::ReferenceImage>,
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
    // The sprite feed's writes, same direct-DOM contract. A sprite node
    // rebuilt between feed ticks briefly shows its palette until the next
    // tick re-applies — self-healing at snapshot cadence.
    {
        let canvas_dom_id = canvas_dom_id.clone();
        use_effect(move || {
            if let Some(feed) = sprite_live_feed {
                live_fills::apply_sprite_live_fills(&canvas_dom_id, &feed());
            }
        });
    }

    // Overlap-selection state (P5). All of it is ephemeral view policy:
    // where the last pick was pressed (the cycle's whole memory), the open
    // candidate menu, the hold timer's ticket, and the right-press travel
    // that tells a context menu from a pan.
    let mut last_press = use_signal(|| None::<LastPress>);
    let mut menu = use_signal(|| None::<CandidateMenu>);
    // Read reactively exactly once: the menu is the only one of these the
    // render depends on. The rest are peeked from handlers, so writing them
    // never costs a re-render.
    let menu_open = menu();
    let hold_gen = use_signal(|| 0_u64);
    let mut right_press = use_signal(|| None::<RightPress>);
    let menu_close = use_callback(move |()| menu.set(None));
    // Escape closes the menu, heard at the document rather than by focusing
    // the overlay (see `candidate_menu::MenuEscListener`). Same
    // install-on-open / drop-on-close shape as the ResizeObserver above,
    // and the same reason for routing through a `Callback`: a signal write
    // from a raw JS callback needs the Dioxus runtime.
    #[cfg(target_arch = "wasm32")]
    let esc_listener = use_hook(|| {
        std::rc::Rc::new(std::cell::RefCell::new(
            None::<candidate_menu::MenuEscListener>,
        ))
    });
    use_effect(move || {
        let open = menu().is_some();
        #[cfg(target_arch = "wasm32")]
        {
            let mut slot = esc_listener.borrow_mut();
            if !open {
                *slot = None;
            } else if slot.is_none() {
                *slot = candidate_menu::MenuEscListener::install(move || menu_close.call(()));
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (open, menu_close);
    });

    let interact = CanvasInteract {
        session,
        camera,
        drag,
        anchor,
        placement,
    };
    // The activity discriminator: the fixture grammar owns svg-level
    // presses when the shell listens for fixture events and nothing is
    // dived (there is no session to edit). Dived, the editor grammar runs
    // and fixtures stay inert to single clicks (neighbour double-click is
    // the one exception — the dive-switch).
    let fixture_mode = on_fixture.is_some() && focused.is_none();
    let fixtures_down = fixtures.clone();
    let fixtures_dbl = fixtures.clone();
    let fixtures_menu = fixtures.clone();
    let focused_dbl = focused.clone();
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
    // Authored vertex chains, repeats included: a repeated shape's editable
    // geometry is instance 0 — the unrotated inner shape — so that is what
    // takes clicks and shows handles. Every vertexed shape answers here
    // (path, polygon, shaped matrix), and a CLOSED chain is drawn closed, so
    // the hit line covers the seam edge a polygon actually has.
    let hit_outlines: Vec<(usize, Vec<[f32; 2]>)> = doc
        .objects
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            let points = editable_vertices(&object.shape)?;
            Some((
                index,
                chain_points(points, vertices_are_closed(&object.shape)),
            ))
        })
        .collect();
    // A shaped matrix's lamps are a FIELD, and its body draws no band at all
    // (bodies.rs), so nothing else on the canvas draws the outline they fill:
    // the authored silhouette gets its own thin line. Authored grain, like the
    // wiring annotations: instance 0 of a repeat, never all of them.
    let filled_outlines: Vec<(usize, Vec<[f32; 2]>)> = doc
        .objects
        .iter()
        .enumerate()
        .filter(|(_, object)| is_filled_polygon(&object.shape))
        .filter_map(|(index, object)| {
            let points = editable_vertices(&object.shape)?;
            Some((index, chain_points(points, true)))
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
                let points = chain_points(
                    editable_vertices(&repeat.shape)?,
                    vertices_are_closed(shape),
                );
                Some(
                    (1..repeat.count)
                        .map(|instance| {
                            let rotation =
                                Rotation2d::about(repeat.center, repeat.instance_degrees(instance));
                            points.iter().map(|p| rotation.apply(*p)).collect()
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
    // The object BODIES: the aligned band + lamp cells the Arrange view
    // draws, derived HERE from the live document, so editing `align` in the
    // properties panel repaints the picture on the next render. One pass
    // over the resolved lamps per render — the doc layer never recomputes
    // any of it inside an element.
    let bodies = object_bodies(doc, &resolved);
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
    // Wiring annotations follow the Mapping view's AUTHORED grain: arrows
    // and numbers cover each object's authored strand only, so a repeat's
    // expanded instances keep their lamp dots without per-instance chrome
    // (and a dome-scale document doesn't pay for N sets of arrows).
    let annotation_spans = authored_spans(&resolved);
    let positions = resolved.positions();
    let arrows = opts.arrows.then(|| {
        wiring_arrows(&ArrowInput {
            positions: &positions,
            spans: &annotation_spans,
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

    // Vertex handles for a single selected shape — every shape with authored
    // vertices (path, polygon, shaped matrix), taken through a repeat at
    // instance 0, whose other instances follow the drag on the next resolve.
    let vertex_points: Vec<[f32; 2]> = selection
        .single()
        .and_then(|path| path.resolve(doc))
        .filter(|_| tool_is_select)
        .and_then(editable_vertices)
        .map(<[[f32; 2]]>::to_vec)
        .unwrap_or_default();
    let selected_vertex = selection.vertex;

    // Draft preview: the placed vertices, the ghost lamps the tool would
    // commit, and the chain link from the previous object's last lamp.
    let draft_points: Vec<[f32; 2]> = match &tool {
        MapTool::Path { draft } | MapTool::Polygon { draft, .. } => draft.clone(),
        _ => Vec::new(),
    };
    let draft_ghosts: Vec<[f32; 2]> = match &tool {
        // The path tool's own count heuristic, previewed through the
        // resolver.
        MapTool::Path { draft } if draft.len() >= 2 => {
            let length: f32 = draft
                .windows(2)
                .map(|pair| {
                    ((pair[1][0] - pair[0][0]).powi(2) + (pair[1][1] - pair[0][1]).powi(2)).sqrt()
                })
                .sum();
            let count = ((length / DEFAULT_PITCH).round() as u32).max(2);
            ghost_positions(Map2dShape::Path(lpc_mapping::PathShape {
                points: draft.clone(),
                count,
                reversed: false,
                gaps: Vec::new(),
                align: lpc_mapping::PathAlign::On,
            }))
        }
        // The polygon tool previews the SHAPE ITSELF: `polygon_draft_shape`
        // is what `polygon_finish` commits, resolved by the real resolver —
        // so the lattice on screen is the lattice that lands, cell for cell.
        MapTool::Polygon { draft, mode } => polygon_draft_shape(draft, *mode)
            .map(ghost_positions)
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    // The close target: a polygon draft's first vertex, once there are
    // enough points for the next click on it to close the outline.
    let draft_close_target = match &tool {
        MapTool::Polygon { draft, .. } => close_target(draft).copied(),
        _ => None,
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

    // The candidate menu, as a SIBLING of the svg inside
    // `.lpme-canvas-wrap`: it is HTML (rows of names), the two elements
    // share a box, and keeping it out of the svg means nothing about the
    // canvas's DOM — or its story captures — changes when it is closed.
    // Gated on fixture mode, so a dived canvas can never grow one.
    let menu_overlay: Element = match (fixture_mode, menu_open, on_fixture) {
        (true, Some(open), Some(handler)) => candidate_menu::menu_overlay(open, menu, handler),
        _ => rsx! {},
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
                // Any new press supersedes a pending hold timer and puts
                // an open menu away. This press's own ticket is what the
                // hold timer below is armed with.
                let generation = bump(hold_gen);
                if menu.peek().is_some() {
                    menu.set(None);
                }
                if secondary_button(&evt) {
                    capture_pointer(&evt);
                    let point = evt.data().client_coordinates();
                    right_press.set(Some(RightPress {
                        start_client: [point.x, point.y],
                        moved: false,
                    }));
                    drag.set(Some(CanvasDrag::Pan {
                        last: event_view_point(&anchor, &evt),
                    }));
                    return;
                }
                // Not the right button, so any remembered right-press
                // travel is stale: a SYNTHESIZED `contextmenu` (the mobile
                // long-press, the menu key) must never inherit an old
                // pan's "this was a drag" verdict.
                if right_press.peek().is_some() {
                    right_press.set(None);
                }
                capture_pointer(&evt);
                if fixture_mode {
                    // Fixture grammar, one canvas-level hit test: press on
                    // a sprite arms a maybe-move, background press arms a
                    // deselect tap. No editor grammar runs — no session.
                    let point = evt.data().client_coordinates();
                    let client = [point.x, point.y];
                    let view = event_view_point(&anchor, &evt);
                    let project = camera.peek().view_to_doc(view);
                    let project_point = [f64::from(project[0]), f64::from(project[1])];
                    match hit_fixture(&fixtures_down, project_point) {
                        Some(sprite) => {
                            // Cycling is a claim about AIM, so it is judged
                            // in client pixels against the previous pick —
                            // and only on the same sprite.
                            let repeat_press = last_press.peek().as_ref().is_some_and(|last| {
                                last.key == sprite.key && within_cycle_radius(last.client, client)
                            });
                            let pick = resolve_object_pick(
                                sprite,
                                project_point,
                                own_space_slop(camera.peek().scale, sprite.placement.s),
                                repeat_press,
                            );
                            let lamp = nearest_lamp(sprite, project_point);
                            // Hold-to-menu, but only where the point is
                            // genuinely AMBIGUOUS: with one candidate the
                            // menu would say nothing the press has not
                            // already decided, and arming it would only
                            // steal a slow, deliberate drag.
                            if pick.candidates.len() >= 2 {
                                candidate_menu::arm_hold_menu(
                                    generation,
                                    hold_gen,
                                    drag,
                                    menu,
                                    candidate_menu::build(sprite, &pick.candidates, view, lamp),
                                );
                            }
                            drag.set(Some(CanvasDrag::FixturePress {
                                key: sprite.key.clone(),
                                lamp,
                                object: pick.index,
                                start_client: client,
                                original: sprite.placement,
                                moved: false,
                            }));
                        }
                        None => drag.set(Some(CanvasDrag::FixtureTap {
                            start_client: client,
                            moved: false,
                        })),
                    }
                    return;
                }
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
                    MapTool::Polygon { ref draft, .. } => {
                        // Close-on-first is the polygon's primary finish:
                        // a click inside the first vertex's hit ring closes
                        // the outline instead of adding a point. The ring is
                        // SCREEN-sized (the vertex-handle convention), so the
                        // target neither shrinks with zoom nor swallows
                        // deliberate clicks when zoomed in.
                        if closes_polygon_draft(draft, doc_point, eff) {
                            // A refused finish (fewer than three DISTINCT
                            // vertices survive the draft merge) is an IGNORED
                            // gesture — the draft and the tool both stay, so
                            // the author keeps their clicks.
                            if session.write().polygon_finish().is_some() {
                                on_committed.call(());
                            }
                        } else {
                            session.write().polygon_add_point(doc_point);
                        }
                    }
                }
            },
            onpointermove: move |evt| {
                let Some(current_drag) = drag() else {
                    return;
                };
                let doc_point = event_doc_point(&interact, &evt);
                match current_drag {
                    CanvasDrag::FixturePress {
                        key,
                        lamp,
                        object,
                        start_client,
                        original,
                        moved,
                    } => {
                        let point = evt.data().client_coordinates();
                        let client = [point.x, point.y];
                        let now_moved = moved || exceeds_drag_threshold(start_client, client);
                        if now_moved && !moved {
                            // The press became a drag: the hold's ticket
                            // goes stale, so a menu can never open over a
                            // fixture move.
                            bump(hold_gen);
                        }
                        if now_moved && let Some(handler) = &on_fixture {
                            handler.call(FixtureEvent::Move {
                                key: key.clone(),
                                placement: dragged_placement(
                                    original,
                                    start_client,
                                    client,
                                    camera.peek().scale,
                                ),
                                commit: false,
                            });
                        }
                        drag.set(Some(CanvasDrag::FixturePress {
                            key,
                            lamp,
                            object,
                            start_client,
                            original,
                            moved: now_moved,
                        }));
                    }
                    CanvasDrag::FixtureTap {
                        start_client,
                        moved,
                    } => {
                        let point = evt.data().client_coordinates();
                        if !moved && exceeds_drag_threshold(start_client, [point.x, point.y]) {
                            drag.set(Some(CanvasDrag::FixtureTap {
                                start_client,
                                moved: true,
                            }));
                        }
                    }
                    CanvasDrag::Pan { last } => {
                        // A right-drag past the threshold is a PAN, and a
                        // pan is never a context menu — remember that for
                        // the `oncontextmenu` that may still be coming.
                        let point = evt.data().client_coordinates();
                        let tracked = *right_press.peek();
                        if let Some(press) = tracked
                            && !press.moved
                            && exceeds_drag_threshold(press.start_client, [point.x, point.y])
                        {
                            right_press.set(Some(RightPress { moved: true, ..press }));
                        }
                        // Any drag puts the menu away (the scrim normally
                        // eats the press first; a captured pointer can
                        // still reach here).
                        if menu.peek().is_some() {
                            menu.set(None);
                        }
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
            onpointerup: move |evt| {
                // The release cancels any pending hold — including the one
                // whose press was already consumed by a fired menu, whose
                // drag is `None` and which therefore returns below.
                bump(hold_gen);
                let Some(current_drag) = drag() else {
                    return;
                };
                drag.set(None);
                match current_drag {
                    CanvasDrag::Pan { .. } => {}
                    CanvasDrag::FixturePress {
                        key,
                        lamp,
                        object,
                        start_client,
                        original,
                        moved,
                    } => {
                        if let Some(handler) = &on_fixture {
                            if moved {
                                // One gesture = one committed move. The
                                // shell holds the override until the write
                                // echoes — no snap-back.
                                let point = evt.data().client_coordinates();
                                handler.call(FixtureEvent::Move {
                                    key,
                                    placement: dragged_placement(
                                        original,
                                        start_client,
                                        [point.x, point.y],
                                        camera.peek().scale,
                                    ),
                                    commit: true,
                                });
                            } else {
                                // The pick landed: remember WHERE, so the
                                // next press on the same spot cycles.
                                last_press.set(Some(LastPress {
                                    key: key.clone(),
                                    client: start_client,
                                }));
                                handler.call(FixtureEvent::Select(Some(FixturePick {
                                    key,
                                    lamp,
                                    object,
                                })));
                            }
                        }
                    }
                    // A background tap that never moved deselects — and
                    // ends the cycle: the stack it was walking is gone.
                    CanvasDrag::FixtureTap { moved, .. } => {
                        if !moved && let Some(handler) = &on_fixture {
                            last_press.set(None);
                            handler.call(FixtureEvent::Select(None));
                        }
                    }
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
                // The pointer left the canvas: whatever the hold was
                // about, it is no longer being held here.
                bump(hold_gen);
                if matches!(
                    drag(),
                    Some(CanvasDrag::Pan { .. }) | Some(CanvasDrag::Marquee { .. })
                ) {
                    drag.set(None);
                }
            },
            onpointercancel: move |_| {
                bump(hold_gen);
                // A cancelled fixture drag resets the shell's override to
                // the press-time placement (nothing was committed).
                if let Some(CanvasDrag::FixturePress {
                    key,
                    original,
                    moved: true,
                    ..
                }) = drag.peek().clone()
                    && let Some(handler) = &on_fixture
                {
                    handler.call(FixtureEvent::Move {
                        key,
                        placement: original,
                        commit: false,
                    });
                }
                drag.set(None);
            },
            oncontextmenu: move |evt| {
                // The browser menu never belongs on the canvas — right-drag
                // is the pan, and this is the one keystroke-free way to
                // reach the candidate list on a mouse.
                evt.prevent_default();
                if !fixture_mode {
                    return;
                }
                // A mobile long-press often SYNTHESIZES a contextmenu on
                // top of the hold that already opened the menu; a menu
                // that is up means this event has nothing left to do.
                if menu.peek().is_some() {
                    return;
                }
                // A right press that travelled was a pan, not a request.
                if right_press.peek().is_some_and(|press| press.moved) {
                    return;
                }
                let point = evt.data().client_coordinates();
                let view = event_view_point_mouse(&anchor, &evt);
                let project = camera.peek().view_to_doc(view);
                let project_point = [f64::from(project[0]), f64::from(project[1])];
                let Some(sprite) = hit_fixture(&fixtures_menu, project_point) else {
                    return;
                };
                let pick = resolve_object_pick(
                    sprite,
                    project_point,
                    own_space_slop(camera.peek().scale, sprite.placement.s),
                    false,
                );
                let lamp = nearest_lamp(sprite, project_point);
                match pick.candidates.len() {
                    // Nothing to disambiguate: leave the background alone.
                    0 => {}
                    // One candidate is not a choice — a menu with a single
                    // row is noise, so name it outright.
                    1 => {
                        if let Some(handler) = &on_fixture {
                            last_press.set(Some(LastPress {
                                key: sprite.key.clone(),
                                client: [point.x, point.y],
                            }));
                            handler.call(FixtureEvent::Select(Some(FixturePick {
                                key: sprite.key.clone(),
                                lamp,
                                object: pick.index,
                            })));
                        }
                    }
                    _ => menu.set(Some(candidate_menu::build(
                        sprite,
                        &pick.candidates,
                        view,
                        lamp,
                    ))),
                }
            },
            ondoubleclick: move |evt| {
                // Double-click dispatch, in order (editor grammar only — never
                // in fixture mode, where there is no tool and no session):
                //
                //   1. a live DRAFT finishes. The polygon's own gesture is
                //      close-on-first, but the double-click must still be
                //      CONSUMED: mid-draft it can never fall through.
                //   2. Select tool, one vertexed object selected, the click on
                //      one of its EDGES → insert a corner there.
                //   3. otherwise the fixture dive / dive-switch below.
                //
                // Lamp dots handle their own double-click (descend into a
                // group) and stop propagation first, so rung 2 only ever sees
                // clicks that missed every lamp.
                if !fixture_mode {
                    let tool = session.read().tool.clone();
                    match tool {
                        MapTool::Path { .. } => {
                            if session.write().path_finish().is_some() {
                                on_committed.call(());
                            }
                            return;
                        }
                        MapTool::Polygon { .. } => {
                            if session.write().polygon_finish().is_some() {
                                on_committed.call(());
                            }
                            return;
                        }
                        MapTool::Select => {
                            if insert_vertex_on_edge(&interact, &evt, eff) {
                                on_committed.call(());
                                return;
                            }
                        }
                        _ => {}
                    }
                }
                // Fixture dive / dive-switch: a sprite double-click dives
                // when not dived; a NEIGHBOUR while dived switches the
                // dive (D2). Lamp handlers stop propagation first, so the
                // focused document's own double-click grammar (descend)
                // always wins over this.
                let Some(handler) = &on_fixture else { return };
                let point = evt.data().client_coordinates();
                let origin = anchor.peek().origin();
                let view = [point.x as f32 - origin[0], point.y as f32 - origin[1]];
                let project = camera.peek().view_to_doc(view);
                if let Some(sprite) =
                    hit_fixture(&fixtures_dbl, [f64::from(project[0]), f64::from(project[1])])
                    && focused_dbl.as_deref() != Some(sprite.key.as_str())
                {
                    handler.call(FixtureEvent::Dive(sprite.key.clone()));
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
                    // Tile spacing is project space (scales with the camera
                    // by design), but userSpaceOnUse pattern *content*
                    // inherits that same scale — left alone, the dots would
                    // grow with zoom right along with the spacing. Counter
                    // the camera scale so the rendered dot stays
                    // screen-constant, same idiom as the fixture layer.
                    circle {
                        cx: "1",
                        cy: "1",
                        r: "{1.0 / cam.scale}",
                        fill: "rgba(255, 255, 255, 0.06)",
                    }
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
                // Fixture sprites: project space, above the grid, below the
                // placed doc layers. Dived, the focused sprite's body yields
                // to the live doc layers and neighbours dim.
                {fixture_layer(&FixtureLayerInput {
                    sprites: &fixtures,
                    focused: focused.as_deref(),
                    cam_scale: cam.scale,
                })}
                // Doc layers render only with a session to show: dived, or
                // hosted without the fixture grammar (the plain editor).
                if !fixture_mode {
                g {
                    transform: "{placement.svg_transform()}",
                    {doc_layers(&DocLayersInput {
                        interact,
                        opts,
                        reference: reference.as_ref(),
                        canvas_rect,
                        fit_rect,
                        arrows: arrows.as_ref(),
                        ghost_outlines: &ghost_outlines,
                        gap_segments: &gap_segments,
                        hit_outlines: &hit_outlines,
                        filled_outlines: &filled_outlines,
                        bodies: &bodies,
                        resolved: &resolved,
                        annotation_spans: &annotation_spans,
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
                        draft_close_target,
                    })}
                    {marquee_layer(eff, marquee_rect)}
                }
                }
            }
        }
        {menu_overlay}
    }
}

/// Vertices a polygon draft needs before its first point becomes a close
/// target: below three there is no outline, and `polygon_finish` would
/// refuse anyway.
const CLOSE_MIN_VERTICES: usize = 3;

/// Screen-pixel stroke width of the invisible hit line laid over an authored
/// chain — what makes skinny geometry clickable, and what a double-click must
/// land inside to mean "this edge".
pub(crate) const HIT_LINE_PX: f32 = 14.0;

/// Where on a vertex chain a click landed: which segment, and the point on it
/// nearest the click.
#[derive(Clone, Copy, Debug, PartialEq)]
struct EdgeHit {
    /// Index of the segment, counting the implicit closing seam last.
    segment: usize,
    /// The foot of the click on that segment — the new corner lands ON the
    /// edge, so inserting one never kinks the outline by the aim error.
    at: [f32; 2],
}

impl EdgeHit {
    /// Where the new vertex sits in the points list. A segment splits by
    /// taking the seat AFTER its start vertex — which for the closing seam
    /// (last → first) is the END of the list, never index 0.
    fn insert_index(self) -> usize {
        self.segment + 1
    }
}

/// The edge of `points` a click at `point` landed on, judged in SCREEN pixels
/// against the same [`HIT_LINE_PX`] band the hit line already offers the
/// pointer, so aim never depends on the camera.
///
/// `closed` adds the implicit last → first seam as the final segment. A hit
/// whose foot is within a vertex handle's grab ring of either end is refused:
/// that click is about the corner that is already there, and splitting a
/// segment right beside its own endpoint only makes a degenerate one.
fn hit_edge(points: &[[f32; 2]], closed: bool, point: [f32; 2], eff: f32) -> Option<EdgeHit> {
    let scale = eff.max(1e-6);
    let reach = HIT_LINE_PX / 2.0 / scale;
    let seam = layers::selection::VERTEX_HIT_PX / scale;
    let last = points.len().checked_sub(1)?;
    let segments = if closed && points.len() >= CLOSE_MIN_VERTICES {
        last + 1
    } else {
        last
    };
    let mut best: Option<(f32, EdgeHit)> = None;
    for segment in 0..segments {
        let a = points[segment];
        let b = points[(segment + 1) % points.len()];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len_sq = dx * dx + dy * dy;
        if len_sq <= f32::EPSILON {
            continue;
        }
        let t = (((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / len_sq).clamp(0.0, 1.0);
        let foot = [a[0] + t * dx, a[1] + t * dy];
        let distance = ((point[0] - foot[0]).powi(2) + (point[1] - foot[1]).powi(2)).sqrt();
        if distance > reach {
            continue;
        }
        // Too near either corner: this is a vertex gesture, not an edge one.
        let span = len_sq.sqrt();
        if t * span < seam || (1.0 - t) * span < seam {
            continue;
        }
        if best.is_none_or(|(closest, _)| distance < closest) {
            best = Some((distance, EdgeHit { segment, at: foot }));
        }
    }
    best.map(|(_, hit)| hit)
}

/// The vertex a click would close `draft` on, `None` while the draft is too
/// short to be an outline.
fn close_target(draft: &[[f32; 2]]) -> Option<&[f32; 2]> {
    if draft.len() < CLOSE_MIN_VERTICES {
        return None;
    }
    draft.first()
}

/// Whether a click at `point` (doc space) closes `draft` on its first
/// vertex.
///
/// Judged against [`layers::selection::VERTEX_HIT_PX`] — the same
/// screen-pixel ring a finished shape's vertex handles take grabs in — so
/// the close target is a constant size at every zoom, and the gesture that
/// closes an outline is the gesture that will later grab its corner.
fn closes_polygon_draft(draft: &[[f32; 2]], point: [f32; 2], eff: f32) -> bool {
    let Some(first) = close_target(draft) else {
        return false;
    };
    let radius = layers::selection::VERTEX_HIT_PX / eff.max(1e-6);
    let (dx, dy) = (point[0] - first[0], point[1] - first[1]);
    dx * dx + dy * dy <= radius * radius
}

/// Rung 2 of the double-click dispatch: add a corner where the click landed
/// on an edge of the single selected outline.
///
/// Refuses (and lets the click fall through) unless exactly one object is
/// selected, it has authored vertices, and the click landed on one of its
/// edges away from the corners already there. The new corner becomes the
/// vertex selection, so it shows hot and ⌫ takes it straight back.
fn insert_vertex_on_edge(interact: &CanvasInteract, evt: &Event<MouseData>, eff: f32) -> bool {
    let view = event_view_point_mouse(&interact.anchor, evt);
    let point = interact
        .placement
        .inverse_f32(interact.camera.peek().view_to_doc(view));
    let mut session = interact.session;
    let planned = {
        let read = session.peek();
        read.selection.single().cloned().and_then(|path| {
            let shape = path.resolve(read.doc())?;
            let hit = hit_edge(
                editable_vertices(shape)?,
                vertices_are_closed(shape),
                point,
                eff,
            )?;
            Some((path, hit))
        })
    };
    let Some((path, hit)) = planned else {
        return false;
    };
    let at = hit.insert_index();
    let mut write = session.write();
    if !write.insert_vertex_at(&path, at, hit.at) {
        return false;
    }
    write.selection.vertex = Some(at);
    true
}

/// True when a shape's authored vertices form a CLOSED loop — a polygon or
/// a shaped matrix, at any depth under a repeat.
fn vertices_are_closed(shape: &Map2dShape) -> bool {
    match shape {
        Map2dShape::Polygon(_) | Map2dShape::FilledPolygon(_) => true,
        Map2dShape::Repeat(repeat) => vertices_are_closed(&repeat.shape),
        _ => false,
    }
}

/// True for a shaped matrix, at any depth under a repeat.
fn is_filled_polygon(shape: &Map2dShape) -> bool {
    match shape {
        Map2dShape::FilledPolygon(_) => true,
        Map2dShape::Repeat(repeat) => is_filled_polygon(&repeat.shape),
        _ => false,
    }
}

/// A vertex chain as it is DRAWN: closed chains repeat their first vertex,
/// so the loop has no phantom mouth at the seam.
fn chain_points(points: &[[f32; 2]], closed: bool) -> Vec<[f32; 2]> {
    let mut drawn = points.to_vec();
    if closed && points.len() >= CLOSE_MIN_VERTICES {
        drawn.extend(points.first().copied());
    }
    drawn
}

/// Ghost-lamp positions for a draft shape, through the REAL resolver: a
/// one-object document resolved exactly as the finished object will be, so
/// the preview can never drift from what committing creates.
fn ghost_positions(shape: Map2dShape) -> Vec<[f32; 2]> {
    let ghost_doc = lpc_mapping::Map2dDoc {
        objects: vec![lpc_mapping::Map2dObject {
            name: String::new(),
            id: None,
            stride: None,
            shape,
        }],
        ..lpc_mapping::Map2dDoc::new()
    };
    resolve(&ghost_doc)
        .map(|resolved| resolved.positions())
        .unwrap_or_default()
}

/// Advance a generation counter and return the new value.
///
/// The hold timer's whole cancellation story: every path that ends a press
/// (a move past the drag threshold, pointer-up, pointer-leave, cancel, a
/// new press) bumps the counter, and a timer that fires holding a stale
/// ticket does nothing. Peeked, never read reactively — bumping it on
/// every gesture must not cost a render.
fn bump(mut counter: Signal<u64>) -> u64 {
    let next = *counter.peek() + 1;
    counter.set(next);
    next
}

/// The ruled ~10 px selection slop, converted into one sprite's OWN space:
/// a doc unit reaches the screen through the camera scale and the sprite's
/// placement scale, so dividing by both keeps the ring a constant number of
/// pixels at every zoom and on every sprite.
fn own_space_slop(cam_scale: f32, placement_scale: f64) -> f64 {
    SELECT_SLOP_PX / (f64::from(cam_scale) * placement_scale).max(1e-6)
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

/// Same anchoring for the plain-mouse events (`contextmenu`), which carry
/// [`MouseData`] rather than pointer data.
fn event_view_point_mouse(anchor: &Signal<CanvasAnchor>, evt: &Event<MouseData>) -> [f32; 2] {
    let point = evt.data().client_coordinates();
    let origin = anchor.peek().origin();
    [point.x as f32 - origin[0], point.y as f32 - origin[1]]
}

fn event_view_point_wheel(anchor: &Signal<CanvasAnchor>, evt: &Event<WheelData>) -> [f32; 2] {
    let point = evt.data().client_coordinates();
    let origin = anchor.peek().origin();
    [point.x as f32 - origin[0], point.y as f32 - origin[1]]
}

#[cfg(test)]
mod tests {
    use lpc_mapping::{FilledPolygonShape, GridCorner, GridRouting, PolygonShape, RepeatShape};

    use super::*;
    use crate::editor_core::map_tool::PolygonMode;

    fn square() -> Vec<[f32; 2]> {
        vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]
    }

    /// Close-on-first only arms once the draft is an outline, and its ring is
    /// SCREEN-sized: zoomed out, the same doc-space miss that closes at low
    /// zoom must not close at high zoom.
    #[test]
    fn the_close_target_is_a_screen_sized_ring_on_the_first_vertex() {
        let draft = vec![[0.0_f32, 0.0], [100.0, 0.0], [100.0, 100.0]];
        assert!(closes_polygon_draft(&draft, [4.0, 3.0], 1.0), "5 px < 9 px");
        assert!(!closes_polygon_draft(&draft, [16.0, 0.0], 1.0));
        // Zoomed 4× in, 5 doc units is 20 screen px — outside the ring.
        assert!(!closes_polygon_draft(&draft, [4.0, 3.0], 4.0));
        // Zoomed out, the same ring reaches further in doc units (9 px at
        // half scale is 18 units).
        assert!(closes_polygon_draft(&draft, [16.0, 0.0], 0.5));
        // Two vertices are not an outline yet, however close the click.
        assert!(!closes_polygon_draft(&draft[..2], [0.0, 0.0], 1.0));
        assert!(!closes_polygon_draft(&[], [0.0, 0.0], 1.0));
    }

    /// Closed chains are DRAWN closed (the seam edge is real geometry a hit
    /// line and a repeat ghost must cover); open ones are left alone.
    #[test]
    fn closed_chains_repeat_their_first_vertex_when_drawn() {
        let polygon = Map2dShape::Polygon(PolygonShape {
            points: square(),
            count: 15,
            align: lpc_mapping::PathAlign::On,
        });
        assert!(vertices_are_closed(&polygon));
        assert_eq!(chain_points(&square(), true).len(), 5);
        assert_eq!(chain_points(&square(), true)[4], [0.0, 0.0]);
        assert_eq!(chain_points(&square(), false).len(), 4);
        // A degenerate chain has no loop to close.
        assert_eq!(chain_points(&square()[..2], true).len(), 2);

        let path = Map2dShape::Path(lpc_mapping::PathShape {
            points: square(),
            count: 4,
            reversed: false,
            gaps: Vec::new(),
            align: lpc_mapping::PathAlign::On,
        });
        assert!(!vertices_are_closed(&path));
        assert!(!is_filled_polygon(&path));
    }

    /// Both closed-shape predicates reach through repeat wrappers — a
    /// repeated shaped matrix still draws its silhouette and its seam.
    #[test]
    fn the_shape_predicates_reach_through_repeats() {
        let filled = Map2dShape::FilledPolygon(FilledPolygonShape {
            points: square(),
            pitch: 26.0,
            angle_deg: 0.0,
            origin: [0.0, 0.0],
            routing: GridRouting::Snake,
            start_corner: GridCorner::Tl,
        });
        assert!(is_filled_polygon(&filled));
        assert!(vertices_are_closed(&filled));
        let wrapped = Map2dShape::Repeat(RepeatShape {
            shape: Box::new(filled),
            center: [0.0, 0.0],
            count: 3,
        });
        assert!(is_filled_polygon(&wrapped));
        assert!(vertices_are_closed(&wrapped));
    }

    /// The filled-mode ghosts ARE the resolver's cells: the preview and the
    /// finished object agree lamp for lamp, which is the whole reason the
    /// draft resolves a real shape instead of sketching one.
    #[test]
    fn filled_draft_ghosts_are_the_resolved_lattice() {
        let draft = square();
        let shape = polygon_draft_shape(&draft, PolygonMode::Filled).expect("an outline");
        let Map2dShape::FilledPolygon(filled) = &shape else {
            panic!("filled mode commits a shaped matrix");
        };
        let cells = lpc_mapping::filled_polygon_cells(filled);
        let ghosts = ghost_positions(shape.clone());
        assert_eq!(ghosts.len(), cells.len());
        assert_eq!(ghosts.len(), 16, "100×100 at pitch 26 is a 4×4 lattice");
        for (ghost, cell) in ghosts.iter().zip(cells.iter()) {
            assert_eq!(ghost, cell);
        }
        // Outline mode previews the perimeter population of the same draft.
        let outline = polygon_draft_shape(&draft, PolygonMode::Outline).expect("an outline");
        assert_eq!(ghost_positions(outline).len(), 15);
    }

    /// The edge a double-click means, and where its corner sits. The seat is
    /// the segment's index PLUS ONE, which for the closing seam is the end of
    /// the list — index 0 would drop the new corner on the far side of the
    /// shape.
    #[test]
    fn an_edge_hit_names_its_segment_and_seats_the_seam_at_the_end() {
        let points = square();
        // Top edge (segment 0): the corner lands ON the edge, not at the aim.
        let hit = hit_edge(&points, true, [50.0, 3.0], 1.0).expect("the top edge");
        assert_eq!(hit.segment, 0);
        assert_eq!(hit.at, [50.0, 0.0]);
        assert_eq!(hit.insert_index(), 1);
        // The implicit closing seam [0,100] → [0,0] is the LAST segment, and
        // its corner appends.
        let seam = hit_edge(&points, true, [-2.0, 50.0], 1.0).expect("the seam");
        assert_eq!(seam.segment, 3);
        assert_eq!(seam.at, [0.0, 50.0]);
        assert_eq!(seam.insert_index(), points.len());
        // An open chain has no seam: the same click hits nothing.
        assert!(hit_edge(&points, false, [-2.0, 50.0], 1.0).is_none());
        assert_eq!(
            hit_edge(&points, false, [50.0, 3.0], 1.0).map(|hit| hit.segment),
            Some(0)
        );
        // Beyond the hit line's own half-width, nothing.
        assert!(hit_edge(&points, true, [50.0, 9.0], 1.0).is_none());
        // The band is SCREEN-sized: zoomed 4× in, 3 doc units is 12 px out.
        assert!(hit_edge(&points, true, [50.0, 3.0], 4.0).is_none());
        // Near a corner the gesture is about that corner, not the edge.
        assert!(hit_edge(&points, true, [4.0, 0.0], 1.0).is_none());
        // Degenerate chains have no edge at all.
        assert!(hit_edge(&points[..1], true, [0.0, 0.0], 1.0).is_none());
        assert!(hit_edge(&[], true, [0.0, 0.0], 1.0).is_none());
    }

    /// Two edges within reach of one click: the NEAREST wins, so a click in a
    /// tight corner region never picks the far side.
    #[test]
    fn an_edge_hit_picks_the_nearest_edge() {
        // A 6-unit-wide sliver: both long edges are inside the hit band.
        let sliver = vec![[0.0_f32, 0.0], [100.0, 0.0], [100.0, 6.0], [0.0, 6.0]];
        assert_eq!(
            hit_edge(&sliver, true, [50.0, 2.0], 1.0).map(|hit| hit.segment),
            Some(0),
            "nearer the top edge"
        );
        assert_eq!(
            hit_edge(&sliver, true, [50.0, 4.0], 1.0).map(|hit| hit.segment),
            Some(2),
            "nearer the bottom edge"
        );
    }
}
