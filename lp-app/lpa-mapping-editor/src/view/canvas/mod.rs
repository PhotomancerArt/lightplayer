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
//! wiring arrows, lamps, wiring numbers, the selection outline + corner
//! resize handles, path vertex handles, the path-draft preview, and the
//! marquee.
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
use crate::editor_core::editor_session::{MapEditorSession, editable_path};
use crate::editor_core::map_tool::MapTool;
use crate::editor_core::placement::Placement;
use crate::editor_core::view_geometry::{ArrowInput, wiring_arrows};
use crate::view::view_options::EditorViewOptions;

pub use canvas_anchor::{CanvasAnchor, capture_pointer};
pub use lamp_metrics::{authored_spans, fit_region, lamp_display_radius};
pub use palette::object_color;

pub use layers::cells::{LampCell, lamp_cells};
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
                    align: lpc_mapping::PathAlign::On,
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
                // Path-tool finish keeps priority (editor grammar; never
                // in fixture mode — there is no tool without a session).
                if !fixture_mode && matches!(session.read().tool, MapTool::Path { .. }) {
                    if session.write().path_finish().is_some() {
                        on_committed.call(());
                    }
                    return;
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
                        path_objects: &path_objects,
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
                    })}
                    {marquee_layer(eff, marquee_rect)}
                }
                }
            }
        }
        {menu_overlay}
    }
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
