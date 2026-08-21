//! The fixture layer: the project's fixtures as arranged sprites — frames,
//! name tags, honest bodies, selection rings — rendered in PROJECT space
//! between the dot grid and the placed doc layers.
//!
//! Sprites are plain data (the shell builds them from its surface; this
//! crate stays project-unaware), and the layer is purely visual:
//! `pointer-events: none` throughout — the canvas-level hit test owns
//! every fixture gesture (SVG child delegation is unreliable under
//! Dioxus's event delegation, 07a39242f).

use dioxus::prelude::*;

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
    /// Selected instance's `(start, lamps)` window, for lamp rings.
    pub selected_range: Option<(u32, u32)>,
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
#[derive(Clone, Debug, PartialEq)]
pub enum FixtureEvent {
    /// Tap on a fixture selects it; a background tap deselects (`None`).
    Select(Option<String>),
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
