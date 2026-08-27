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
    FixtureSprite, HelpFloat, LampCell, MapEditorSession, Placement, SpriteObject, ZoomFloat,
    aligned_outline, display_inset_padding, hit_body, lamp_cells, object_color, tool_hint,
};
use lpa_studio_core::{
    ArtifactLocation, EditorMetaFixture, EditorMetaOp, EditorMetaVerb, NodeId, ProjectController,
    ProjectEditorOp, UiAction, UiArrangeTransform, UiPatchSurface, UiPatchTarget, UiSelection,
};
use lpc_mapping::{Bounds2d, Map2dDoc, Map2dShape, PathAlign, ResolvedMap2d};

use crate::app::node::lamp_view::{UNLIT_RGB, fixture_live_colors};
use crate::app::patch::patch_panel::srgb8;

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
    /// Are this render's BOUNDS the real ones — a resolved body, a
    /// shape-less strip, or a footprint-backed placeholder? A bare
    /// placeholder's guessed block must never be baked into a held pack
    /// slot (G1 round 1: slots computed from guessed bounds diverged
    /// between views and shifted once the body arrived).
    settled: bool,
    /// `(path, start, lamps)` for instance selection rings.
    instances: Vec<(String, u32, u32)>,
    /// The document's physical strands, from the SAME parse the body came
    /// from — how each run of lamps is drawn (see [`StrandMeta`]). Empty for
    /// bodies with no document behind them.
    strands: Vec<StrandMeta>,
    /// The doc's declared lamp footprint, in ITS OWN (arbitrary) units —
    /// the only physical length a document states, so every derived render
    /// length floors on it rather than on some absolute constant.
    sample_diameter: f32,
}

/// One physical strand of a fixture's map2d document, as the CANVAS needs
/// it: which lamps it covers, and the facts its body is drawn from.
///
/// A strand is a resolver span (a repeat's instance is already its own) cut
/// again at the path's jumper gaps — a jumper is a physical break, so no
/// body may bridge one, exactly as no body bridges two objects.
#[derive(Clone, PartialEq)]
struct StrandMeta {
    /// Lamp range in the fixture's TRUE numbering.
    start: u32,
    count: u32,
    /// Authored stroke alignment (map2d format 4).
    align: PathAlign,
    /// These lamps read as a RIBBON and wear voronoi cells (path, polygon);
    /// grid and ring lamps are a field and keep their dots.
    cells: bool,
    /// The lamps close their own loop (a polygon's perimeter wraps), so the
    /// outline repeats the first lamp and the band comes out an annulus
    /// rather than a ribbon with a mouth at the seam.
    closed: bool,
}

impl StrandMeta {
    fn owns(&self, lamp: u32) -> bool {
        lamp >= self.start && lamp < self.start.saturating_add(self.count)
    }

    /// Does this strand fall inside `start..end` (an instance's window)?
    fn within(&self, start: u32, end: u32) -> bool {
        self.start >= start && self.start < end
    }
}

/// A committed gesture held on screen until the snapshot confirms it: the
/// override survives pointer-up so the fixtures never snap back while the
/// write round-trips (the jump-back bug). One gesture = one override,
/// possibly many fixtures (the multi move/scale).
#[derive(Clone, PartialEq)]
pub(crate) struct DragOverride {
    transforms: std::collections::BTreeMap<String, UiArrangeTransform>,
    /// True after pointer-up: the override retires once the surface
    /// carries (approximately — the kernel quantizes) every transform.
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
    selection: UiSelection,
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
    /// The Mapping view passes true: the fixture selection box wears
    /// corner scale handles (transform furniture is that view's
    /// activity; Patching multi-selects without it).
    #[props(default)]
    transform_handles: bool,
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
    // Has the USER edited content on this mount (a fixture drag/scale, a
    // committed dive edit)? Bounds reconciliation settles ASYNC ARRIVALS
    // only — once the user moves things, their gesture must never re-fit
    // the view under them (G1 round 2: "the view is auto scaling as I
    // drag"). Explicit fits (the zoom float, `0`) still work.
    let mut content_edited = use_signal(|| false);

    let (base_sprites, nodes) = renders.read().clone();
    // Retire a committed override once the surface caught up on EVERY
    // member (the kernel quantizes to 4 decimals, so compare loosely).
    let retire_override = {
        let over = drag_override.peek().clone();
        matches!(over, Some(over) if over.committed
        && over.transforms.iter().all(|(key, transform)| {
            base_sprites.iter().any(|sprite| {
                sprite.key == *key
                    && transforms_close(&transform_of(&sprite.placement), transform)
            })
        }))
    };
    if retire_override {
        drag_override.set(None);
    }
    // Effective sprites: the override wins while it lives.
    let mut sprites = base_sprites;
    if let Some(over) = drag_override.read().as_ref() {
        for sprite in sprites.iter_mut() {
            if let Some(transform) = over.transforms.get(&sprite.key) {
                sprite.placement = placement_of(transform);
            }
        }
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
                .map(|fixture| fixture_live_colors(&surface, fixture))
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
            let Some(fixture) = surface
                .fixtures
                .iter()
                .find(|fixture| fixture.node == *node)
            else {
                continue;
            };
            let mut colors = fixture_live_colors(&surface, fixture);
            // Keep-last-good across an apply gap — but only while the
            // fixture still HAS a run. A fixture with nothing on a wire
            // publishes no frame at all, and keeping its last good colors
            // then would leave the light it used to make painted on lamps
            // that are now dark: exactly the second opinion Q9 exists to
            // remove. Unmapped means unlit, and the selected object's chase
            // arrives from the controller below.
            if colors.is_empty()
                && !fixture.patch.cells.is_empty()
                && let Some(kept) = previous.get(key)
            {
                colors = kept.clone();
            }
            // THE unmapped chase (Q9), from the controller: the selected
            // object has no wire, so no published frame can carry its
            // colors — but the panel strip is showing them, and a sprite
            // that stayed dark would be a second opinion about one object.
            // The very same `chase_preview` paints both.
            let colors = with_chase_preview(&surface, *node, colors, fixture.patch.lamps);
            if colors.is_empty() {
                continue;
            }
            feeds.insert(key.clone(), colors);
        }
        if previous != feeds {
            sprite_live.set(feeds);
        }
    }

    // Fit runs at render, guarded: armed at mount and by the zoom float /
    // `0` key, waiting for a real viewport measurement and real bounds.
    // Dived, fit frames the FOCUSED fixture's placed bounds (the optional
    // "snap viewport to fixture" affordance); otherwise it fits all.
    // The fit ALSO re-runs when either MEASUREMENT moves while the camera
    // is still exactly the value the last fit produced: the first
    // viewport measurement races container layout settling (docks, the
    // mobile fold — the story-baseline churner,
    // docs/debt/story-capture-pipeline.md), and the CONTENT races its own
    // async loads (bodies, the arrangement document — the peach opened
    // out of view, G1 round 1). Once the user pans or zooms, the camera
    // is theirs and reconciliation stops.
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
        let bounds_key =
            bounds.map(|bounds| [bounds.min_x, bounds.min_y, bounds.width, bounds.height]);
        // After the first user edit, reconcile against the RECORDED bounds
        // (a frozen comparison): only the viewport half keeps settling.
        let reconcile_bounds = if *content_edited.read() {
            fit_done.read().fitted_bounds()
        } else {
            bounds_key
        };
        let viewport_now = *viewport.read();
        if let Some([width, height]) = viewport_now
            && (*fit_pending.read()
                || fit_done
                    .read()
                    .stale([width, height], &camera.peek(), reconcile_bounds))
        {
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
            next.record([width, height], *camera.peek(), bounds_key);
            if *fit_done.peek() != next {
                fit_done.set(next);
            }
        }
    }

    let dispatch_set_many = arrange_set_many_dispatch(&surface);
    let select = move |target: Option<UiPatchTarget>| {
        on_action.call(UiAction::from_op(
            lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
            ProjectEditorOp::PatchSelect {
                selection: UiSelection::from_option(target),
            },
        ));
    };
    let dispatch_selection = |on_action: &EventHandler<UiAction>, selection: UiSelection| {
        on_action.call(UiAction::from_op(
            lpa_studio_core::ProjectEditorTarget::NodeTree.node_id(),
            ProjectEditorOp::PatchSelect { selection },
        ));
    };
    let on_fixture = {
        let nodes = nodes.clone();
        let grammar_surface = surface.clone();
        let full_selection = selection.clone();
        // The arm completes against a SINGLE end (multi is not armable).
        let grammar_selection = selection.single().cloned();
        move |event: FixtureEvent| match event {
            FixtureEvent::Select {
                pick: Some(pick),
                toggle,
            } => {
                if let Some(node) = nodes.get(&pick.key) {
                    drag_override.set(None);
                    if toggle {
                        // Shift-click: toggle the FIXTURE within the root
                        // sibling set — the multi-selection gesture. The
                        // finer object grain stays the plain click's
                        // answer; a set is built of fixtures.
                        let mut next = full_selection.clone();
                        next.toggle_sibling(UiPatchTarget::Fixture { node: *node });
                        dispatch_selection(&on_action, next);
                        return;
                    }
                    // The PICK GRAIN is view policy (grain follows
                    // activity): Patching names the OBJECT under the
                    // cursor (Q10 — the walk-up loop taps physical
                    // pieces); Mapping selects the FIXTURE (D4 — click
                    // selects at scope level, double-click descends).
                    let target = plain_click_target(patch_verbs, &grammar_surface, *node, &pick);
                    // The same fixture-side completion the tree's rows
                    // carry: armed assign + a free segment → the clicked
                    // OBJECT takes it (stronger than the old next-unmapped
                    // resolution — a click says WHICH). Unarmed, this is a
                    // plain select.
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
            FixtureEvent::Select { pick: None, .. } => {
                drag_override.set(None);
                select(None);
            }
            FixtureEvent::Marquee { keys, additive } => {
                drag_override.set(None);
                // The marquee selects at FIXTURE grain (root siblings).
                // Additive keeps the current fixture-grain members and
                // adds the swept ones; plain replaces.
                let mut targets: Vec<UiPatchTarget> = if additive {
                    full_selection
                        .targets()
                        .iter()
                        .filter(|target| matches!(target, UiPatchTarget::Fixture { .. }))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                };
                for key in &keys {
                    if let Some(node) = nodes.get(key) {
                        let target = UiPatchTarget::Fixture { node: *node };
                        if !targets.contains(&target) {
                            targets.push(target);
                        }
                    }
                }
                let mut next = full_selection.clone();
                next.set_siblings(targets);
                dispatch_selection(&on_action, next);
            }
            FixtureEvent::Move { moves, commit } => {
                if !*content_edited.peek() {
                    content_edited.set(true);
                }
                let transforms: std::collections::BTreeMap<String, UiArrangeTransform> = moves
                    .iter()
                    .map(|(key, placement)| (key.clone(), transform_of(placement)))
                    .collect();
                if commit {
                    // One gesture = one op = one undo step — however many
                    // fixtures moved. The override stays up (committed)
                    // until the snapshot echoes the write — no snap-back.
                    drag_override.set(Some(DragOverride {
                        transforms: transforms.clone(),
                        committed: true,
                    }));
                    let entries: Vec<lpa_studio_core::EditorMetaSet> = transforms
                        .iter()
                        .map(|(key, transform)| lpa_studio_core::EditorMetaSet {
                            node_key: key.clone(),
                            node: nodes.get(key).copied(),
                            transform: *transform,
                        })
                        .collect();
                    if let Some(op) = dispatch_set_many(entries) {
                        on_action.call(UiAction::from_op(ProjectController::NODE_ID, op));
                    }
                } else {
                    drag_override.set(Some(DragOverride {
                        transforms,
                        committed: false,
                    }));
                }
            }
            FixtureEvent::Dive { key, lamp, object } => {
                // Entering selects the CLICKED object (D4: double-click
                // descends to what you pointed at), else the fixture's
                // first object, else the entered-empty drawing state. The
                // dispatch is the whole entry — the dive derives from the
                // selection's scope. `on_focus` stays the host's
                // dive-capable gate (stories omit it).
                if on_focus.is_some()
                    && let Some(node) = nodes.get(&key)
                {
                    drag_override.set(None);
                    let clicked = match sprite_target(&grammar_surface, *node, object, lamp) {
                        UiPatchTarget::Fixture { .. } => None,
                        target => Some(target),
                    };
                    let first = clicked.or_else(|| {
                        grammar_surface
                            .fixtures
                            .iter()
                            .find(|fixture| fixture.node == *node)
                            .and_then(|fixture| fixture.instances.first())
                            .map(|instance| {
                                crate::app::patch::verb_ui::instance_target(*node, instance)
                            })
                    });
                    let mut next = UiSelection::empty();
                    match first {
                        Some(target) => next.select_one(target),
                        None => next.enter(*node),
                    }
                    dispatch_selection(&on_action, next);
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
    // Committed dive edits latch content-edited too: a moved object's
    // bounds land after the apply echoes, and that arrival must not
    // re-fit either.
    let committed_inner = on_committed.unwrap_or_else(|| EventHandler::new(|()| {}));
    let committed = EventHandler::new(move |()| {
        if !*content_edited.peek() {
            content_edited.set(true);
        }
        committed_inner.call(());
    });
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
                transform_handles,
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

/// Lay the controller's unmapped-chase preview (Q9) over one fixture's
/// sprite colors, in the fixture's own lamp numbering.
///
/// The preview is the SAME data the panel strip paints, converted through
/// the same linear → sRGB transfer a published frame sample takes — so the
/// strip and the sprites cannot disagree, and both advance and freeze on
/// the controller's one frame clock.
///
/// An unmapped object usually has no live colors at all behind it (nothing
/// published its lamps), so the base may be empty: it is grown to the
/// fixture's lamp count with the unlit neutral first, which reads as
/// geometry rather than as black lamps. A fixture the preview does not name
/// is returned untouched.
fn with_chase_preview(
    surface: &UiPatchSurface,
    node: NodeId,
    base: Vec<[u8; 3]>,
    lamps: u32,
) -> Vec<[u8; 3]> {
    let Some(preview) = surface
        .chase_preview
        .as_ref()
        .filter(|preview| preview.node == node)
    else {
        return base;
    };
    let end = preview.start as usize + preview.colors.len();
    let mut colors = base;
    if colors.len() < end.max(lamps as usize) {
        colors.resize(end.max(lamps as usize), UNLIT_RGB);
    }
    for (offset, rgb) in preview.colors.iter().enumerate() {
        if let Some(slot) = colors.get_mut(preview.start as usize + offset) {
            *slot = srgb8(*rgb);
        }
    }
    colors
}

/// The plain-click pick grain, per view policy (unified-selection D4 /
/// walk-up Q10): patch-verb hosts (Patching) resolve the OBJECT under the
/// cursor — the walk-up loop taps physical pieces and "a click says
/// WHICH"; everything else (Mapping, stories) selects the FIXTURE — click
/// selects at scope level, and descent is the double-click's job.
fn plain_click_target(
    pick_objects: bool,
    surface: &UiPatchSurface,
    node: NodeId,
    pick: &lpa_mapping_editor::FixturePick,
) -> UiPatchTarget {
    if pick_objects {
        sprite_target(surface, node, pick.object, pick.lamp)
    } else {
        UiPatchTarget::Fixture { node }
    }
}

/// What a sprite click selects (Q10): the OBJECT owning the clicked lamp,
/// or the whole fixture when nothing finer can be named.
///
/// The lamp arrives in the fixture's OWN numbering (the canvas corrects for
/// display subsampling before it leaves), which is the space instance spans
/// are measured in — so the lookup is a plain containment test. Two honest
/// fallbacks: a body that draws no lamps yields no lamp at all, and a
/// fixture whose document has no object table (the scarf) IS its own object.
///
/// The object target follows addressability like every other surface's
/// (`instance_target`): a sticky id selects by path, an id-less strand by
/// the range its lamps occupy.
fn sprite_target(
    surface: &UiPatchSurface,
    node: NodeId,
    object: Option<usize>,
    lamp: Option<u32>,
) -> UiPatchTarget {
    let fixture = UiPatchTarget::Fixture { node };
    let Some(entry) = surface.fixtures.iter().find(|entry| entry.node == node) else {
        return fixture;
    };
    // The HULL answers first (round 3: the whole body is the target): the
    // pick's index is into the sprite's object list, which is built one
    // entry per surface instance IN ORDER (`sprite_objects`).
    if let Some(index) = object
        && let Some(instance) = entry.instances.get(index)
    {
        return crate::app::patch::verb_ui::instance_target(node, instance);
    }
    let Some(lamp) = lamp else {
        return fixture;
    };
    match entry
        .instances
        .iter()
        .find(|instance| lamp >= instance.start && lamp < instance.start + instance.lamps)
    {
        Some(instance) => crate::app::patch::verb_ui::instance_target(node, instance),
        // Under a display stride the clicked lamp can fall in a gap between
        // spans; the fixture is the honest answer rather than a guess.
        None => fixture,
    }
}

/// Prebuild the `EditorMetaOp::SetMany` factory: `editor.json` artifact +
/// the fixture facts every write refreshes footprints through. `None` =
/// the artifact is unknown (surface not settled), so moves no-op
/// honestly. A single-fixture gesture is simply a set of one — same op,
/// same one undo step.
fn arrange_set_many_dispatch(
    surface: &UiPatchSurface,
) -> impl Fn(Vec<lpa_studio_core::EditorMetaSet>) -> Option<EditorMetaOp> + Clone + 'static {
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
    move |entries| {
        if entries.is_empty() {
            return None;
        }
        Some(EditorMetaOp {
            artifact: artifact.clone()?,
            fixtures: fixtures.clone(),
            verb: EditorMetaVerb::SetMany { entries },
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
fn sprite_of(render: &FixtureRender, selection: &UiSelection) -> FixtureSprite {
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
        objects: sprite_objects(render, selection),
    }
}

/// The fixture's objects as canvas BODIES (G1 round 3: an object is a
/// THING, not a field of dots): one aligned outline per instance, in the
/// sprite's own space, ONE ENTRY PER INSTANCE IN SURFACE ORDER — a
/// `FixturePick::object` is an index into this list, so bodyless instances
/// stay as empty placeholders rather than shifting everyone after them.
///
/// The design-language round replaced the padded convex hull with the band
/// the lamp strand sweeps, aligned as the document authored it, plus voronoi
/// CELLS for the ribbon-like kinds; the hit body stays the symmetric on-path
/// band whatever the visual alignment (planning Q7).
///
/// Computed once per sprite build (selection changes rebuild sprites
/// anyway); the per-object cost is one offset pass plus the cell clipping
/// over its DRAWN lamps, which display subsampling already bounds.
fn sprite_objects(render: &FixtureRender, selection: &UiSelection) -> Vec<SpriteObject> {
    let FixtureBody::Lamps { points, total } = &render.body else {
        return Vec::new();
    };
    if points.is_empty() || render.instances.is_empty() {
        return Vec::new();
    }
    // The same subsample arithmetic the live-fill hooks use: drawn point i
    // is TRUE lamp i × stride.
    let stride = (*total as usize).div_ceil(points.len()).max(1);
    render
        .instances
        .iter()
        .map(|(path, start, lamps)| {
            let end = start.saturating_add(*lamps);
            let (strands, displayed) = instance_strands(render, points, stride, *start, end);
            // The band's reach off the lamps, derived from THIS instance's
            // own numbers — doc units are arbitrary (G1 ruling), so nothing
            // absolute may appear here: a fraction of the strand pitch,
            // floored by the doc's declared lamp footprint. ONE value — hit
            // body, visual outline and cells all stand off the lamps by the
            // same amount, so they agree by construction.
            let reach = instance_pitch(&strands)
                .map_or(0.0, |pitch| 0.65 * pitch)
                .max(0.55 * f64::from(render.sample_diameter))
                .max(f64::EPSILON) as f32;
            // How this instance draws: from the first strand of the document
            // inside its window (an instance never spans two objects, so one
            // answer covers it). Unknown — a body with no document behind it
            // — draws the neutral symmetric band with dots.
            let meta = render.strands.iter().find(|meta| meta.within(*start, end));
            let align = meta.map_or(PathAlign::On, |meta| meta.align);
            // A closed shape's outline wraps: repeat the first lamp so the
            // band is an annulus, not a ribbon with a mouth at the seam. The
            // CELLS stay on the open run — one cell per drawn lamp, always.
            let outline_strands: Vec<Vec<[f32; 2]>> = match meta {
                Some(meta) if meta.closed => strands
                    .iter()
                    .map(|strand| {
                        let mut closed = strand.clone();
                        closed.extend(strand.first().copied());
                        closed
                    })
                    .collect(),
                _ => strands.clone(),
            };
            let cells = match meta {
                Some(meta) if meta.cells => {
                    object_cells(&strands, align, &displayed, render.sample_diameter)
                }
                _ => Vec::new(),
            };
            let label = if path.is_empty() {
                format!("lamps {start}\u{2013}{}", end.saturating_sub(1))
            } else {
                path.trim_start_matches('/').to_string()
            };
            SpriteObject {
                label,
                hull: hit_body(&outline_strands, reach),
                outline: aligned_outline(&outline_strands, align, reach),
                cells,
                lamps: (*start, *lamps),
                selected: object_selected(selection, render.node, path, *start, *lamps),
            }
        })
        .collect()
}

/// One instance's DRAWN lamps, cut into physical strands: a new run wherever
/// the owning [`StrandMeta`] changes (a repeat's next instance, the far side
/// of a jumper), so no body ever bridges a break in the wire.
///
/// Returns the runs and, alongside them, the sprite-displayed index of every
/// point in run order — the mapping the cells' live-fill hooks stride
/// through.
fn instance_strands(
    render: &FixtureRender,
    points: &[[f32; 2]],
    stride: usize,
    start: u32,
    end: u32,
) -> (Vec<Vec<[f32; 2]>>, Vec<usize>) {
    let mut strands: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut displayed: Vec<usize> = Vec::new();
    let mut run: Option<Option<usize>> = None;
    for (index, point) in points.iter().enumerate() {
        let lamp = (index * stride) as u32;
        if lamp < start || lamp >= end {
            continue;
        }
        let owner = render.strands.iter().position(|meta| meta.owns(lamp));
        if run != Some(owner) {
            strands.push(Vec::new());
            run = Some(owner);
        }
        if let Some(strand) = strands.last_mut() {
            strand.push(*point);
        }
        displayed.push(index);
    }
    (strands, displayed)
}

/// The instance's cells, re-indexed from its own lamp order onto the
/// SPRITE's displayed points — the index the canvas multiplies by the
/// display stride to name a true lamp, exactly as the circles do. Cells
/// clipped away to nothing carry no polygon and are dropped here rather than
/// emitting an empty path element.
fn object_cells(
    strands: &[Vec<[f32; 2]>],
    align: PathAlign,
    displayed: &[usize],
    sample_diameter: f32,
) -> Vec<LampCell> {
    lamp_cells(strands, align, sample_diameter)
        .into_iter()
        .filter(|cell| cell.polygon.len() >= 3)
        .filter_map(|cell| {
            displayed.get(cell.lamp).map(|index| LampCell {
                lamp: *index,
                polygon: cell.polygon,
            })
        })
        .collect()
}

/// Median of the consecutive drawn-point gaps across an instance's strands
/// — its pitch as displayed. `None` when no strand has two distinct points.
fn instance_pitch(strands: &[Vec<[f32; 2]>]) -> Option<f64> {
    let mut gaps: Vec<f64> = strands
        .iter()
        .flat_map(|strand| {
            strand.windows(2).map(|w| {
                let (dx, dy) = (f64::from(w[1][0] - w[0][0]), f64::from(w[1][1] - w[0][1]));
                (dx * dx + dy * dy).sqrt()
            })
        })
        .filter(|gap| *gap > 1e-9)
        .collect();
    if gaps.is_empty() {
        return None;
    }
    gaps.sort_by(f64::total_cmp);
    Some(gaps[gaps.len() / 2])
}

/// Every strand of a resolved document, in wiring order — the drawing facts
/// the canvas cannot recover from lamp positions alone.
fn strand_metas(doc: &Map2dDoc, resolved: &ResolvedMap2d) -> Vec<StrandMeta> {
    let mut metas = Vec::with_capacity(resolved.spans.len());
    for span in &resolved.spans {
        let Some(object) = doc.objects.get(span.object as usize) else {
            continue;
        };
        // A repeat rotates an inner shape: it is the innermost LEAF that says
        // how the lamps run.
        let mut shape = &object.shape;
        while let Map2dShape::Repeat(repeat) = shape {
            shape = &repeat.shape;
        }
        let (align, cells, closed, breaks) = match shape {
            Map2dShape::Path(path) => (path.align, true, false, lpc_mapping::path_gap_breaks(path)),
            Map2dShape::Polygon(polygon) => (polygon.align, true, true, Vec::new()),
            // Grid and ring lamps are a FIELD, not a ribbon: they wear the
            // neutral on-path band and keep their dots.
            _ => (PathAlign::On, false, false, Vec::new()),
        };
        // Cut the span at each jumper: one meta per lit run.
        let mut cursor = 0;
        for offset in breaks.into_iter().chain([span.count]) {
            if offset <= cursor || offset > span.count {
                continue;
            }
            metas.push(StrandMeta {
                start: span.start + cursor,
                count: offset - cursor,
                align,
                cells,
                closed,
            });
            cursor = offset;
        }
    }
    metas
}

/// Is THIS object the selection? Path-addressed instances match by path;
/// id-less rows select at range grain (`instance_target`), so the window
/// is the identity there.
fn object_selected(
    selection: &UiSelection,
    node: NodeId,
    path: &str,
    start: u32,
    lamps: u32,
) -> bool {
    selection.targets().iter().any(|target| match target {
        UiPatchTarget::Instance { node: n, path: p } => *n == node && !path.is_empty() && p == path,
        UiPatchTarget::Range {
            node: n,
            start: s,
            count,
        } => *n == node && *s == start && *count == Some(lamps),
        _ => false,
    })
}

/// Does the selection concern this fixture (any grain, any member)?
fn selection_touches(selection: &UiSelection, node: NodeId) -> bool {
    selection.targets().iter().any(|target| match target {
        UiPatchTarget::Fixture { node: n }
        | UiPatchTarget::Instance { node: n, .. }
        | UiPatchTarget::Range { node: n, .. } => *n == node,
        _ => false,
    })
}

/// The selected instance's lamp window on this fixture, when one is.
fn selected_instance_range(selection: &UiSelection, render: &FixtureRender) -> Option<(u32, u32)> {
    // One window per sprite (the first member's): the per-object rings
    // already mark every selected hull, so the range highlight stays a
    // single-window affordance.
    selection.targets().iter().find_map(|target| match target {
        UiPatchTarget::Instance { node, path } if *node == render.node => render
            .instances
            .iter()
            .find(|(p, _, _)| p == path)
            .map(|(_, start, lamps)| (*start, *lamps)),
        UiPatchTarget::Range { node, start, count } if *node == render.node => {
            Some((*start, count.unwrap_or(u32::MAX)))
        }
        _ => None,
    })
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
/// dropped — which is exactly why a slot may only be ADOPTED from settled
/// facts: the arrangement document must have answered (before that,
/// arranged-ness is unknown) and the fixture's bounds must be real.
pub(crate) fn refresh_pack_slots(
    surface: &UiPatchSurface,
    bodies: &BTreeMap<ArtifactLocation, String>,
    held: &PackSlots,
) -> Option<PackSlots> {
    if !surface.editor_meta_loaded {
        return None;
    }
    let renders = build_renders(surface, bodies, held);
    merge_pack_slots(&renders, held)
}

/// The pure half of [`refresh_pack_slots`]: adopt the auto-packed
/// transform of every unarranged fixture without a held slot.
fn merge_pack_slots(renders: &[FixtureRender], held: &PackSlots) -> Option<PackSlots> {
    let fresh: Vec<&FixtureRender> = renders
        .iter()
        .filter(|render| render.settled && !render.arranged && !held.contains_key(&render.key))
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
                // The strand facts ride out of the SAME parse: the canvas
                // needs the authored alignment and the physical breaks to
                // draw a body, and nothing downstream may read the document
                // a second time to get them.
                let strands = strand_metas(&doc, &resolved);
                Some((resolved, strands, doc.sample_diameter))
            });
        let (body, bounds, strands, sample_diameter, settled) = match resolved {
            Some((resolved, strands, sample_diameter)) => {
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
                (
                    FixtureBody::Lamps { points, total },
                    bounds,
                    strands,
                    sample_diameter,
                    true,
                )
            }
            None if fixture.mapping_artifact.is_some() => {
                // A map2d exists but is not loaded: the footprint block.
                // Settled only when a cached footprint backs the bounds —
                // a guessed block must never seed a held pack slot.
                let lamps = fixture.patch.lamps;
                let footprint = arrange.footprint.map(|fp| fp.bbox);
                let settled = footprint.is_some();
                let bounds = footprint.unwrap_or_else(|| placeholder_bounds(lamps));
                (
                    FixtureBody::Placeholder { lamps },
                    bounds,
                    Vec::new(),
                    lpc_mapping::DEFAULT_SAMPLE_DIAMETER,
                    settled,
                )
            }
            None => {
                // The peach: no map2d document at all — the range strip.
                let lamps = fixture.patch.lamps;
                let width = f64::from(lamps.max(8)) * 3.0;
                (
                    FixtureBody::Strip { lamps },
                    [0.0, 0.0, width, 10.0],
                    Vec::new(),
                    lpc_mapping::DEFAULT_SAMPLE_DIAMETER,
                    true,
                )
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
            settled,
            instances: fixture
                .instances
                .iter()
                .map(|instance| (instance.path.clone(), instance.start, instance.lamps))
                .collect(),
            strands,
            sample_diameter,
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
            settled: true,
            instances: Vec::new(),
            strands: Vec::new(),
            sample_diameter: lpc_mapping::DEFAULT_SAMPLE_DIAMETER,
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

    /// A guessed placeholder must never seed a HELD slot (G1 round 1):
    /// the slot would bake bounds that move when the body arrives, and
    /// two views packing at different load times then disagree about the
    /// same fixture. Unsettled renders keep packing ad hoc — visible, but
    /// never adopted.
    #[test]
    fn unsettled_bounds_never_become_held_slots() {
        let mut unsettled = render("b", false, [0.0, 0.0, 24.0, 16.0]);
        unsettled.settled = false;
        assert_eq!(
            merge_pack_slots(&[unsettled.clone()], &PackSlots::new()),
            None,
            "no slot adopted from guessed bounds"
        );
        let settled = render("b", false, [0.0, 0.0, 24.0, 16.0]);
        assert!(
            merge_pack_slots(&[settled], &PackSlots::new()).is_some(),
            "the same fixture with real bounds adopts its slot"
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
        .map(|r| sprite_of(r, &UiSelection::empty()))
        .collect();
        let bounds = fit_bounds(&sprites).expect("bounds");
        assert!(bounds.min_x <= 0.0 && bounds.min_y <= 0.0);
        assert!(bounds.min_x + bounds.width >= 90.0, "{bounds:?}");
    }

    /// A three-instance fixture with nothing on a wire — the shape a
    /// walk-up user clicks at.
    fn dome_surface() -> UiPatchSurface {
        use lpa_studio_core::{UiFixturePatch, UiPatchInstance, UiPatchSurfaceFixture};

        let instance = |path: &str, start: u32| UiPatchInstance {
            path: path.to_string(),
            label: path.to_string(),
            start,
            lamps: 30,
            stride: 1,
            placed: false,
        };
        UiPatchSurface {
            fixtures: vec![UiPatchSurfaceFixture {
                node: NodeId::new(2),
                label: "dome".to_string(),
                manual_flow: true,
                patch: UiFixturePatch {
                    lamps: 90,
                    ..Default::default()
                },
                instances: vec![
                    instance("/sector/0", 0),
                    instance("/sector/1", 30),
                    instance("/sector/2", 60),
                ],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// The pick grain is VIEW POLICY (the one grammar, two activities):
    /// Patching (`patch_verbs`) resolves the object under the cursor —
    /// Q10's "a click says WHICH" — while Mapping selects the fixture,
    /// because at root scope a click selects at scope level (D4) and
    /// descent belongs to the double-click.
    #[test]
    fn plain_click_grain_follows_the_view() {
        let surface = dome_surface();
        let node = NodeId::new(2);
        let pick = lpa_mapping_editor::FixturePick {
            key: "k".into(),
            lamp: Some(35),
            object: None,
        };
        assert_eq!(
            plain_click_target(true, &surface, node, &pick),
            UiPatchTarget::Instance {
                node,
                path: "/sector/1".to_string(),
            },
            "Patching: the object under the cursor"
        );
        assert_eq!(
            plain_click_target(false, &surface, node, &pick),
            UiPatchTarget::Fixture { node },
            "Mapping: click selects at scope level"
        );
    }

    /// Q10: a sprite click names the OBJECT the clicked lamp belongs to.
    /// The fixture is the fallback, not the answer — it is what a click can
    /// still mean when the sprite draws no lamps or no span covers the one
    /// it hit.
    #[test]
    fn a_sprite_click_resolves_its_lamp_to_an_object() {
        let surface = dome_surface();
        let node = NodeId::new(2);

        assert_eq!(
            sprite_target(&surface, node, None, Some(0)),
            UiPatchTarget::Instance {
                node,
                path: "/sector/0".to_string(),
            }
        );
        assert_eq!(
            sprite_target(&surface, node, None, Some(59)),
            UiPatchTarget::Instance {
                node,
                path: "/sector/1".to_string(),
            },
            "the last lamp of a span is still that span's"
        );
        assert_eq!(
            sprite_target(&surface, node, None, None),
            UiPatchTarget::Fixture { node },
            "a placeholder or strip body names no lamp — the fixture answers"
        );
        assert_eq!(
            sprite_target(&surface, node, None, Some(900)),
            UiPatchTarget::Fixture { node },
            "a lamp no span covers falls back rather than guessing"
        );
        assert_eq!(
            sprite_target(&surface, NodeId::new(99), None, Some(0)),
            UiPatchTarget::Fixture {
                node: NodeId::new(99),
            },
            "an unknown fixture resolves nothing finer than itself"
        );
    }

    /// An id-less document still selects — at RANGE grain, the same target
    /// its tree row builds, so the arm's two ends agree about what was
    /// clicked (the grain-robustness invariant).
    #[test]
    fn a_sprite_click_on_an_idless_document_selects_a_range() {
        let mut surface = dome_surface();
        for instance in &mut surface.fixtures[0].instances {
            instance.path.clear();
        }
        assert_eq!(
            sprite_target(&surface, NodeId::new(2), None, Some(35)),
            UiPatchTarget::Range {
                node: NodeId::new(2),
                start: 30,
                count: Some(30),
            }
        );
    }

    /// Q9's canvas half: the sprites paint the CONTROLLER's chase, at the
    /// controller's phase — the very colors the panel strip is showing.
    #[test]
    fn the_sprite_feed_paints_the_core_computed_chase() {
        use lpa_studio_core::UiPatchChasePreview;

        let mut surface = dome_surface();
        let node = NodeId::new(2);
        // No preview: the sprites are whatever the wire published (here,
        // nothing) — the canvas invents no chase of its own.
        assert!(with_chase_preview(&surface, node, Vec::new(), 90).is_empty());

        surface.chase_preview = Some(UiPatchChasePreview {
            node,
            start: 30,
            colors: vec![[65535, 0, 0]; 30],
            phase: 0.25,
        });
        let colors = with_chase_preview(&surface, node, Vec::new(), 90);
        assert_eq!(colors.len(), 90, "the fixture's whole lamp field");
        assert_eq!(colors[0], UNLIT_RGB, "lamps outside the object stay unlit");
        assert_eq!(colors[30], srgb8([65535, 0, 0]));
        assert_eq!(colors[59], srgb8([65535, 0, 0]));
        assert_eq!(colors[60], UNLIT_RGB);

        // A live base keeps its own lamps and takes the chase over the
        // object's — one fixture, two honest sources.
        let base = vec![[9, 9, 9]; 90];
        let colors = with_chase_preview(&surface, node, base, 90);
        assert_eq!(colors[0], [9, 9, 9]);
        assert_eq!(colors[30], srgb8([65535, 0, 0]));

        // Another fixture's sprites are untouched.
        assert!(with_chase_preview(&surface, NodeId::new(7), Vec::new(), 90).is_empty());
    }

    /// A fixture with NO run publishes no frame, so its live colors are
    /// empty — and keeping the last good ones would leave the light it used
    /// to make painted on lamps that are now dark. Unmapping must actually
    /// go dark (and then only the SELECTED object chases, from the
    /// controller). The keep-last-good exists for apply gaps on a fixture
    /// that still has a run, and that case still keeps.
    #[test]
    fn keep_last_good_does_not_outlive_a_fixtures_runs() {
        use lpa_studio_core::UiPatchCell;

        let surface = dome_surface();
        let fixture = &surface.fixtures[0];
        assert!(fixture.patch.cells.is_empty(), "nothing on a wire");
        assert!(
            fixture_live_colors(&surface, fixture).is_empty(),
            "and so no frame, and so no colors to feed"
        );

        let mut mapped = dome_surface();
        mapped.fixtures[0].patch.cells.push(UiPatchCell {
            id: "2:0".to_string(),
            source_start: 0,
            lamps: 30,
            wire_start: 0,
            ..Default::default()
        });
        assert!(
            !mapped.fixtures[0].patch.cells.is_empty(),
            "a fixture that still has a run keeps its last good frame across \
             an apply gap — the condition the feed guards on",
        );
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

    /// A path strand of `count` lamps from `start`, drawn like a document
    /// path (cells, symmetric band).
    fn path_strand(start: u32, count: u32) -> StrandMeta {
        StrandMeta {
            start,
            count,
            align: PathAlign::On,
            cells: true,
            closed: false,
        }
    }

    fn lamp_render(points: Vec<[f32; 2]>, total: u32) -> FixtureRender {
        FixtureRender {
            node: NodeId::new(7),
            key: "k".into(),
            label: "fx".into(),
            color: "#fff",
            transform: UiArrangeTransform::default(),
            arranged: true,
            bounds: [0.0, 0.0, 100.0, 10.0],
            body: FixtureBody::Lamps { points, total },
            settled: true,
            instances: Vec::new(),
            strands: Vec::new(),
            sample_diameter: lpc_mapping::DEFAULT_SAMPLE_DIAMETER,
        }
    }

    /// Objects become canvas BODIES (round 3): one body per instance, IN
    /// SURFACE ORDER — a pick's index must line up — and a degenerate span
    /// stays as an empty placeholder rather than shifting its neighbours.
    #[test]
    fn sprite_objects_are_one_hull_per_instance_in_order() {
        let mut render = lamp_render((0..20).map(|i| [i as f32 * 5.0, 0.0]).collect(), 20);
        render.instances = vec![
            ("/a/0".into(), 0, 10),
            ("/a/1".into(), 10, 10),
            ("/gap".into(), 40, 5), // past the drawn points: no body
        ];
        render.strands = vec![path_strand(0, 10), path_strand(10, 10)];
        let objects = sprite_objects(&render, &UiSelection::empty());
        assert_eq!(objects.len(), 3, "one entry per instance, always");
        assert!(!objects[0].hull.is_empty(), "a real span grows a body");
        assert!(!objects[0].outline.is_empty(), "…and something to paint");
        assert!(!objects[1].hull.is_empty());
        assert!(
            objects[2].hull.is_empty() && objects[2].outline.is_empty(),
            "no lamps drawn in the span = no body, but the SLOT remains"
        );
        assert!(objects[2].cells.is_empty());
        assert_eq!(objects[1].lamps, (10, 10), "true numbering rides along");
    }

    /// The cells a path object wears are indexed in the SPRITE's displayed
    /// space, so the canvas's `index × stride` names the same true lamp the
    /// circles would have named — the live-fill feed's whole contract.
    #[test]
    fn cells_are_indexed_in_the_sprites_displayed_space() {
        // 40 true lamps drawn as 20 points: stride 2.
        let mut render = lamp_render((0..20).map(|i| [i as f32 * 5.0, 0.0]).collect(), 40);
        render.instances = vec![("/a/0".into(), 0, 20), ("/a/1".into(), 20, 20)];
        render.strands = vec![path_strand(0, 20), path_strand(20, 20)];
        let objects = sprite_objects(&render, &UiSelection::empty());
        assert_eq!(objects[0].cells.len(), 10, "one cell per DRAWN lamp");
        let first: Vec<usize> = objects[0].cells.iter().map(|cell| cell.lamp).collect();
        assert_eq!(first, (0..10).collect::<Vec<_>>());
        let second: Vec<usize> = objects[1].cells.iter().map(|cell| cell.lamp).collect();
        assert_eq!(
            second,
            (10..20).collect::<Vec<_>>(),
            "the second instance continues the sprite's own indexing"
        );
    }

    /// A physical break inside one instance (a repeat's next strand, the far
    /// side of a jumper) splits the body: two loops, and nothing bridging
    /// the gap between them.
    #[test]
    fn a_strand_break_inside_an_instance_splits_the_body() {
        let points: Vec<[f32; 2]> = (0..10)
            .map(|i| {
                if i < 5 {
                    [i as f32, 0.0]
                } else {
                    [i as f32 + 40.0, 0.0]
                }
            })
            .collect();
        let mut render = lamp_render(points, 10);
        render.instances = vec![("/a".into(), 0, 10)];
        render.strands = vec![path_strand(0, 5), path_strand(5, 5)];
        let objects = sprite_objects(&render, &UiSelection::empty());
        assert_eq!(objects[0].outline.len(), 2, "one loop per lit run");
        assert!(
            !lpa_mapping_editor::point_in_loops(&objects[0].hull, [25.0, 0.0]),
            "the jumper stays open — no body bridges it"
        );
        assert!(lpa_mapping_editor::point_in_loops(
            &objects[0].hull,
            [2.0, 0.0]
        ));
    }

    /// Grid and ring lamps are a FIELD: they get the neutral on-path band
    /// and keep their dots, so the canvas draws no cells for them.
    #[test]
    fn field_kinds_get_a_body_but_no_cells() {
        let mut render = lamp_render((0..10).map(|i| [i as f32 * 5.0, 0.0]).collect(), 10);
        render.instances = vec![("/g".into(), 0, 10)];
        render.strands = vec![StrandMeta {
            start: 0,
            count: 10,
            align: PathAlign::On,
            cells: false,
            closed: false,
        }];
        let objects = sprite_objects(&render, &UiSelection::empty());
        assert!(!objects[0].outline.is_empty());
        assert!(objects[0].cells.is_empty(), "dots stay dots");
    }

    /// The whole seam on REAL bytes: the mini-dome's own document through
    /// resolve → strand facts → sprite bodies. Its sector is a repeat of a
    /// jumpered path, so each instance must come out as one body of eight
    /// lit runs wearing a cell per drawn lamp — and nothing painted across a
    /// jumper.
    #[test]
    fn the_mini_dome_sectors_draw_as_jumpered_runs_of_cells() {
        let example = lpa_studio_core::app::home::embedded_example("examples/mini-dome")
            .expect("the mini-dome example is embedded");
        let text = example
            .files
            .iter()
            .find(|(file, _)| *file == "dome/dome.map2d.json")
            .map(|(_, bytes)| std::str::from_utf8(bytes).expect("utf8"))
            .expect("the dome document");
        let doc = Map2dDoc::from_json(text).expect("the dome parses");
        let resolved = lpc_mapping::resolve(&doc).expect("the dome resolves");
        let points: Vec<[f32; 2]> = resolved.lamps.iter().map(|lamp| lamp.pos).collect();
        let total = points.len() as u32;
        let mut render = lamp_render(points.clone(), total);
        let bounds = lpc_mapping::bounds_of_points(&points).expect("bounds");
        render.bounds = [
            f64::from(bounds.min_x),
            f64::from(bounds.min_y),
            f64::from(bounds.width),
            f64::from(bounds.height),
        ];
        render.strands = strand_metas(&doc, &resolved);
        render.instances = (0..5)
            .map(|instance| (format!("/sector/{instance}"), instance * 30, 30))
            .collect();

        let objects = sprite_objects(&render, &UiSelection::empty());
        assert_eq!(objects.len(), 5, "one body per sector instance");
        for (index, object) in objects.iter().enumerate() {
            assert_eq!(
                object.outline.len(),
                8,
                "sector {index}: the path's seven jumpers leave eight lit runs"
            );
            assert_eq!(object.cells.len(), 30, "sector {index}: a cell per lamp");
            // The cells name this instance's lamps, in the sprite's own
            // displayed indexing (stride 1 here — 150 lamps, all drawn).
            let lamps: Vec<usize> = object.cells.iter().map(|cell| cell.lamp).collect();
            assert_eq!(lamps, (index * 30..index * 30 + 30).collect::<Vec<_>>());
            // Every lamp of the sector lands inside its own hit body.
            for lamp in index * 30..index * 30 + 30 {
                assert!(
                    lpa_mapping_editor::point_in_loops(&object.hull, points[lamp]),
                    "sector {index} lamp {lamp} fell outside its own body"
                );
            }
        }
    }

    /// The drawing facts reach the sprite build from the ONE parse the
    /// resolver already did: alignment per object, cells only for the ribbon
    /// kinds, a repeat's instances as separate strands, and a jumper cutting
    /// its path's span in two.
    #[test]
    fn strand_metas_carry_alignment_kind_and_the_physical_breaks() {
        use lpc_mapping::{Map2dObject, PathShape, PolygonShape, RepeatShape, RingShape};

        let object = |shape| Map2dObject {
            name: String::new(),
            id: None,
            stride: None,
            shape,
        };
        let doc = Map2dDoc {
            objects: vec![
                object(Map2dShape::Path(PathShape {
                    points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [20.0, 10.0]],
                    count: 5,
                    reversed: false,
                    gaps: vec![1],
                    align: PathAlign::Inside,
                })),
                object(Map2dShape::Repeat(RepeatShape {
                    shape: Box::new(Map2dShape::Polygon(PolygonShape {
                        points: vec![[0.0, 0.0], [10.0, 0.0], [5.0, 8.0]],
                        count: 3,
                        align: PathAlign::Outside,
                    })),
                    center: [0.0, 0.0],
                    count: 2,
                })),
                object(Map2dShape::Ring(RingShape {
                    center: [0.0, 0.0],
                    radius: 10.0,
                    outer_count: 4,
                    rings: 1,
                    counts: Vec::new(),
                    order: Default::default(),
                    start_angle_deg: -90.0,
                    dir: Default::default(),
                })),
            ],
            ..Map2dDoc::new()
        };
        let resolved = lpc_mapping::resolve(&doc).expect("the document resolves");
        let metas = strand_metas(&doc, &resolved);
        // The gapped path is TWO runs of its one span; the repeat is one
        // strand per instance; the ring is one field.
        let shape: Vec<(u32, u32)> = metas.iter().map(|meta| (meta.start, meta.count)).collect();
        assert_eq!(shape, vec![(0, 3), (3, 2), (5, 3), (8, 3), (11, 4)]);
        assert!(metas[0].align == PathAlign::Inside && metas[0].cells);
        assert_eq!(metas[1].align, PathAlign::Inside, "both sides of a jumper");
        assert!(metas[2].closed, "a polygon's perimeter wraps");
        assert_eq!(metas[3].align, PathAlign::Outside, "through the repeat");
        assert!(!metas[4].cells, "a ring keeps its dots");
        assert_eq!(metas[4].align, PathAlign::On);
    }

    /// A pick's hull index resolves to THAT instance; the nearest lamp
    /// stays the fallback where no hull claimed the point.
    #[test]
    fn sprite_target_prefers_the_picked_hull() {
        let node = NodeId::new(7);
        let mut surface = UiPatchSurface::default();
        surface
            .fixtures
            .push(lpa_studio_core::UiPatchSurfaceFixture {
                node,
                instances: vec![
                    lpa_studio_core::UiPatchInstance {
                        path: "/a/0".into(),
                        label: "a 0".into(),
                        start: 0,
                        lamps: 10,
                        ..Default::default()
                    },
                    lpa_studio_core::UiPatchInstance {
                        path: "/a/1".into(),
                        label: "a 1".into(),
                        start: 10,
                        lamps: 10,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            });
        assert_eq!(
            sprite_target(&surface, node, Some(1), Some(2)),
            UiPatchTarget::Instance {
                node,
                path: "/a/1".into()
            },
            "the hull the press landed in wins over the nearest lamp"
        );
        assert_eq!(
            sprite_target(&surface, node, None, Some(2)),
            UiPatchTarget::Instance {
                node,
                path: "/a/0".into()
            },
            "no hull = the lamp's owner, exactly as before"
        );
        assert_eq!(
            sprite_target(&surface, node, None, None),
            UiPatchTarget::Fixture { node },
        );
    }
}
