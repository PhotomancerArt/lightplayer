//! Mapping view options for the control-product lamp display.
//!
//! The lamp view is one renderer with options (D7 of the 2D mapping plan):
//! `live` colors lamps from the control frame (the classic preview),
//! `universes` colors by derived DMX universe, `numbers` prints wiring-order
//! indices inside lamps, and `arrows` overlays wiring-direction arrows —
//! per-path runs plus a dashed chain hop between consecutive paths. Options
//! compose; precedence for lamp fill is live > universes > neutral.
//!
//! Pure geometry helpers live here (host-tested); the SVG overlay and the
//! toggle chrome are thin Dioxus components over them.

use dioxus::prelude::*;
use lpa_studio_core::{ControlLayout2d, ControlPathSpan2d};

/// RGB lamps per DMX universe (mirrors `lpc_mapping::LAMPS_PER_UNIVERSE`).
pub const LAMPS_PER_UNIVERSE: u32 = 170;

/// Universe fill palette (UI concern; distinct from object palettes and from
/// the violet bound-state family).
const UNIVERSE_COLORS: &[[u8; 3]] = &[
    [90, 169, 230],  // blue
    [228, 192, 101], // gold
    [63, 214, 142],  // green
    [199, 146, 234], // lavender
    [240, 145, 59],  // orange
    [239, 83, 80],   // red
    [100, 216, 203], // teal
    [240, 98, 146],  // pink
];

/// Neutral lamp fill when neither live nor universe coloring applies.
const NEUTRAL_LAMP_RGB: [u8; 3] = [96, 102, 112];

/// View options for the lamp map display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapViewOptions {
    pub numbers: bool,
    pub arrows: bool,
    pub universes: bool,
    pub live: bool,
}

impl Default for MapViewOptions {
    fn default() -> Self {
        Self {
            numbers: false,
            arrows: false,
            universes: false,
            live: true,
        }
    }
}

/// Zero-based universe of a wiring-order lamp index.
#[must_use]
pub fn lamp_universe(lamp_index: u32) -> u32 {
    lamp_index / LAMPS_PER_UNIVERSE
}

/// Universe fill color for a lamp.
#[must_use]
pub fn universe_rgb(lamp_index: u32) -> [u8; 3] {
    UNIVERSE_COLORS[(lamp_universe(lamp_index) as usize) % UNIVERSE_COLORS.len()]
}

#[must_use]
pub const fn neutral_lamp_rgb() -> [u8; 3] {
    NEUTRAL_LAMP_RGB
}

/// One wiring arrow segment in overlay viewBox units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapArrowSeg {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    /// `true` for the dashed hop between consecutive paths.
    pub chain: bool,
}

/// Overlay geometry: viewBox size plus arrow segments.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapArrowOverlay {
    pub view_width: f32,
    pub view_height: f32,
    pub segs: Vec<MapArrowSeg>,
}

/// Build wiring arrows from a display layout. Lamps are connected in
/// wiring order within each path span (all lamps form one span when the
/// producer sent none), and consecutive spans are linked with a chain hop.
#[must_use]
pub fn wiring_arrow_overlay(layout: &ControlLayout2d) -> MapArrowOverlay {
    const VIEW_W: f32 = 1000.0;
    /// Pull arrow endpoints off the lamp centers so heads stay visible.
    const END_GAP: f32 = 16.0;
    /// Segments shorter than this draw no arrow (lamps visually touch).
    const MIN_LEN: f32 = 34.0;

    let aspect = layout.width_hint.max(1) as f32 / layout.height_hint.max(1) as f32;
    let view_height = VIEW_W / aspect;
    let point = |index: usize| -> Option<[f32; 2]> {
        let lamp = layout.lamps.get(index)?;
        Some([lamp.center[0] * VIEW_W, lamp.center[1] * view_height])
    };

    let spans: Vec<ControlPathSpan2d> = if layout.paths.is_empty() {
        vec![ControlPathSpan2d {
            first_lamp: 0,
            lamp_count: layout.lamps.len() as u32,
        }]
    } else {
        layout.paths.clone()
    };

    let mut segs = Vec::new();
    let mut push_seg = |a: [f32; 2], b: [f32; 2], chain: bool| {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < MIN_LEN {
            return;
        }
        let ux = dx / len;
        let uy = dy / len;
        segs.push(MapArrowSeg {
            x1: a[0] + ux * END_GAP,
            y1: a[1] + uy * END_GAP,
            x2: b[0] - ux * END_GAP,
            y2: b[1] - uy * END_GAP,
            chain,
        });
    };

    for span in &spans {
        let start = span.first_lamp as usize;
        let end = start + span.lamp_count as usize;
        for index in start..end.saturating_sub(1) {
            if let (Some(a), Some(b)) = (point(index), point(index + 1)) {
                push_seg(a, b, false);
            }
        }
    }
    for pair in spans.windows(2) {
        let tail = pair[0].first_lamp as usize + pair[0].lamp_count as usize;
        let head = pair[1].first_lamp as usize;
        if let (Some(a), Some(b)) = (point(tail.saturating_sub(1)), point(head)) {
            push_seg(a, b, true);
        }
    }

    MapArrowOverlay {
        view_width: VIEW_W,
        view_height,
        segs,
    }
}

/// The wiring-arrow SVG overlay, absolutely positioned over the lamp layout.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MapArrowsOverlay(overlay: MapArrowOverlay) -> Element {
    if overlay.segs.is_empty() {
        return rsx! {};
    }
    rsx! {
        svg {
            class: "ux-map-arrows",
            view_box: "0 0 {overlay.view_width} {overlay.view_height}",
            preserve_aspect_ratio: "none",
            defs {
                marker {
                    id: "ux-map-arrow-head",
                    view_box: "0 0 8 8",
                    ref_x: "7",
                    ref_y: "4",
                    marker_width: "5",
                    marker_height: "5",
                    orient: "auto-start-reverse",
                    path { d: "M0,0.8 L7.4,4 L0,7.2 z", fill: "currentColor" }
                }
            }
            for (index, seg) in overlay.segs.iter().enumerate() {
                line {
                    key: "{index}",
                    class: if seg.chain { "ux-map-arrow-chain" } else { "ux-map-arrow-wire" },
                    x1: "{seg.x1}",
                    y1: "{seg.y1}",
                    x2: "{seg.x2}",
                    y2: "{seg.y2}",
                    marker_end: "url(#ux-map-arrow-head)",
                }
            }
        }
    }
}

/// Compact toggle pills for the map view options.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MapViewToggles(value: MapViewOptions, on_change: EventHandler<MapViewOptions>) -> Element {
    let toggle = move |apply: fn(MapViewOptions) -> MapViewOptions| {
        let next = apply(value);
        move |_| on_change.call(next)
    };
    rsx! {
        div { class: "ux-map-toggle-row",
            button {
                class: if value.numbers { "ux-map-toggle ux-map-toggle-on" } else { "ux-map-toggle" },
                title: "wiring numbers",
                onclick: toggle(|mut v| { v.numbers = !v.numbers; v }),
                "123"
            }
            button {
                class: if value.arrows { "ux-map-toggle ux-map-toggle-on" } else { "ux-map-toggle" },
                title: "wiring arrows",
                onclick: toggle(|mut v| { v.arrows = !v.arrows; v }),
                "→"
            }
            button {
                class: if value.universes { "ux-map-toggle ux-map-toggle-on" } else { "ux-map-toggle" },
                title: "universe colors (170 lamps each)",
                onclick: toggle(|mut v| { v.universes = !v.universes; v }),
                "uni"
            }
            button {
                class: if value.live { "ux-map-toggle ux-map-toggle-on" } else { "ux-map-toggle" },
                title: "live output colors",
                onclick: toggle(|mut v| { v.live = !v.live; v }),
                "live"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{ControlLamp2d, Revision};

    #[test]
    fn universe_derivation_flows_at_170() {
        assert_eq!(lamp_universe(0), 0);
        assert_eq!(lamp_universe(169), 0);
        assert_eq!(lamp_universe(170), 1);
        assert_ne!(universe_rgb(0), universe_rgb(170));
    }

    #[test]
    fn arrows_connect_within_spans_and_chain_between_them() {
        let layout = layout_with_spans(
            &[[0.1, 0.5], [0.3, 0.5], [0.5, 0.5], [0.7, 0.5]],
            &[(0, 2), (2, 2)],
        );
        let overlay = wiring_arrow_overlay(&layout);
        let chains: Vec<_> = overlay.segs.iter().filter(|seg| seg.chain).collect();
        let wires: Vec<_> = overlay.segs.iter().filter(|seg| !seg.chain).collect();
        assert_eq!(wires.len(), 2); // 0→1 and 2→3
        assert_eq!(chains.len(), 1); // 1→2
    }

    #[test]
    fn missing_spans_fall_back_to_one_run() {
        let layout = layout_with_spans(&[[0.1, 0.5], [0.5, 0.5], [0.9, 0.5]], &[]);
        let overlay = wiring_arrow_overlay(&layout);
        assert_eq!(overlay.segs.len(), 2);
        assert!(overlay.segs.iter().all(|seg| !seg.chain));
    }

    #[test]
    fn touching_lamps_draw_no_arrow() {
        let layout = layout_with_spans(&[[0.5, 0.5], [0.51, 0.5]], &[]);
        assert!(wiring_arrow_overlay(&layout).segs.is_empty());
    }

    fn layout_with_spans(centers: &[[f32; 2]], spans: &[(u32, u32)]) -> ControlLayout2d {
        let lamps = centers
            .iter()
            .enumerate()
            .map(|(index, center)| ControlLamp2d {
                lamp_index: index as u32,
                sample_start: index as u32 * 3,
                center: *center,
                radius: 0.02,
            })
            .collect();
        ControlLayout2d::new(Revision::new(1), 4, 1, lamps).with_paths(
            spans
                .iter()
                .map(|(first_lamp, lamp_count)| ControlPathSpan2d {
                    first_lamp: *first_lamp,
                    lamp_count: *lamp_count,
                })
                .collect(),
        )
    }
}
