//! Mapping view options for the control-product lamp display.
//!
//! The lamp view is one renderer with options (D7 of the 2D mapping plan):
//! `live` colors lamps from the control frame (the classic preview),
//! `universes` colors by derived DMX universe, `numbers` prints wiring-order
//! indices inside lamps, and `arrows` overlays wiring-direction arrows —
//! per-path runs plus a dashed chain hop between consecutive paths. Options
//! compose; precedence for lamp fill is live > universes > neutral.
//!
//! The palette and arrow geometry live in `lpa-mapping-editor` (shared with
//! the editor canvas); this module adapts `ControlLayout2d` to that neutral
//! input and keeps the Studio-side SVG overlay + toggle chrome components.

use dioxus::prelude::*;
use lpa_mapping_editor::{ArrowInput, EditorViewOptions, wiring_arrows};
use lpa_studio_core::ControlLayout2d;

use crate::base::icon::{StudioIcon, StudioIconName};

// The palette/derivation and arrow geometry live in the editor crate so the
// face renderer and the mapping editor share one implementation.
pub use lpa_mapping_editor::{MapArrowOverlay, neutral_lamp_rgb, universe_rgb};

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

// One view state serves both faces of the output section (the toggle bar
// stays live across the view ⇄ edit flip): the editor's options are the
// superset, these conversions carry the shared fields.
impl From<EditorViewOptions> for MapViewOptions {
    fn from(opts: EditorViewOptions) -> Self {
        Self {
            numbers: opts.numbers,
            arrows: opts.arrows,
            universes: opts.universes,
            live: opts.live,
        }
    }
}

impl MapViewOptions {
    /// Editor options with these shared fields and editor-only fields at
    /// their defaults (initial face state).
    #[must_use]
    pub fn into_editor(self) -> EditorViewOptions {
        EditorViewOptions {
            numbers: self.numbers,
            arrows: self.arrows,
            universes: self.universes,
            live: self.live,
            fit_preview: false,
        }
    }

    /// Write the shared fields into `editor`, preserving editor-only state
    /// (fit preview).
    pub fn apply_to_editor(self, editor: &mut EditorViewOptions) {
        editor.numbers = self.numbers;
        editor.arrows = self.arrows;
        editor.universes = self.universes;
        editor.live = self.live;
    }
}

/// Build wiring arrows from a control display layout: adapt the layout to
/// the shared neutral geometry (1000-unit-wide view, aspect from the hints).
#[must_use]
pub fn wiring_arrow_overlay(layout: &ControlLayout2d) -> MapArrowOverlay {
    const VIEW_W: f32 = 1000.0;
    let aspect = layout.width_hint.max(1) as f32 / layout.height_hint.max(1) as f32;
    let view_height = VIEW_W / aspect;
    let positions: Vec<[f32; 2]> = layout
        .lamps
        .iter()
        .map(|lamp| [lamp.center[0] * VIEW_W, lamp.center[1] * view_height])
        .collect();
    let spans: Vec<(u32, u32)> = layout
        .paths
        .iter()
        .map(|span| (span.first_lamp, span.lamp_count))
        .collect();
    wiring_arrows(&ArrowInput {
        positions: &positions,
        spans: &spans,
        view_width: VIEW_W,
        view_height,
        end_gap: 16.0,
        min_len: 34.0,
    })
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

/// Pinned icon-toggle bar for the map view options (sits above the lamp
/// display in the output section — pinned, not floating, per the M3 gate).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn MapViewToggles(
    value: MapViewOptions,
    on_change: EventHandler<MapViewOptions>,
    /// Render only the buttons (the host provides the bar wrapper).
    #[props(default = false)]
    bare: bool,
) -> Element {
    let toggle = move |apply: fn(MapViewOptions) -> MapViewOptions| {
        let next = apply(value);
        move |_| on_change.call(next)
    };
    let class_for = |on: bool| {
        if on {
            "ux-map-toggle ux-map-toggle-on"
        } else {
            "ux-map-toggle"
        }
    };
    let buttons = rsx! {
            button {
                class: class_for(value.numbers),
                title: "wiring numbers (N)",
                onclick: toggle(|mut v| { v.numbers = !v.numbers; v }),
                StudioIcon { name: StudioIconName::MapNumbers, size: 13 }
            }
            button {
                class: class_for(value.arrows),
                title: "wiring arrows (A)",
                onclick: toggle(|mut v| { v.arrows = !v.arrows; v }),
                StudioIcon { name: StudioIconName::MapArrows, size: 13 }
            }
            button {
                class: class_for(value.universes),
                title: "universe colors, 170 lamps each (U)",
                onclick: toggle(|mut v| { v.universes = !v.universes; v }),
                StudioIcon { name: StudioIconName::MapUniverses, size: 13 }
            }
            button {
                class: class_for(value.live),
                title: "live output colors (L)",
                onclick: toggle(|mut v| { v.live = !v.live; v }),
                StudioIcon { name: StudioIconName::MapLive, size: 13 }
            }
    };
    if bare {
        buttons
    } else {
        rsx! {
            div { class: "ux-map-toggle-bar", {buttons} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_studio_core::{ControlLamp2d, ControlPathSpan2d, Revision};

    // Geometry semantics are tested in lpa-mapping-editor; this covers the
    // ControlLayout2d adapter only.
    #[test]
    fn adapter_scales_hints_and_forwards_spans() {
        let lamps = vec![
            lamp(0, [0.1, 0.5]),
            lamp(1, [0.3, 0.5]),
            lamp(2, [0.5, 0.5]),
            lamp(3, [0.7, 0.5]),
        ];
        let layout = ControlLayout2d::new(Revision::new(1), 4, 1, lamps).with_paths(vec![
            ControlPathSpan2d {
                first_lamp: 0,
                lamp_count: 2,
            },
            ControlPathSpan2d {
                first_lamp: 2,
                lamp_count: 2,
            },
        ]);
        let overlay = wiring_arrow_overlay(&layout);
        assert_eq!(overlay.view_width, 1000.0);
        assert_eq!(overlay.view_height, 250.0); // 4:1 hints
        assert_eq!(overlay.segs.iter().filter(|seg| !seg.chain).count(), 2);
        assert_eq!(overlay.segs.iter().filter(|seg| seg.chain).count(), 1);
    }

    fn lamp(index: u32, center: [f32; 2]) -> ControlLamp2d {
        ControlLamp2d {
            lamp_index: index,
            sample_start: index * 3,
            center,
            radius: 0.02,
        }
    }
}
