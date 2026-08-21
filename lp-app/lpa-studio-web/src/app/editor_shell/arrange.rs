//! Arrange POLICY for the Mapping view's fixture activity (the one-canvas
//! cutover): sprite building — resolve loaded map2d bodies, display
//! subsampling, honest placeholder/strip fallbacks, auto-pack — plus the
//! drag-override-until-echo lifecycle, project-space fit bounds, and the
//! event wiring from the crate canvas's fixture grammar to
//! [`lpa_studio_core::EditorMetaOp`] / `PatchSelect` dispatch.
//!
//! The canvas ([`lpa_mapping_editor::EditorCanvas`]) is the SURFACE: it
//! renders [`FixtureSprite`]s and emits [`FixtureEvent`]s; every
//! project-shaped decision — what a key means, when a write happens, when
//! an override retires — stays here.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lpa_mapping_editor::{
    Camera, CanvasDrag, EditorCanvas, EditorViewOptions, FitReconcile, FixtureBody, FixtureEvent,
    FixtureSprite, HelpFloat, MapEditorSession, Placement, ZoomFloat, display_inset_padding,
    object_color, tool_hint,
};
use lpa_studio_core::{
    ArtifactLocation, EditorMetaFixture, EditorMetaOp, EditorMetaVerb, NodeId, ProjectController,
    ProjectEditorOp, UiAction, UiArrangeTransform, UiPatchSurface, UiPatchTarget,
};
use lpc_mapping::Bounds2d;

use crate::app::node::lamp_view::fixture_live_colors;

/// Display cap: a fixture with more lamps than this renders every k-th
/// lamp (display subsampling only — dome-scale fixtures must not melt the
/// SVG; the resolver still ran the full document).
const MAX_DISPLAY_LAMPS: usize = 2000;

/// Gap between auto-packed fixtures, in project units.
const PACK_GAP: f64 = 24.0;

/// One fixture's policy facts: the sprite the canvas renders, plus what
/// the events need (node identity, instance windows).
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

/// A committed drag held on screen until the snapshot confirms it: the
/// override survives pointer-up so the fixture never snaps back while the
/// write round-trips (the jump-back bug).
#[derive(Clone, PartialEq)]
pub(crate) struct DragOverride {
    key: String,
    transform: UiArrangeTransform,
    /// True after pointer-up: the override retires once the surface
    /// carries (approximately — the kernel quantizes) this transform.
    committed: bool,
}

/// `UiArrangeTransform` and the crate's `Placement` are the same shape
/// (translate ∘ rotate ∘ uniform scale); these convert at the boundary.
pub(crate) fn placement_of(transform: &UiArrangeTransform) -> Placement {
    Placement {
        t: transform.t,
        r: transform.r,
        s: transform.s,
    }
}

pub(crate) fn transform_of(placement: &Placement) -> UiArrangeTransform {
    UiArrangeTransform {
        t: placement.t,
        r: placement.r,
        s: placement.s,
    }
}

/// The dived fixture, as the canvas host needs it: which node, whose
/// session, and whether the asset pipeline has it editable (a refused or
/// still-loading body keeps the fixture a sprite).
#[derive(Clone, PartialEq)]
pub(crate) struct DiveHost {
    pub node: NodeId,
    pub session: Signal<MapEditorSession>,
    pub editable: bool,
}

/// The one-canvas host for BOTH activities: sprites built from the
/// surface, the project camera (fit-all seed, then frozen — neither
/// arranging nor DIVING ever moves it), the override lifecycle, and — when
/// dived — the live session rendered through the focused fixture's
/// placement with the editor furniture (hint, zoom, help) around it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ProjectCanvasHost(
    surface: UiPatchSurface,
    /// map2d bodies by artifact (extracted from the snapshot's node views;
    /// stories inject embedded-example bytes directly).
    bodies: BTreeMap<ArtifactLocation, String>,
    selection: Option<UiPatchTarget>,
    /// Sticky auto-pack slots (shell-owned; see [`PackSlots`]). Stories
    /// omit it and get the ad-hoc packing.
    #[props(default)]
    pack: PackSlots,
    /// The dive: layer state, not component identity — the same mounted
    /// canvas takes the session + placement when set.
    #[props(default)]
    dive: Option<DiveHost>,
    /// Center-owned view toggles (the toolbar drives them). Stories omit
    /// it and get defaults.
    #[props(default)]
    view_opts: Option<Signal<EditorViewOptions>>,
    /// Center-owned fit request (the `0` key and the zoom float's percent
    /// button arm it). Stories omit it.
    #[props(default)]
    fit_pending: Option<Signal<bool>>,
    /// Fired after any committed session change while dived (the center
    /// bumps the commit pipeline).
    #[props(default)]
    on_committed: Option<EventHandler<()>>,
    /// Double-click on a fixture (or a NEIGHBOUR while dived — the
    /// dive-switch): the shell dives into it. Absent = arrange-only
    /// (stories).
    #[props(default)]
    on_focus: Option<EventHandler<NodeId>>,
    /// Live colors on ALL fixture sprites, default-on (the Patching
    /// view's guide invariant: patched objects glow with product data,
    /// unpatched stay palette). The Mapping view omits it — its live
    /// display is the dived feed behind the L toggle.
    #[props(default)]
    live_sprites: bool,
    /// The Patching view passes true: a sprite is a COUNTERPART an armed
    /// assign can complete against, exactly like its tree row. Selection
    /// is unchanged either way — plain clicks never write.
    #[props(default)]
    patch_verbs: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    // The frame-scoped arm; absent outside the workbench frame (stories).
    let patching_ui =
        use_hook(try_consume_context::<crate::app::editor_shell::patching::PatchingUi>);
    // Geometry is derived per (surface, bodies, pack, selection) change —
    // resolver runs are cheap at fixture grain and the memo keeps drag
    // overrides and camera work off that path.
    let memo_surface = surface.clone();
    let memo_bodies = bodies.clone();
    let memo_pack = pack.clone();
    let memo_selection = selection.clone();
    let renders = use_memo(use_reactive!(|(
        memo_surface,
        memo_bodies,
        memo_pack,
        memo_selection,
    )| {
        let renders = build_renders(&memo_surface, &memo_bodies, &memo_pack);
        let sprites = renders
            .iter()
            .map(|render| sprite_of(render, &memo_selection))
            .collect::<Vec<_>>();
        let nodes: BTreeMap<String, NodeId> = renders
            .iter()
            .map(|render| (render.key.clone(), render.node))
            .collect();
        (sprites, nodes)
    }));
    // The project camera: seeded from fit-all when content first appears,
    // then FROZEN — neither arranging a fixture nor DIVING into one ever
    // moves the camera (the drift bug + the one-canvas ruling). The zoom
    // float's fit re-frames on demand.
    let mut camera = use_signal(Camera::new);
    let viewport = use_signal(|| None::<[f32; 2]>);
    // Which measurement the current fit consumed — so a viewport that
    // settles AFTER the fit re-runs it (see the fit block below).
    let mut fit_done = use_signal(FitReconcile::default);
    let local_fit = use_signal(|| true);
    let mut fit_pending = fit_pending.unwrap_or(local_fit);
    let local_view = use_signal(EditorViewOptions::default);
    let view_opts = view_opts.unwrap_or(local_view);
    // The canvas's session prop idles on an empty document while no
    // fixture is dived (there is nothing to edit in fixture view).
    let arrange_session = use_signal(|| MapEditorSession::new(lpc_mapping::Map2dDoc::new()));
    let arrange_drag = use_signal(|| None::<CanvasDrag>);
    let mut arrange_live = use_signal(Vec::<[u8; 3]>::new);
    // Which fixture the live feed's colors belong to — a dive SWITCH must
    // drop them (another fixture's colors on this doc's lamps would lie).
    let mut live_source = use_signal(|| None::<NodeId>);
    // Live drag override, held until the snapshot confirms the write.
    let mut drag_override = use_signal(|| None::<DragOverride>);

    let (base_sprites, nodes) = renders.read().clone();
    // Retire a committed override once the surface caught up (the kernel
    // quantizes to 4 decimals, so compare loosely).
    let retire_override = {
        let over = drag_override.peek().clone();
        matches!(over, Some(over) if over.committed
        && base_sprites.iter().any(|sprite| {
            sprite.key == over.key
                && transforms_close(&transform_of(&sprite.placement), &over.transform)
        }))
    };
    if retire_override {
        drag_override.set(None);
    }
    // Effective sprites: the override wins while it lives.
    let mut sprites = base_sprites;
    if let Some(over) = drag_override.read().as_ref()
        && let Some(sprite) = sprites.iter_mut().find(|sprite| sprite.key == over.key)
    {
        sprite.placement = placement_of(&over.transform);
    }

    // The dive as the canvas sees it: the focused fixture's key, effective
    // placement, and own-space bounds — only once the asset pipeline made
    // it editable (a loading/refused body keeps the fixture a sprite).
    let dive_focus: Option<(String, Placement, [f64; 4])> = dive
        .as_ref()
        .filter(|dive| dive.editable)
        .and_then(|dive| {
            nodes
                .iter()
                .find(|(_, node)| **node == dive.node)
                .map(|(key, _)| key.clone())
        })
        .and_then(|key| {
            sprites
                .iter()
                .find(|sprite| sprite.key == key)
                .map(|sprite| (key, sprite.placement, sprite.bounds))
        });

    // The live feed (the toolbar's `live` / L): while dived, decode the
    // focused fixture's lamp colors out of its output's published wire
    // frame and WRITE them into the canvas's live signal — a signal, never
    // a prop, so a frame tick re-renders nothing (the canvas applies them
    // as direct DOM writes). Keep-last-good: an apply round-trip can drop
    // the frame for a tick or two, and falling back to the palette would
    // read as live mode dropping out.
    if dive_focus.is_some()
        && let Some(dive) = dive.as_ref()
    {
        if *live_source.peek() != Some(dive.node) {
            live_source.set(Some(dive.node));
            arrange_live.set(Vec::new());
        }
        if view_opts.read().live {
            let colors = surface
                .fixtures
                .iter()
                .find(|fixture| fixture.node == dive.node)
                .map(|fixture| fixture_live_colors(&fixture.patch))
                .unwrap_or_default();
            if !colors.is_empty() && *arrange_live.peek() != colors {
                arrange_live.set(colors);
            }
        }
    }

    // The SPRITE live feed (live_sprites hosts — the Patching view): one
    // color vec per fixture key, decoded from each fixture's published
    // frame per snapshot tick. Keep-last-good PER FIXTURE across apply
    // gaps (same reasoning as the dived feed); compare-then-set so a
    // quiet frame re-renders nothing.
    let mut sprite_live = use_signal(BTreeMap::<String, Vec<[u8; 3]>>::new);
    if live_sprites {
        let previous = sprite_live.peek().clone();
        let mut feeds = BTreeMap::new();
        for (key, node) in &nodes {
            let colors = surface
                .fixtures
                .iter()
                .find(|fixture| fixture.node == *node)
                .map(|fixture| fixture_live_colors(&fixture.patch))
                .unwrap_or_default();
            if colors.is_empty() {
                if let Some(kept) = previous.get(key) {
                    feeds.insert(key.clone(), kept.clone());
                }
            } else {
                feeds.insert(key.clone(), colors);
            }
        }
        if previous != feeds {
            sprite_live.set(feeds);
        }
    }

    // Fit runs at render, guarded: armed at mount and by the zoom float /
    // `0` key, waiting for a real viewport measurement and real bounds.
    // Dived, fit frames the FOCUSED fixture's placed bounds (the optional
    // "snap viewport to fixture" affordance); otherwise it fits all.
    // The fit ALSO re-runs when the measurement moves while the camera is
    // still exactly the value the last fit produced: the first
    // measurement races container layout settling (docks, the mobile
    // fold), and freezing the camera on it baked a nondeterministic zoom
    // into story baselines (the churner —
    // docs/debt/story-capture-pipeline.md). Once the user pans or zooms,
    // the camera is theirs and reconciliation stops.
    {
        let viewport_now = *viewport.read();
        if let Some([width, height]) = viewport_now
            && (*fit_pending.read() || fit_done.read().stale([width, height], &camera.peek()))
        {
            let bounds = match &dive_focus {
                Some((_, placement, own_bounds)) => {
                    let corners = placement.corners(*own_bounds);
                    let min_x = corners.iter().map(|c| c[0]).fold(f64::MAX, f64::min);
                    let min_y = corners.iter().map(|c| c[1]).fold(f64::MAX, f64::min);
                    let max_x = corners.iter().map(|c| c[0]).fold(f64::MIN, f64::max);
                    let max_y = corners.iter().map(|c| c[1]).fold(f64::MIN, f64::max);
                    Some(lpc_mapping::Bounds2d {
                        min_x: min_x as f32,
                        min_y: min_y as f32,
                        width: (max_x - min_x).max(1.0) as f32,
                        height: (max_y - min_y).max(1.0) as f32,
                    })
                }
                None => fit_bounds(&sprites),
            };
            if let Some(bounds) = bounds {
                let padding = match &dive_focus {
                    Some(_) => display_inset_padding(bounds, width, height),
                    None => 0.0,
                };
                camera.write().fit(bounds, width, height, padding);
                if *fit_pending.peek() {
                    fit_pending.set(false);
                }
            }
            // Reconcile even with nothing to frame — the default camera is
            // deterministic too, and the capture guard must clear on an
            // empty canvas. Change-guarded: a bare signal write at render
            // would re-render forever.
            let mut next = *fit_done.peek();
            next.record([width, height], *camera.peek());
            if *fit_done.peek() != next {
                fit_done.set(next);
            }
        }
    }

    let dispatch_set = arrange_set_dispatch(&surface);
    let select = move |target: Option<UiPatchTarget>| {
        on_action.call(UiAction::from_op(
            lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
            ProjectEditorOp::PatchSelect { target },
        ));
    };
    let on_fixture = {
        let nodes = nodes.clone();
        let grammar_surface = surface.clone();
        let grammar_selection = selection.clone();
        move |event: FixtureEvent| match event {
            FixtureEvent::Select(Some(key)) => {
                if let Some(node) = nodes.get(&key) {
                    drag_override.set(None);
                    let target = UiPatchTarget::Fixture { node: *node };
                    // The same fixture-side completion the tree's rows
                    // carry: armed assign + a free segment → the clicked
                    // sprite takes it. Unarmed, this is a plain select.
                    if patch_verbs {
                        crate::app::editor_shell::patching::complete_assign_on_object(
                            &on_action,
                            &grammar_surface,
                            &grammar_selection,
                            patching_ui,
                            &target,
                        );
                    }
                    select(Some(target));
                }
            }
            FixtureEvent::Select(None) => {
                drag_override.set(None);
                select(None);
            }
            FixtureEvent::Move {
                key,
                placement,
                commit,
            } => {
                let transform = transform_of(&placement);
                if commit {
                    // One gesture = one op = one undo step. The override
                    // stays up (committed) until the snapshot echoes the
                    // write — no snap-back.
                    drag_override.set(Some(DragOverride {
                        key: key.clone(),
                        transform,
                        committed: true,
                    }));
                    if let Some(node) = nodes.get(&key)
                        && let Some(op) = dispatch_set(&key, *node, transform)
                    {
                        on_action.call(UiAction::from_op(ProjectController::NODE_ID, op));
                    }
                } else {
                    drag_override.set(Some(DragOverride {
                        key,
                        transform,
                        committed: false,
                    }));
                }
            }
            FixtureEvent::Dive(key) => {
                if let Some(node) = nodes.get(&key)
                    && let Some(on_focus) = &on_focus
                {
                    on_focus.call(*node);
                }
            }
        }
    };

    let canvas_session = dive
        .as_ref()
        .filter(|_| dive_focus.is_some())
        .map(|dive| dive.session)
        .unwrap_or(arrange_session);
    let dive_placement = dive_focus
        .as_ref()
        .map(|(_, placement, _)| *placement)
        .unwrap_or_default();
    let focused_key = dive_focus.as_ref().map(|(key, _, _)| key.clone());
    let hint = dive_focus
        .is_some()
        .then(|| tool_hint(&canvas_session.read()));
    let committed = on_committed.unwrap_or_else(|| EventHandler::new(|()| {}));
    rsx! {
        div {
            class: "lpme-canvas-wrap",
            // The geometry guard (clock-face precedent): the size the
            // camera's fit was reconciled against. The story capture's
            // ready gate refuses to photograph a visible canvas whose
            // real box disagrees with this stamp.
            "data-fit-viewport": fit_done.read().guard_attr(),
            EditorCanvas {
                session: canvas_session,
                camera,
                view_opts,
                viewport,
                drag: arrange_drag,
                live_feed: arrange_live,
                sprite_live_feed: live_sprites.then_some(sprite_live),
                on_committed: committed,
                placement: dive_placement,
                fixtures: sprites,
                focused: focused_key,
                on_fixture,
            }
            if let Some(hint) = hint {
                div { class: "lpme-hint", "{hint}" }
            }
            ZoomFloat { camera, viewport, fit_pending }
            if dive_focus.is_some() {
                HelpFloat {}
            }
        }
    }
}

/// Prebuild the `EditorMetaOp::Set` factory: `editor.json` artifact + the
/// fixture facts every write refreshes footprints through. `None` = the
/// artifact is unknown (surface not settled), so moves no-op honestly.
fn arrange_set_dispatch(
    surface: &UiPatchSurface,
) -> impl Fn(&str, NodeId, UiArrangeTransform) -> Option<EditorMetaOp> + Clone + 'static {
    let artifact = surface.editor_meta_artifact.clone();
    let fixtures: Vec<EditorMetaFixture> = surface
        .fixtures
        .iter()
        .filter_map(|fixture| {
            Some(EditorMetaFixture {
                node_key: fixture.address.clone()?,
                mapping_artifact: fixture.mapping_artifact.clone(),
            })
        })
        .collect();
    move |key, node, transform| {
        Some(EditorMetaOp {
            artifact: artifact.clone()?,
            fixtures: fixtures.clone(),
            verb: EditorMetaVerb::Set {
                node_key: key.to_string(),
                node: Some(node),
                transform,
            },
        })
    }
}

/// Loose transform equality at the kernel's canonical precision (writes
/// quantize to 4 decimals).
fn transforms_close(a: &UiArrangeTransform, b: &UiArrangeTransform) -> bool {
    let close = |x: f64, y: f64| (x - y).abs() < 1e-3;
    close(a.t[0], b.t[0]) && close(a.t[1], b.t[1]) && close(a.r, b.r) && close(a.s, b.s)
}

/// A render's canvas sprite, selection flags applied.
fn sprite_of(render: &FixtureRender, selection: &Option<UiPatchTarget>) -> FixtureSprite {
    FixtureSprite {
        key: render.key.clone(),
        label: render.label.clone(),
        color: render.color.to_string(),
        placement: placement_of(&render.transform),
        bounds: render.bounds,
        body: render.body.clone(),
        arranged: render.arranged,
        selected: selection_touches(selection, render.node),
        selected_range: selected_instance_range(selection, render),
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

/// Sticky auto-pack slots, keyed by editor key. Owned by the shell; a
/// fixture keeps its slot for the LIFE OF THE MOUNT once assigned — even
/// while arranged, so undoing an arrange returns it to its old slot — and
/// no event ever re-packs an existing slot (the G1 jump bug: re-packing
/// the whole set whenever it changed made every neighbour move when one
/// fixture was dragged).
pub(crate) type PackSlots = BTreeMap<String, UiArrangeTransform>;

/// Grow the held slots to cover the current unarranged set: `None` when
/// every unarranged fixture already has a slot (keep them — stability is
/// the point), else the held slots plus freshly packed positions for the
/// fixtures that have NEVER had one. Existing slots are never moved or
/// dropped.
pub(crate) fn refresh_pack_slots(
    surface: &UiPatchSurface,
    bodies: &BTreeMap<ArtifactLocation, String>,
    held: &PackSlots,
) -> Option<PackSlots> {
    let renders = build_renders(surface, bodies, held);
    merge_pack_slots(&renders, held)
}

/// The pure half of [`refresh_pack_slots`]: adopt the auto-packed
/// transform of every unarranged fixture without a held slot.
fn merge_pack_slots(renders: &[FixtureRender], held: &PackSlots) -> Option<PackSlots> {
    let fresh: Vec<&FixtureRender> = renders
        .iter()
        .filter(|render| !render.arranged && !held.contains_key(&render.key))
        .collect();
    if fresh.is_empty() {
        return None;
    }
    let mut next = held.clone();
    for render in fresh {
        next.insert(render.key.clone(), render.transform);
    }
    Some(next)
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
/// (stable order = surface order). When held slots exist their fixtures
/// define the live row, and new fixtures CONTINUE it to the right —
/// re-deriving a row from the arranged extent would collide with (or sit
/// away from) the held neighbours. Ephemeral: nothing is written until a
/// fixture is first dragged.
fn auto_pack(renders: &mut [FixtureRender], held: &PackSlots) {
    // The held row: every held slot's world-space top edge and right
    // extent — measured at the SLOT placement even for fixtures currently
    // arranged, because an undone arrange returns them to that slot (the
    // slot stays reserved). Held slots are always pack-made (r=0, s=1,
    // tops aligned), so the min top recovers the row's y.
    let mut row_top = f64::MAX;
    let mut held_right = f64::MIN;
    for render in renders.iter() {
        let Some(slot) = held.get(&render.key) else {
            continue;
        };
        for point in placement_of(slot).corners(render.bounds) {
            row_top = row_top.min(point[1]);
            held_right = held_right.max(point[0]);
        }
    }
    let (row_y, mut cursor_x) = if row_top < f64::MAX {
        (row_top, held_right + PACK_GAP)
    } else {
        // No held row yet: start one below the arranged content.
        let arranged_max_y = renders
            .iter()
            .filter(|render| render.arranged)
            .map(|render| {
                // Conservative world-space extent: transformed bounds corners.
                placement_of(&render.transform)
                    .corners(render.bounds)
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
        (row_y, 0.0)
    };
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

/// Fit-all: the union of every fixture's transformed frame in project
/// space, padded like the old viewBox fit (generous top room for the name
/// tags above frames).
fn fit_bounds(sprites: &[FixtureSprite]) -> Option<Bounds2d> {
    let mut min = [f64::MAX, f64::MAX];
    let mut max = [f64::MIN, f64::MIN];
    for sprite in sprites {
        for corner in sprite.placement.corners(sprite.bounds) {
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
    let pad_x = width * 0.10 + 10.0;
    let pad_y = height * 0.14 + 14.0;
    Some(Bounds2d {
        min_x: (min[0] - pad_x) as f32,
        min_y: (min[1] - pad_y) as f32,
        width: (width + 2.0 * pad_x) as f32,
        height: (height + 2.0 * pad_y) as f32,
    })
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
    /// content moving; a held fixture is NEVER re-packed.
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

    /// The G1 jump bug's regression test: one fixture leaving the
    /// unarranged set (first drag) must not move anyone else — the merge
    /// keeps every held slot and reports nothing new.
    #[test]
    fn arranging_a_fixture_never_repacks_its_neighbours() {
        let slot = |x: f64| UiArrangeTransform {
            t: [x, 90.0],
            r: 0.0,
            s: 1.0,
        };
        let held: PackSlots = [("b".to_string(), slot(0.0)), ("c".to_string(), slot(44.0))]
            .into_iter()
            .collect();
        // "b" was just dragged: arranged now, but its slot stays held.
        let renders = vec![
            render("b", true, [0.0, 0.0, 20.0, 20.0]),
            render("c", false, [0.0, 0.0, 20.0, 20.0]),
        ];
        assert_eq!(
            merge_pack_slots(&renders, &held),
            None,
            "no fresh fixtures: held slots (including b's, for undo) stay put"
        );
    }

    /// Undo of a first drag: the fixture returns to the unarranged set and
    /// its RETAINED slot places it exactly where it was.
    #[test]
    fn undone_arrange_returns_to_the_old_slot() {
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
        let mut renders = vec![render("b", false, [0.0, 0.0, 20.0, 20.0])];
        assert_eq!(merge_pack_slots(&renders, &held), None);
        for item in renders.iter_mut().filter(|item| !item.arranged) {
            if let Some(slot) = held.get(&item.key) {
                item.transform = *slot;
            }
        }
        auto_pack(&mut renders, &held);
        assert_eq!(renders[0].transform.t, [7.0, 9.0]);
    }

    /// A NEW fixture continues the held row to the right — including past
    /// slots reserved by fixtures currently arranged (undo returns them
    /// there), never into or below them.
    #[test]
    fn new_fixtures_continue_the_held_row() {
        let slot = |x: f64| UiArrangeTransform {
            t: [x, 90.0],
            r: 0.0,
            s: 1.0,
        };
        let held: PackSlots = [("b".to_string(), slot(0.0)), ("c".to_string(), slot(44.0))]
            .into_iter()
            .collect();
        // "c" is arranged away; its slot (right edge x=64) stays reserved.
        let mut renders = vec![
            render("b", false, [0.0, 0.0, 20.0, 20.0]),
            render("c", true, [0.0, 0.0, 20.0, 20.0]),
            render("d", false, [0.0, 0.0, 20.0, 20.0]),
        ];
        for item in renders.iter_mut().filter(|item| !item.arranged) {
            if let Some(slot) = held.get(&item.key) {
                item.transform = *slot;
            }
        }
        auto_pack(&mut renders, &held);
        assert_eq!(
            renders[2].transform.t,
            [64.0 + PACK_GAP, 90.0],
            "d appends after c's reserved slot, on the held row's y"
        );
        let merged = merge_pack_slots(&renders, &held).expect("d is fresh");
        assert_eq!(merged.len(), 3);
        assert_eq!(merged["b"], slot(0.0));
        assert_eq!(merged["d"].t, [64.0 + PACK_GAP, 90.0]);
    }

    #[test]
    fn fit_bounds_covers_every_transformed_frame() {
        let sprites: Vec<FixtureSprite> = [
            render("a", true, [0.0, 0.0, 40.0, 40.0]),
            render("b", false, [0.0, 0.0, 20.0, 20.0]),
        ]
        .iter()
        .map(|r| sprite_of(r, &None))
        .collect();
        let bounds = fit_bounds(&sprites).expect("bounds");
        assert!(bounds.min_x <= 0.0 && bounds.min_y <= 0.0);
        assert!(bounds.min_x + bounds.width >= 90.0, "{bounds:?}");
    }

    #[test]
    fn placement_transform_round_trips() {
        let transform = UiArrangeTransform {
            t: [12.5, -3.0],
            r: 22.5,
            s: 0.75,
        };
        assert_eq!(transform_of(&placement_of(&transform)), transform);
    }
}
