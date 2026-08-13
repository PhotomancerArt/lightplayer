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
    Camera, CanvasDrag, EditorCanvas, EditorViewOptions, FixtureBody, FixtureEvent, FixtureSprite,
    MapEditorSession, Placement, object_color,
};
use lpa_studio_core::{
    ArtifactLocation, EditorMetaFixture, EditorMetaOp, EditorMetaVerb, NodeId, ProjectController,
    ProjectEditorOp, UiAction, UiArrangeTransform, UiPatchSurface, UiPatchTarget,
};
use lpc_mapping::Bounds2d;

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

/// The fixture-view host: mounts the ONE crate canvas over sprites built
/// from the surface, owns the project camera (fit-all seed, then frozen —
/// arranging never moves the camera), and runs the override lifecycle.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ArrangeCanvasHost(
    surface: UiPatchSurface,
    /// map2d bodies by artifact (extracted from the snapshot's node views;
    /// stories inject embedded-example bytes directly).
    bodies: BTreeMap<ArtifactLocation, String>,
    selection: Option<UiPatchTarget>,
    /// Sticky auto-pack slots (shell-owned; see [`PackSlots`]). Stories
    /// omit it and get the ad-hoc packing.
    #[props(default)]
    pack: PackSlots,
    /// Double-click on a fixture: the shell dives into it for mapping
    /// edits. Absent = the canvas is arrange-only (stories).
    #[props(default)]
    on_focus: Option<EventHandler<NodeId>>,
    on_action: EventHandler<UiAction>,
) -> Element {
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
    // then FROZEN — arranging a fixture must never move the camera (the
    // gate's drift bug). The fit button re-frames on demand.
    let camera = use_signal(Camera::new);
    let viewport = use_signal(|| None::<[f32; 2]>);
    let mut fit_pending = use_signal(|| true);
    // The canvas's session-shaped props idle in fixture view: no fixture
    // is dived here (P3 parity checkpoint — the dive still runs the old
    // MappingSessionHost path).
    let arrange_session = use_signal(|| MapEditorSession::new(lpc_mapping::Map2dDoc::new()));
    let arrange_view = use_signal(EditorViewOptions::default);
    let arrange_drag = use_signal(|| None::<CanvasDrag>);
    let arrange_live = use_signal(Vec::<[u8; 3]>::new);
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

    // Fit-all runs as an effect once a real viewport measurement AND real
    // bounds exist, then arms only on demand (the ⤢ fit button).
    {
        let renders = renders;
        let mut camera = camera;
        let mut fit_pending_effect = fit_pending;
        use_effect(move || {
            let viewport_now = viewport();
            if fit_pending_effect()
                && let Some([width, height]) = viewport_now
                && let Some(bounds) = fit_bounds(&renders.read().0)
            {
                camera.write().fit(bounds, width, height, 0.0);
                fit_pending_effect.set(false);
            }
        });
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
        move |event: FixtureEvent| match event {
            FixtureEvent::Select(Some(key)) => {
                if let Some(node) = nodes.get(&key) {
                    drag_override.set(None);
                    select(Some(UiPatchTarget::Fixture { node: *node }));
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

    rsx! {
        div { class: "lpme-canvas-wrap",
            button {
                class: "tw:absolute tw:right-2 tw:top-2 tw:z-10 tw:cursor-pointer tw:rounded tw:border tw:border-border-strong tw:bg-card-subtle tw:px-1.5 tw:py-0.5 tw:font-mono tw:text-[10.5px] tw:text-subtle-foreground tw:hover:text-strong-foreground",
                title: "Fit everything in view",
                onclick: move |_| fit_pending.set(true),
                "⤢ fit"
            }
            EditorCanvas {
                session: arrange_session,
                camera,
                view_opts: arrange_view,
                viewport,
                drag: arrange_drag,
                live_feed: arrange_live,
                on_committed: move |()| {},
                fixtures: sprites,
                focused: None::<String>,
                on_fixture,
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

/// The dive's context layer (the P3 parity checkpoint still runs the old
/// MappingSessionHost dive): every OTHER fixture's display points carried
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
    let focus_placement = placement_of(&focus.transform);
    renders
        .iter()
        .filter(|render| render.node != focused)
        .filter_map(|render| {
            let FixtureBody::Lamps { points, .. } = &render.body else {
                return None;
            };
            let placement = placement_of(&render.transform);
            let points = points
                .iter()
                .map(|point| {
                    let world = placement.apply([f64::from(point[0]), f64::from(point[1])]);
                    let local = focus_placement.inverse(world);
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
