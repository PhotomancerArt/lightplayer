//! The editor canvas: doc-space SVG rendering with camera pan/zoom.
//!
//! Layers (bottom → top): dot-grid background, the authored canvas rect,
//! the fit-preview overlay (how the doc aspect-fits into a render target),
//! wiring arrows, lamps, wiring numbers. The canvas renders **doc space**
//! — the camera maps doc units to CSS pixels; nothing here is the
//! aspect-fitted texture view.

use dioxus::prelude::*;
use lpc_mapping::{Bounds2d, ResolvedMap2d, bounds_of_points, resolve};

use crate::editor_core::camera::Camera;
use crate::editor_core::editor_session::MapEditorSession;
use crate::editor_core::view_geometry::{ArrowInput, universe_rgb, wiring_arrows};
use crate::view::map_editor::EditorViewOptions;

/// Object fill palette (wiring-order cycling; matches the UX spike).
const OBJECT_COLORS: &[&str] = &[
    "#5aa9e6", "#3fd68e", "#e4c065", "#c792ea", "#f0913b", "#64d8cb",
];

#[must_use]
pub fn object_color(object_index: usize) -> &'static str {
    OBJECT_COLORS[object_index % OBJECT_COLORS.len()]
}

/// Lamp display radius in doc units: a fraction of the median consecutive
/// lamp spacing, so dense grids stay dots and sparse strips stay visible.
#[must_use]
pub fn lamp_display_radius(resolved: &ResolvedMap2d) -> f32 {
    let mut gaps: Vec<f32> = Vec::new();
    for span in &resolved.spans {
        let start = span.start as usize;
        let end = start + span.count as usize;
        for index in start..end.saturating_sub(1) {
            let a = resolved.lamps[index].pos;
            let b = resolved.lamps[index + 1].pos;
            let gap = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
            if gap > f32::EPSILON {
                gaps.push(gap);
            }
        }
    }
    if gaps.is_empty() {
        return 7.0;
    }
    gaps.sort_by(|a, b| a.partial_cmp(b).expect("finite gaps"));
    (gaps[gaps.len() / 2] * 0.34).clamp(1.5, 24.0)
}

/// The doc-space region a `target_aspect` render texture covers after
/// aspect-fit: the smallest rect with the target aspect that contains
/// `frame`, centered — the visual answer to "how does my doc map into
/// shader coordinates".
#[must_use]
pub fn fit_region(frame: Bounds2d, target_aspect: f32) -> Bounds2d {
    let frame_aspect = frame.width / frame.height.max(1e-6);
    let (width, height) = if frame_aspect >= target_aspect {
        (frame.width, frame.width / target_aspect)
    } else {
        (frame.height * target_aspect, frame.height)
    };
    Bounds2d {
        min_x: frame.min_x - (width - frame.width) / 2.0,
        min_y: frame.min_y - (height - frame.height) / 2.0,
        width,
        height,
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn EditorCanvas(
    session: Signal<MapEditorSession>,
    camera: Signal<Camera>,
    view_opts: Signal<EditorViewOptions>,
    viewport: Signal<[f32; 2]>,
) -> Element {
    let mut pan_last = use_signal(|| None::<[f32; 2]>);

    let cam = camera();
    let opts = view_opts();
    let session_read = session.read();
    let doc = session_read.doc();
    let resolved = resolve(doc).unwrap_or(ResolvedMap2d {
        lamps: Vec::new(),
        spans: Vec::new(),
    });
    let radius = lamp_display_radius(&resolved);
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
        fit_region(frame, 1.0) // 16×16 default target: square
    });
    let spans: Vec<(u32, u32)> = resolved
        .spans
        .iter()
        .map(|span| (span.start, span.count))
        .collect();
    let positions = resolved.positions();
    let arrows = opts.arrows.then(|| {
        wiring_arrows(&ArrowInput {
            positions: &positions,
            spans: &spans,
            view_width: 0.0, // unused by the renderer below (doc space)
            view_height: 0.0,
            end_gap: radius * 1.5,
            min_len: radius * 3.4,
        })
    });
    let show_numbers = opts.numbers && cam.scale * radius >= 5.0;
    let number_font = (radius * 0.9).clamp(4.0, 16.0);
    drop(session_read);

    rsx! {
        svg {
            class: "lpme-canvas",
            onpointerdown: move |evt| {
                let point = evt.data().client_coordinates();
                pan_last.set(Some([point.x as f32, point.y as f32]));
            },
            onpointermove: move |evt| {
                if let Some(last) = pan_last() {
                    let point = evt.data().client_coordinates();
                    let next = [point.x as f32, point.y as f32];
                    camera.write().pan(next[0] - last[0], next[1] - last[1]);
                    pan_last.set(Some(next));
                }
            },
            onpointerup: move |_| pan_last.set(None),
            onpointerleave: move |_| pan_last.set(None),
            onwheel: move |evt| {
                evt.prevent_default();
                let delta = evt.data().delta();
                let (dx, dy) = match delta {
                    dioxus::html::geometry::WheelDelta::Pixels(v) => (v.x as f32, v.y as f32),
                    dioxus::html::geometry::WheelDelta::Lines(v) => {
                        (v.x as f32 * 16.0, v.y as f32 * 16.0)
                    }
                    dioxus::html::geometry::WheelDelta::Pages(v) => {
                        (v.x as f32 * 100.0, v.y as f32 * 100.0)
                    }
                };
                let modifiers = evt.data().modifiers();
                if modifiers.ctrl() || modifiers.meta() {
                    let point = evt.data().client_coordinates();
                    let factor = (1.0015f32).powf(-dy);
                    camera.write().zoom_at([point.x as f32, point.y as f32], factor);
                } else {
                    camera.write().pan(-dx, -dy);
                }
            },
            defs {
                pattern {
                    id: "lpme-dots",
                    width: "28",
                    height: "28",
                    pattern_units: "userSpaceOnUse",
                    circle { cx: "1", cy: "1", r: "1", fill: "rgba(255, 255, 255, 0.06)" }
                }
                marker {
                    id: "lpme-arrow-head",
                    view_box: "0 0 8 8",
                    ref_x: "7",
                    ref_y: "4",
                    marker_width: "5",
                    marker_height: "5",
                    orient: "auto-start-reverse",
                    path { d: "M0,0.8 L7.4,4 L0,7.2 z", fill: "currentColor" }
                }
            }
            g {
                transform: "translate({cam.x},{cam.y}) scale({cam.scale})",
                rect {
                    x: "-100000",
                    y: "-100000",
                    width: "200000",
                    height: "200000",
                    fill: "url(#lpme-dots)",
                }
                if let Some(rect) = canvas_rect {
                    rect {
                        class: "lpme-canvas-rect",
                        x: "{rect.min_x}",
                        y: "{rect.min_y}",
                        width: "{rect.width}",
                        height: "{rect.height}",
                        stroke_width: "{1.5 / cam.scale}",
                    }
                }
                if let Some(rect) = fit_rect {
                    rect {
                        class: "lpme-fit-rect",
                        x: "{rect.min_x}",
                        y: "{rect.min_y}",
                        width: "{rect.width}",
                        height: "{rect.height}",
                        stroke_width: "{2.0 / cam.scale}",
                    }
                    text {
                        class: "lpme-fit-label",
                        x: "{rect.min_x + 6.0 / cam.scale}",
                        y: "{rect.min_y - 6.0 / cam.scale}",
                        font_size: "{12.0 / cam.scale}",
                        "texture frame (square target)"
                    }
                }
                if let Some(overlay) = arrows {
                    for (index, seg) in overlay.segs.iter().enumerate() {
                        line {
                            key: "{index}",
                            class: if seg.chain { "lpme-arrow-chain" } else { "lpme-arrow-wire" },
                            x1: "{seg.x1}",
                            y1: "{seg.y1}",
                            x2: "{seg.x2}",
                            y2: "{seg.y2}",
                            stroke_width: "{(radius * 0.22).clamp(0.6, 3.0)}",
                            marker_end: "url(#lpme-arrow-head)",
                        }
                    }
                }
                for lamp in &resolved.lamps {
                    circle {
                        key: "l{lamp.index}",
                        cx: "{lamp.pos[0]}",
                        cy: "{lamp.pos[1]}",
                        r: "{radius}",
                        fill: if opts.universes {
                            {
                                let [r, g, b] = universe_rgb(lamp.index);
                                format!("rgb({r} {g} {b})")
                            }
                        } else {
                            object_color(lamp.object as usize).to_string()
                        },
                        stroke: "#000",
                        stroke_width: "{(radius * 0.12).clamp(0.2, 1.5)}",
                    }
                }
                if show_numbers {
                    for lamp in &resolved.lamps {
                        text {
                            key: "n{lamp.index}",
                            class: "lpme-lamp-num",
                            x: "{lamp.pos[0]}",
                            y: "{lamp.pos[1] + number_font * 0.34}",
                            font_size: "{number_font}",
                            text_anchor: "middle",
                            "{lamp.index + 1}"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_region_pads_the_short_axis_and_centers() {
        let frame = Bounds2d {
            min_x: 0.0,
            min_y: 0.0,
            width: 200.0,
            height: 50.0,
        };
        // Wide frame into a square target: height pads to 200, centered.
        let region = fit_region(frame, 1.0);
        assert!((region.width - 200.0).abs() < 1e-3);
        assert!((region.height - 200.0).abs() < 1e-3);
        assert!((region.min_y - (-75.0)).abs() < 1e-3);
        // Tall frame into a wide target: width pads.
        let tall = Bounds2d {
            min_x: 10.0,
            min_y: 0.0,
            width: 50.0,
            height: 100.0,
        };
        let region = fit_region(tall, 2.0);
        assert!((region.width - 200.0).abs() < 1e-3);
        assert!((region.min_x - (10.0 - 75.0)).abs() < 1e-3);
    }

    #[test]
    fn lamp_radius_tracks_median_spacing() {
        let doc = lpc_mapping::corpus::panel_16x16();
        let resolved = resolve(&doc).unwrap();
        let radius = lamp_display_radius(&resolved);
        // Panel pitch is 26 → radius ≈ 26 * 0.34.
        assert!((radius - 26.0 * 0.34).abs() < 0.5, "radius {radius}");
    }
}
