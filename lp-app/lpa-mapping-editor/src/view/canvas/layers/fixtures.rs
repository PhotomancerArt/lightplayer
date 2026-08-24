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
//! Objects are THINGS here (G1 round 3): each one draws a body around its own
//! lamps — faint at rest, lifted on hover, accent-stroked when selected — and
//! the WHOLE body is the click target, which is what makes a 3 px lamp on a
//! dome reachable at all. Since the design-language round that body is an
//! ALIGNED OUTLINE (outline.rs) rather than a convex hull, and a path object's
//! lamps are voronoi CELLS (cells.rs) rather than dots — the cell is the
//! lamp's live surface, so it carries the live-fill hooks the dot used to.
//!
//! The shell builds all of that geometry; this layer only paints it and
//! hit-tests it.

use dioxus::prelude::*;

use super::cells::LampCell;
use super::hull::hull_path_d;
use super::outline::point_in_loops;
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
    /// drawn when the selection has no BODY to outline instead — the body
    /// IS the selection indicator wherever one exists (G1 round 3).
    pub selected_range: Option<(u32, u32)>,
    /// This fixture's objects as clickable bodies, in the shell's own order
    /// (a pick reports the INDEX into this list). Empty for a fixture whose
    /// document names no objects, or whose body draws no lamps to outline.
    pub objects: Vec<SpriteObject>,
}

/// One object of a fixture, as the canvas draws and hit-tests it.
///
/// The crate stays project-unaware: an object here is a body to click, an
/// outline to paint, its lamps' cells, a name for the tooltip, and a selected
/// flag. What it MEANS — an instance path, a lamp range — is the shell's
/// business, recovered from the index this sprite lists it at.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteObject {
    /// Human name, for the hover title only.
    pub label: String,
    /// The HIT body in the sprite's OWN space: the symmetric on-path band's
    /// loops (planning Q7 — a thin one-sided VISUAL must not make the object
    /// harder to click), read with nonzero winding. Empty means "no body" —
    /// nothing hits.
    pub hull: Vec<Vec<[f32; 2]>>,
    /// The VISUAL outline loops, aligned as the document authored it. Empty
    /// means nothing is drawn.
    pub outline: Vec<Vec<[f32; 2]>>,
    /// Voronoi cells for objects whose lamps read as a ribbon (path,
    /// polygon); empty for the kinds that keep dots. `lamp` indexes the
    /// sprite's DISPLAYED points, so the live-fill hooks stride exactly like
    /// the circles'.
    pub cells: Vec<LampCell>,
    /// `(first lamp, count)` in the fixture's TRUE numbering — the same
    /// space [`nearest_lamp`] answers in, so overlapping bodies can be broken
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
/// Since G1 round 3 the BODY answers first: `object` is the index into
/// [`FixtureSprite::objects`] whose body contains the point, which is how a
/// click lands on an object without hitting a 3 px lamp. The lamp stays —
/// it is the tiebreak between overlapping bodies, and the only answer where
/// no body covers the point at all.
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
    Select(Option<FixturePick>),
    /// Live during a drag (`commit: false`), once at pointer-up
    /// (`commit: true`) — one committed move per gesture.
    Move {
        key: String,
        placement: Placement,
        commit: bool,
    },
    /// Double-click on a fixture when not dived, or on a NEIGHBOUR while
    /// dived (D2: dive-switch).
    Dive(String),
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
/// [`FixtureSprite::objects`] whose hit body contains it.
///
/// The whole body is the target, so a click in the middle of a dome sector
/// selects that sector without going anywhere near a lamp. Two rules keep
/// that honest where bodies overlap (a repeat whose instances interleave, an
/// object drawn inside another's span):
///
/// - a body that contains the point AND owns the nearest lamp wins outright
///   — the lamp is the finest fact available, so it breaks the tie;
/// - otherwise the LAST containing body wins, matching the paint order
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
    let [lx, ly] = sprite.placement.inverse(project_point);
    let local = [lx as f32, ly as f32];
    let mut found = None;
    for (index, object) in sprite.objects.iter().enumerate() {
        if !point_in_loops(&object.hull, local) {
            continue;
        }
        if owner == Some(index) {
            return Some(index);
        }
        found = Some(index);
    }
    found
}

/// Every loop of an outline as ONE `d`: subpaths, so the whole object is a
/// single element (dome scale is ~150 of them) and the browser's nonzero
/// fill rule does the merging a boolean union would otherwise cost.
#[must_use]
fn loops_path_d(loops: &[Vec<[f32; 2]>]) -> String {
    let mut d = String::new();
    for polygon in loops.iter().filter(|polygon| polygon.len() >= 3) {
        if !d.is_empty() {
            d.push(' ');
        }
        d.push_str(&hull_path_d(polygon));
    }
    d
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
    // The emitter core inside a cell: a screen-constant speck of near-white,
    // so a big cell still reads as "a lamp lives here" rather than as a
    // painted tile.
    let core_r = 1.1 * upp;
    let lamp_points: &[[f32; 2]] = match &sprite.body {
        FixtureBody::Lamps { points, .. } => points,
        _ => &[],
    };
    // A lamp its object paints as a CELL draws no dot: the cell is that
    // lamp's whole surface (and carries its live-fill hooks), so a circle
    // under it would only be a second, differently-shaped copy of it.
    let mut celled = vec![false; lamp_points.len()];
    for object in &sprite.objects {
        for cell in &object.cells {
            if let Some(slot) = celled.get_mut(cell.lamp) {
                *slot = true;
            }
        }
    }
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
                    if !object.outline.is_empty() {
                        path {
                            key: "hull-{index}",
                            class: if object.selected { "lpme-obj-hull lpme-obj-hull-on" } else { "lpme-obj-hull" },
                            d: loops_path_d(&object.outline),
                            // Overlapping strand loops merge and a closed
                            // strand's hole stays open — the same rule
                            // `point_in_loops` hit-tests with, so what is
                            // clickable is what is painted.
                            fill_rule: "nonzero",
                            stroke_width: if object.selected { "{1.4 * upp}" } else { "{1.0 * upp}" },
                            // The object's own colour, for the at-rest stroke
                            // tint (`.lpme-obj-hull` mixes it down); hover and
                            // selected keep the shared blues.
                            style: "--lpme-obj-c: {sprite.color};",
                            // Not a gesture hook — the canvas root owns
                            // those — but the name the body stands for, so a
                            // walk (or a story diff) can say which is which.
                            "data-sprite-object": "{object.label}",
                        }
                    }
                }
                // The CELLS, over every outline (an object's cells must not
                // be tinted by a neighbour's body fill) and under nothing:
                // each one is a lamp's live surface, so it carries the same
                // feed hooks the circles do.
                for (index, object) in sprite.objects.iter().enumerate() {
                    for cell in object.cells.iter() {
                        if cell.polygon.len() >= 3 {
                            path {
                                key: "cell-{index}-{cell.lamp}",
                                d: hull_path_d(&cell.polygon),
                                fill: "{sprite.color}",
                                fill_opacity: "0.85",
                                "data-sprite-fixture": "{sprite.key}",
                                "data-sprite-lamp": "{cell.lamp * lamp_stride}",
                            }
                            if let Some(point) = lamp_points.get(cell.lamp) {
                                circle {
                                    key: "core-{index}-{cell.lamp}",
                                    cx: "{point[0]}",
                                    cy: "{point[1]}",
                                    r: "{core_r}",
                                    fill: "#fffaf0",
                                    fill_opacity: "0.45",
                                }
                            }
                        }
                    }
                }
                match &sprite.body {
                    FixtureBody::Lamps { points, .. } => rsx! {
                        for (index, point) in points.iter().enumerate() {
                            if !celled.get(index).copied().unwrap_or(false) {
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
}
