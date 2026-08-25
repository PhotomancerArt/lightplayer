//! The fixture layer: the project's fixtures as arranged sprites — frames,
//! name tags, honest bodies, selection rings — rendered in PROJECT space
//! between the dot grid and the placed doc layers.
//!
//! Sprites are plain data (the shell builds them from its surface; this
//! crate stays project-unaware), and the layer is purely visual: the
//! canvas-level hit test owns every fixture gesture (SVG child delegation is
//! unreliable under Dioxus's event delegation, 07a39242f). The one exception
//! is the object HULLS, which take `pointer-events` back so the browser can
//! draw their `:hover` state for free — they still carry no handlers, and
//! their events bubble to the canvas root like any other.
//!
//! Objects are THINGS here (G1 round 3): each one draws a padded convex hull
//! of its own lamps — faint at rest, lifted on hover, accent-stroked when
//! selected — and the WHOLE hull is the click target, which is what makes a
//! 3 px lamp on a dome reachable at all.

use dioxus::prelude::*;

use super::hull::{hull_path_d, point_in_polygon};
use crate::editor_core::placement::Placement;

/// Padding around a fixture's own-space bounds for hit-testing, in doc
/// units (the frame is drawn with a similar visual pad).
const HIT_PAD: f64 = 8.0;

/// Movement threshold in CSS pixels before a fixture press becomes a drag —
/// under it, pointer-up is a SELECT, and no stray write ever happens.
pub(crate) const DRAG_THRESHOLD_PX: f64 = 4.0;

/// One fixture's render-ready facts. The shell builds these from its patch
/// surface (bodies display-subsampled, overrides already applied); the
/// canvas renders and hit-tests them as data.
#[derive(Clone, Debug, PartialEq)]
pub struct FixtureSprite {
    /// Stable identity (the shell's editor key); event payloads carry it.
    pub key: String,
    pub label: String,
    /// Object colour (CSS).
    pub color: String,
    /// Where the fixture sits in project space (effective: overrides in).
    pub placement: Placement,
    /// Own-space bounds `[x, y, w, h]` the frame is drawn around.
    pub bounds: [f64; 4],
    pub body: FixtureBody,
    /// False renders the dashed "not yet arranged" frame.
    pub arranged: bool,
    pub selected: bool,
    /// Selected instance's `(start, lamps)` window, for lamp rings. Only
    /// drawn when the selection has no HULL to outline instead — the hull
    /// IS the selection indicator wherever one exists (G1 round 3).
    pub selected_range: Option<(u32, u32)>,
    /// This fixture's objects as clickable bodies, in the shell's own order
    /// (a pick reports the INDEX into this list). Empty for a fixture whose
    /// document names no objects, or whose body draws no lamps to hull.
    pub objects: Vec<SpriteObject>,
}

/// One object of a fixture, as the canvas draws and hit-tests it.
///
/// The crate stays project-unaware: an object here is a closed outline, a
/// name for the tooltip, and a selected flag. What it MEANS — an instance
/// path, a lamp range — is the shell's business, recovered from the index
/// this sprite lists it at.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteObject {
    /// Human name, for the hover title only.
    pub label: String,
    /// The padded hull polygon in the sprite's OWN space, closed implicitly.
    /// Fewer than three points means "no body" — nothing is drawn and
    /// nothing hits.
    pub hull: Vec<[f32; 2]>,
    /// `(first lamp, count)` in the fixture's TRUE numbering — the same
    /// space [`nearest_lamp`] answers in, so overlapping hulls can be broken
    /// apart by the lamp the pointer is actually closest to.
    pub lamps: (u32, u32),
    pub selected: bool,
}

impl SpriteObject {
    /// Does this object own `lamp` (fixture numbering)?
    fn owns(&self, lamp: u32) -> bool {
        let (start, count) = self.lamps;
        lamp >= start && lamp < start.saturating_add(count)
    }
}

/// The three honest fixture bodies (the arrange canvas's vocabulary).
#[derive(Clone, Debug, PartialEq)]
pub enum FixtureBody {
    /// Doc-space lamp points (possibly display-subsampled) + the true
    /// total, so instance rings stay subsample-aware.
    Lamps { points: Vec<[f32; 2]>, total: u32 },
    /// Footprint block: the body is not loaded yet.
    Placeholder { lamps: u32 },
    /// The shape-less strip: a range bar with lamp dots.
    Strip { lamps: u32 },
}

/// What a fixture gesture asks of the shell. The crate keeps only
/// ephemeral pointer state; the shell owns the override lifecycle and
/// feeds effective placements back through the sprites prop.
/// What a tap on the canvas actually hit: the sprite, and — when its body
/// draws real lamps — the TRUE lamp index nearest the pointer.
///
/// The lamp is what lets the shell name an OBJECT rather than a whole
/// fixture (the patching view's Q10 ruling): sprites already carry true
/// indexes for the live-fill feed, and a click is a claim about one lamp.
/// `None` for a placeholder or strip body, where the sprite genuinely knows
/// nothing finer than "this fixture".
///
/// Since G1 round 3 the HULL answers first: `object` is the index into
/// [`FixtureSprite::objects`] whose body contains the point, which is how a
/// click lands on an object without hitting a 3 px lamp. The lamp stays —
/// it is the tiebreak between overlapping hulls, and the only answer where
/// no hull covers the point at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixturePick {
    pub key: String,
    pub lamp: Option<u32>,
    /// Index into the sprite's object list, when a hull claimed the point.
    pub object: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FixtureEvent {
    /// Tap on a fixture selects it; a background tap deselects (`None`).
    /// `toggle` = shift held: the shell toggles the pick within its
    /// sibling set instead of replacing the selection.
    Select {
        pick: Option<FixturePick>,
        toggle: bool,
    },
    /// A marquee's verdict: every sprite whose placed frame intersected
    /// the rect (project space). `additive` = shift held.
    Marquee { keys: Vec<String>, additive: bool },
    /// One gesture's placement updates — a single drag, a multi-move, or
    /// a shared-box scale. Live during the drag (`commit: false`), once
    /// at pointer-up (`commit: true`) — one committed write per gesture.
    Move {
        moves: Vec<(String, Placement)>,
        commit: bool,
    },
    /// Double-click on a fixture when not dived, or on a NEIGHBOUR while
    /// dived (D2: dive-switch). Carries the press-resolved pick facts so
    /// the shell can select the OBJECT the user pointed at on entry
    /// (unified-selection D4: double-click descends to the click).
    Dive {
        key: String,
        lamp: Option<u32>,
        object: Option<usize>,
    },
}

/// The placed frame's project-space AABB — the marquee's intersection
/// unit (a rotated sprite tests by its transformed corners' box, matching
/// the frame the user sees).
pub(crate) fn sprite_project_aabb(sprite: &FixtureSprite) -> [f64; 4] {
    let corners = sprite.placement.corners(sprite.bounds);
    let min_x = corners.iter().map(|c| c[0]).fold(f64::MAX, f64::min);
    let min_y = corners.iter().map(|c| c[1]).fold(f64::MAX, f64::min);
    let max_x = corners.iter().map(|c| c[0]).fold(f64::MIN, f64::max);
    let max_y = corners.iter().map(|c| c[1]).fold(f64::MIN, f64::max);
    [min_x, min_y, max_x, max_y]
}

/// Sprites whose placed frames intersect the project-space rect
/// (min/max corners) — the fixture-grain marquee's answer.
pub(crate) fn sprites_in_rect(
    sprites: &[FixtureSprite],
    min: [f32; 2],
    max: [f32; 2],
) -> Vec<String> {
    sprites
        .iter()
        .filter(|sprite| {
            let [sx0, sy0, sx1, sy1] = sprite_project_aabb(sprite);
            sx0 <= f64::from(max[0])
                && sx1 >= f64::from(min[0])
                && sy0 <= f64::from(max[1])
                && sy1 >= f64::from(min[1])
        })
        .map(|sprite| sprite.key.clone())
        .collect()
}

/// The display-subsample stride: with `total` true lamps drawn as `drawn`
/// points, drawn point `i` stands for true lamp `i * stride`. Never 0.
pub(crate) fn display_stride(total: u32, drawn: usize) -> usize {
    (total as usize).div_ceil(drawn.max(1)).max(1)
}

#[cfg(test)]
mod stride_tests {
    use super::display_stride;

    /// The true-index mapping under subsampling — the off-by-stride trap
    /// the sprite live colors (and the instance rings) both step around.
    #[test]
    fn display_stride_maps_drawn_points_to_true_lamps() {
        assert_eq!(display_stride(150, 150), 1, "no subsample = identity");
        assert_eq!(display_stride(4000, 2000), 2);
        assert_eq!(display_stride(4001, 2000), 3, "ceil, never floor");
        assert_eq!(
            display_stride(10, 0),
            10,
            "empty draw never divides by zero"
        );
        assert_eq!(display_stride(0, 0), 1, "degenerate stays a valid stride");
    }
}

/// Topmost sprite whose (padded) placed frame contains the project-space
/// point — inverse-transform containment in the sprite's own space.
pub(crate) fn hit_fixture<'a>(
    sprites: &'a [FixtureSprite],
    project_point: [f64; 2],
) -> Option<&'a FixtureSprite> {
    sprites.iter().rev().find(|sprite| {
        let [lx, ly] = sprite.placement.inverse(project_point);
        let [bx, by, bw, bh] = sprite.bounds;
        lx >= bx - HIT_PAD
            && lx <= bx + bw + HIT_PAD
            && ly >= by - HIT_PAD
            && ly <= by + bh + HIT_PAD
    })
}

/// The TRUE lamp index nearest a project-space point inside `sprite`, when
/// its body draws lamps at all.
///
/// Drawn point `i` stands for true lamp `i * stride` (the display
/// subsample), so the answer survives the stride: the index this returns is
/// always a real lamp of the fixture's own document, never a drawn slot.
/// Under subsampling only every k-th lamp is reachable — the shell resolves
/// the returned lamp to the OBJECT that owns it, and an object shorter than
/// the stride is sub-pixel on screen anyway.
///
/// Nearest, not hit-tested: the pointer is compared against the drawn points
/// in the sprite's OWN space (where the lamp radius is drawn), so a click in
/// the gap between two lamps still names the one the user was aiming at.
pub(crate) fn nearest_lamp(sprite: &FixtureSprite, project_point: [f64; 2]) -> Option<u32> {
    let FixtureBody::Lamps { points, total } = &sprite.body else {
        return None;
    };
    let stride = display_stride(*total, points.len());
    let [lx, ly] = sprite.placement.inverse(project_point);
    let (index, _) = points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let dx = f64::from(point[0]) - lx;
            let dy = f64::from(point[1]) - ly;
            (index, dx * dx + dy * dy)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    u32::try_from(index * stride).ok()
}

/// Which OBJECT of `sprite` a project-space point lands on: the index into
/// [`FixtureSprite::objects`] whose padded hull contains it.
///
/// The whole hull is the target, so a click in the middle of a dome sector
/// selects that sector without going anywhere near a lamp. Two rules keep
/// that honest where bodies overlap (a repeat whose instances interleave, an
/// object drawn inside another's convex span):
///
/// - a hull that contains the point AND owns the nearest lamp wins outright
///   — the lamp is the finest fact available, so it breaks the tie;
/// - otherwise the LAST containing hull wins, matching the paint order
///   (topmost) the rest of the canvas resolves overlaps by.
///
/// `None` = no body covers the point, and the caller falls back to the
/// nearest lamp exactly as before.
pub(crate) fn hit_object(sprite: &FixtureSprite, project_point: [f64; 2]) -> Option<usize> {
    if sprite.objects.is_empty() {
        return None;
    }
    let owner = nearest_lamp(sprite, project_point)
        .and_then(|lamp| sprite.objects.iter().position(|object| object.owns(lamp)));
    let local = sprite.placement.inverse(project_point);
    let mut found = None;
    for (index, object) in sprite.objects.iter().enumerate() {
        if !point_in_polygon(&object.hull, local) {
            continue;
        }
        if owner == Some(index) {
            return Some(index);
        }
        found = Some(index);
    }
    found
}

/// Has this press travelled far enough (CSS pixels) to count as a drag?
pub(crate) fn exceeds_drag_threshold(start_client: [f64; 2], client: [f64; 2]) -> bool {
    (client[0] - start_client[0]).abs() > DRAG_THRESHOLD_PX
        || (client[1] - start_client[1]).abs() > DRAG_THRESHOLD_PX
}

/// The placement a fixture drag has reached: the press-time placement
/// translated by the pointer's client-space travel converted to project
/// units through the camera scale.
pub(crate) fn dragged_placement(
    original: Placement,
    start_client: [f64; 2],
    client: [f64; 2],
    cam_scale: f32,
) -> Placement {
    let units_per_px = 1.0 / f64::from(cam_scale.max(1e-6));
    Placement {
        t: [
            original.t[0] + (client[0] - start_client[0]) * units_per_px,
            original.t[1] + (client[1] - start_client[1]) * units_per_px,
        ],
        ..original
    }
}

/// The whole moving set, translated by one pointer's travel — the
/// multi-fixture drag (every member keeps its own rotation/scale).
pub(crate) fn dragged_placements(
    originals: &[(String, Placement)],
    start_client: [f64; 2],
    client: [f64; 2],
    cam_scale: f32,
) -> Vec<(String, Placement)> {
    originals
        .iter()
        .map(|(key, original)| {
            (
                key.clone(),
                dragged_placement(*original, start_client, client, cam_scale),
            )
        })
        .collect()
}

/// The per-fixture arrange-scale clamp (matches the toolbar verbs).
const ARRANGE_SCALE_MIN: f64 = 0.05;
const ARRANGE_SCALE_MAX: f64 = 20.0;

/// Clamp a shared-box scale factor so EVERY member's own scale stays in
/// the arrange clamp — the whole set scales by one factor or not at all
/// (uniform about the shared box, D5).
pub(crate) fn clamp_scale_factor(originals: &[(String, Placement)], factor: f64) -> f64 {
    let mut lo = 1e-3_f64;
    let mut hi = f64::MAX;
    for (_, placement) in originals {
        let s = placement.s.max(1e-6);
        lo = lo.max(ARRANGE_SCALE_MIN / s);
        hi = hi.min(ARRANGE_SCALE_MAX / s);
    }
    if lo > hi {
        return 1.0;
    }
    factor.clamp(lo, hi)
}

/// The set scaled uniformly by `factor` about the project-space `anchor`
/// (the shared box's fixed corner): every translation interpolates toward
/// the anchor and every own-scale multiplies — the box scales as one
/// object.
pub(crate) fn scaled_placements(
    originals: &[(String, Placement)],
    anchor: [f32; 2],
    factor: f64,
) -> Vec<(String, Placement)> {
    let ax = f64::from(anchor[0]);
    let ay = f64::from(anchor[1]);
    originals
        .iter()
        .map(|(key, placement)| {
            (
                key.clone(),
                Placement {
                    t: [
                        ax + (placement.t[0] - ax) * factor,
                        ay + (placement.t[1] - ay) * factor,
                    ],
                    r: placement.r,
                    s: placement.s * factor,
                },
            )
        })
        .collect()
}

/// The fixture layer's render input.
pub(crate) struct FixtureLayerInput<'a> {
    pub sprites: &'a [FixtureSprite],
    /// Dived fixture's key: its body is not drawn (the live doc layers
    /// replace it) and every other sprite dims to context opacity.
    pub focused: Option<&'a str>,
    /// Project→screen scale (the camera's); counter-scaling for
    /// screen-constant strokes divides by this and each sprite's own
    /// placement scale.
    pub cam_scale: f32,
}

pub(crate) fn fixture_layer(input: &FixtureLayerInput<'_>) -> Element {
    let units_per_px = 1.0 / f64::from(input.cam_scale.max(1e-6));
    rsx! {
        for sprite in input.sprites.iter() {
            FixtureGroup {
                key: "{sprite.key}",
                sprite: sprite.clone(),
                units_per_px,
                dimmed: input.focused.is_some_and(|focused| focused != sprite.key),
                body_hidden: input.focused == Some(sprite.key.as_str()),
            }
        }
    }
}

/// One fixture on the canvas: name-tag frame above the content (labels
/// never overlap geometry), body under it. Purely visual — the canvas
/// hit test owns every gesture.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FixtureGroup(
    sprite: FixtureSprite,
    units_per_px: f64,
    /// Context look while another fixture is dived.
    dimmed: bool,
    /// The dived fixture: frame and tag stay, the live doc layers draw
    /// the body.
    body_hidden: bool,
) -> Element {
    let [bx, by, bw, bh] = sprite.bounds;
    let selected = sprite.selected;
    // The display-subsample stride: drawn lamp `index` is TRUE lamp
    // `index * stride` — the ring windows and the sprite live-color
    // attributes both need the real index.
    let lamp_stride = match &sprite.body {
        FixtureBody::Lamps { points, total } => display_stride(*total, points.len()),
        _ => 1,
    };
    // Screen-constant strokes/text inside a scaled group: counter the
    // group scale on top of the camera scale.
    let upp = units_per_px / sprite.placement.s.max(1e-6);
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
        g {
            transform: "{sprite.placement.svg_transform()}",
            opacity: if dimmed { "0.3" } else { "1" },
            style: "cursor: grab; pointer-events: none;",
            // The frame: gesture target (via the canvas hit test) +
            // selection highlight.
            rect {
                x: "{bx - frame_pad}",
                y: "{by - frame_pad}",
                width: "{bw + 2.0 * frame_pad}",
                height: "{bh + 2.0 * frame_pad}",
                rx: "{4.0 * upp}",
                fill: if selected { "rgba(76,159,254,0.06)" } else { "transparent" },
                style: "{frame_stroke}",
                stroke_dasharray: if sprite.arranged { "" } else { "3 3" },
            }
            // The name tag, above the content.
            text {
                x: "{bx - frame_pad}",
                y: "{tag_y}",
                font_size: "{tag_font}",
                fill: if selected { "#4c9ffe" } else { "var(--color-muted-foreground)" },
                style: "font-weight: 600; user-select: none;",
                "{sprite.label}"
            }
            if !body_hidden {
                // The OBJECT BODIES, under the lamps: one path each, faint at
                // rest, lifted on hover, accent-stroked when selected. These
                // are the only children that take pointer events — purely so
                // the browser can run `:hover` itself; they carry no handlers
                // and the canvas root still owns every gesture. The hover and
                // selected looks live in `.lpme-obj-hull` (style.css), so a
                // pointer move repaints without re-rendering a thing, which
                // is what keeps 150 dome objects cheap.
                for (index, object) in sprite.objects.iter().enumerate() {
                    if object.hull.len() >= 3 {
                        path {
                            key: "hull-{index}",
                            class: if object.selected { "lpme-obj-hull lpme-obj-hull-on" } else { "lpme-obj-hull" },
                            d: hull_path_d(&object.hull),
                            stroke_width: if object.selected { "{1.4 * upp}" } else { "{1.0 * upp}" },
                            // Not a gesture hook — the canvas root owns
                            // those — but the name the body stands for, so a
                            // walk (or a story diff) can say which is which.
                            "data-sprite-object": "{object.label}",
                        }
                    }
                }
                match &sprite.body {
                    FixtureBody::Lamps { points, .. } => rsx! {
                        for (index, point) in points.iter().enumerate() {
                            circle {
                                key: "{index}",
                                cx: "{point[0]}",
                                cy: "{point[1]}",
                                r: "{lamp_r}",
                                fill: "{sprite.color}",
                                fill_opacity: "0.9",
                                // The live-fill hooks (host sprite feed):
                                // sprite key + TRUE lamp index.
                                "data-sprite-fixture": "{sprite.key}",
                                "data-sprite-lamp": "{index * lamp_stride}",
                            }
                        }
                        if let Some((start, lamps)) = sprite.selected_range {
                            // Ring the selected instance's lamps (subsample-aware:
                            // ring what is drawn).
                            {
                                let stride = lamp_stride;
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
                            stroke: "{sprite.color}",
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
                                        fill: "{sprite.color}",
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite(key: &str, placement: Placement, bounds: [f64; 4]) -> FixtureSprite {
        FixtureSprite {
            key: key.to_string(),
            label: key.to_string(),
            color: "#fff".to_string(),
            placement,
            bounds,
            objects: Vec::new(),
            body: FixtureBody::Placeholder { lamps: 10 },
            arranged: true,
            selected: false,
            selected_range: None,
        }
    }

    #[test]
    fn hit_fixture_tests_padded_bounds_in_own_space() {
        let sprites = vec![sprite(
            "a",
            Placement {
                t: [100.0, 50.0],
                r: 90.0,
                s: 2.0,
            },
            [0.0, 0.0, 10.0, 10.0],
        )];
        // Own-space center (5,5) → scaled (10,10) → rotated 90° (-10,10) →
        // translated (90,60).
        assert_eq!(
            hit_fixture(&sprites, [90.0, 60.0]).map(|s| s.key.as_str()),
            Some("a")
        );
        // Just outside the padded frame in own space: own-space x = 10 +
        // HIT_PAD + ε maps past the pad.
        assert!(hit_fixture(&sprites, [100.0, 50.0 + (10.0 + HIT_PAD + 0.5) * 2.0]).is_none());
        // Inside the pad still hits.
        assert!(hit_fixture(&sprites, [100.0, 50.0 + (10.0 + HIT_PAD - 0.5) * 2.0]).is_some());
    }

    #[test]
    fn hit_fixture_prefers_the_topmost_sprite() {
        let sprites = vec![
            sprite("under", Placement::IDENTITY, [0.0, 0.0, 20.0, 20.0]),
            sprite("over", Placement::IDENTITY, [10.0, 10.0, 20.0, 20.0]),
        ];
        assert_eq!(
            hit_fixture(&sprites, [15.0, 15.0]).map(|s| s.key.as_str()),
            Some("over"),
            "overlap resolves to the last-rendered (topmost) sprite"
        );
        assert_eq!(
            hit_fixture(&sprites, [0.0, 0.0]).map(|s| s.key.as_str()),
            Some("under"),
            "outside the topmost pad, the sprite underneath answers"
        );
    }

    /// A tap names the TRUE lamp nearest it, in the sprite's own space and
    /// through its placement — the fact the shell turns into an OBJECT
    /// selection (Q10).
    #[test]
    fn nearest_lamp_names_a_true_index_through_the_placement() {
        let mut sprite = sprite("a", Placement::IDENTITY, [0.0, 0.0, 30.0, 0.0]);
        sprite.body = FixtureBody::Lamps {
            points: vec![[0.0, 0.0], [10.0, 0.0], [20.0, 0.0], [30.0, 0.0]],
            total: 4,
        };
        assert_eq!(nearest_lamp(&sprite, [0.0, 0.0]), Some(0));
        assert_eq!(nearest_lamp(&sprite, [21.0, 3.0]), Some(2));
        assert_eq!(
            nearest_lamp(&sprite, [100.0, 0.0]),
            Some(3),
            "a click past the end still names the lamp it was aiming at"
        );

        // Through a placement: the point is inverse-transformed first.
        sprite.placement = Placement {
            t: [100.0, 50.0],
            r: 0.0,
            s: 2.0,
        };
        assert_eq!(nearest_lamp(&sprite, [140.0, 50.0]), Some(2));
    }

    /// Display subsampling must not lose the true index: drawn point `i`
    /// stands for lamp `i * stride`, so what comes back is always a lamp of
    /// the fixture's own document (which is what an object span is measured
    /// in) — never a drawn slot.
    #[test]
    fn nearest_lamp_survives_the_display_stride() {
        let mut sprite = sprite("a", Placement::IDENTITY, [0.0, 0.0, 30.0, 0.0]);
        // 4000 true lamps drawn as 2000 points: stride 2.
        sprite.body = FixtureBody::Lamps {
            points: (0..2000).map(|i| [i as f32, 0.0]).collect(),
            total: 4000,
        };
        assert_eq!(display_stride(4000, 2000), 2);
        assert_eq!(nearest_lamp(&sprite, [0.0, 0.0]), Some(0));
        assert_eq!(nearest_lamp(&sprite, [7.0, 0.0]), Some(14), "7 * stride");
        assert_eq!(nearest_lamp(&sprite, [1999.0, 0.0]), Some(3998));
    }

    /// Bodies that draw no lamps know nothing finer than "this fixture" —
    /// and say so, so the shell can fall back to the fixture grain.
    #[test]
    fn a_lampless_body_names_no_lamp() {
        let placeholder = sprite("a", Placement::IDENTITY, [0.0, 0.0, 30.0, 30.0]);
        assert_eq!(nearest_lamp(&placeholder, [1.0, 1.0]), None);

        let mut strip = placeholder.clone();
        strip.body = FixtureBody::Strip { lamps: 60 };
        assert_eq!(nearest_lamp(&strip, [1.0, 1.0]), None);

        let mut empty = placeholder.clone();
        empty.body = FixtureBody::Lamps {
            points: Vec::new(),
            total: 0,
        };
        assert_eq!(nearest_lamp(&empty, [1.0, 1.0]), None);
    }

    #[test]
    fn drag_threshold_is_four_css_pixels() {
        assert!(!exceeds_drag_threshold([10.0, 10.0], [13.0, 13.0]));
        assert!(exceeds_drag_threshold([10.0, 10.0], [14.5, 10.0]));
        assert!(exceeds_drag_threshold([10.0, 10.0], [10.0, -5.0]));
    }

    #[test]
    fn dragged_placement_translates_by_client_travel_over_camera_scale() {
        let original = Placement {
            t: [5.0, 6.0],
            r: 30.0,
            s: 0.5,
        };
        let dragged = dragged_placement(original, [100.0, 100.0], [140.0, 80.0], 2.0);
        // 40 css px right at camera scale 2 = 20 project units; rotation and
        // scale ride along untouched.
        assert!((dragged.t[0] - 25.0).abs() < 1e-9);
        assert!((dragged.t[1] - (6.0 - 10.0)).abs() < 1e-9);
        assert_eq!(dragged.r, original.r);
        assert_eq!(dragged.s, original.s);
    }

    /// The shared-box scale (D5): every member's translation interpolates
    /// toward the anchor and its own scale multiplies — relative layout
    /// preserved, one factor for the whole set.
    #[test]
    fn scaled_placements_are_uniform_about_the_anchor() {
        let originals = vec![
            (
                "a".to_string(),
                Placement {
                    t: [10.0, 10.0],
                    r: 0.0,
                    s: 1.0,
                },
            ),
            (
                "b".to_string(),
                Placement {
                    t: [30.0, 10.0],
                    r: 45.0,
                    s: 2.0,
                },
            ),
        ];
        let scaled = scaled_placements(&originals, [0.0, 10.0], 2.0);
        assert_eq!(scaled[0].1.t, [20.0, 10.0]);
        assert_eq!(scaled[0].1.s, 2.0);
        assert_eq!(scaled[1].1.t, [60.0, 10.0]);
        assert_eq!(scaled[1].1.s, 4.0);
        assert_eq!(scaled[1].1.r, 45.0, "rotation never changes under scale");
        // Distances scale together: |a-b| doubled.
        let dx = scaled[1].1.t[0] - scaled[0].1.t[0];
        assert_eq!(dx, 40.0);
    }

    /// The factor clamp: no member may leave the arrange scale clamp, so
    /// the shared factor narrows to the intersection of every member's
    /// legal range (and a degenerate intersection scales by 1).
    #[test]
    fn scale_factor_clamps_to_every_members_range() {
        let member = |s: f64| {
            (
                "k".to_string(),
                Placement {
                    t: [0.0, 0.0],
                    r: 0.0,
                    s,
                },
            )
        };
        // One member near the top of the clamp: factor caps at 20/10.
        let originals = vec![member(1.0), member(10.0)];
        assert_eq!(clamp_scale_factor(&originals, 5.0), 2.0);
        // Near the bottom: factor floors at 0.05/0.1.
        let originals = vec![member(0.1)];
        assert_eq!(clamp_scale_factor(&originals, 0.001), 0.5);
        // In range passes through.
        let originals = vec![member(1.0)];
        assert_eq!(clamp_scale_factor(&originals, 1.5), 1.5);
    }

    /// The marquee intersects placed frames in project space, corners
    /// transformed — a translated sprite is found where it SITS.
    #[test]
    fn sprites_in_rect_intersects_placed_frames() {
        let sprite = |key: &str, tx: f64| FixtureSprite {
            key: key.to_string(),
            label: key.to_string(),
            color: "#fff".to_string(),
            placement: Placement {
                t: [tx, 0.0],
                r: 0.0,
                s: 1.0,
            },
            bounds: [0.0, 0.0, 20.0, 20.0],
            body: FixtureBody::Placeholder { lamps: 4 },
            arranged: true,
            selected: false,
            selected_range: None,
            objects: Vec::new(),
        };
        let sprites = vec![sprite("near", 0.0), sprite("far", 100.0)];
        assert_eq!(
            sprites_in_rect(&sprites, [-5.0, -5.0], [25.0, 25.0]),
            vec!["near".to_string()]
        );
        assert_eq!(
            sprites_in_rect(&sprites, [-5.0, -5.0], [150.0, 25.0]),
            vec!["near".to_string(), "far".to_string()]
        );
        assert!(sprites_in_rect(&sprites, [40.0, 40.0], [60.0, 60.0]).is_empty());
    }
}
