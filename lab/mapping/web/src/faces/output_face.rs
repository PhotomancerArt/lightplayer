//! An output's face: its title, then one row per port — the port's name
//! and fill over its pixels, as the board has them. The rows, names and
//! run outlines are SVG; the pixels are squares on a canvas (Q4), coloured
//! from the board frame by output and port.

use dioxus::prelude::*;
use lab_mapping_model::{BoardFrame, DetailLevel, Emphasis, UiOutputFace};

use crate::dense_canvas::{Raster, canvas_id, put_when_mounted};
use crate::faces::{CardTitle, card_font, emphasis_class, rgb};

/// Squares this big on screen get a hairline gap between them.
const GAP_PIXEL_PX: f64 = 4.0;

#[component]
pub fn OutputFace(
    face: UiOutputFace,
    name: String,
    w: f64,
    h: f64,
    detail: DetailLevel,
    zoom: f64,
    frame: ReadSignal<BoardFrame>,
) -> Element {
    let font = card_font();
    let full = detail == DetailLevel::Full;
    let shows = detail > DetailLevel::Dot;
    rsx! {
        svg { class: "face-svg", view_box: "0 0 {w} {h}", preserve_aspect_ratio: "none",
            if full {
                CardTitle { name }
            }
            if shows {
                for p in face.ports.iter() {
                    g { key: "{p.key}",
                        if p.tinted {
                            rect {
                                class: "port-tint",
                                x: "{p.row.min.x}", y: "{p.row.min.y}",
                                width: "{p.row.width()}", height: "{p.row.height()}",
                            }
                        }
                        line { class: "port-sep", x1: "0", y1: "{p.row.min.y}", x2: "{w}", y2: "{p.row.min.y}" }
                        if full {
                            text {
                                class: "port-name",
                                style: "font-size: {font}px",
                                x: "{font * 0.7}", y: "{p.row.min.y + p.row.height() * 0.28}",
                                "{p.name}"
                            }
                            text {
                                class: "port-fill",
                                style: "font-size: {font * 0.9}px",
                                x: "{w - font * 0.7}", y: "{p.row.min.y + p.row.height() * 0.28}",
                                text_anchor: "end",
                                if let Some(overlap) = p.overlap.as_ref() {
                                    tspan { class: "port-overlap", "{overlap} · " }
                                }
                                "{p.fill}"
                            }
                        }
                    }
                }
            }
        }
        if shows {
            OutputPixels { face: face.clone(), w, h, zoom, frame }
            svg { class: "face-svg", view_box: "0 0 {w} {h}", preserve_aspect_ratio: "none",
                for p in face.ports.iter() {
                    g { key: "{p.key}",
                        for (i, run) in p.runs.iter().enumerate() {
                            g { key: "{i}", class: "run {run_class(run.emphasis)}",
                                for (j, r) in run.rects.iter().enumerate() {
                                    rect {
                                        key: "{j}",
                                        x: "{r.min.x}", y: "{r.min.y}",
                                        width: "{r.width()}", height: "{r.height()}",
                                        rx: "{(r.height() * 0.2).min(3.0 / zoom)}",
                                    }
                                }
                            }
                        }
                        for (j, r) in p.overlaps.iter().enumerate() {
                            rect {
                                key: "o{j}",
                                class: "run-overlap",
                                x: "{r.min.x}", y: "{r.min.y}",
                                width: "{r.width()}", height: "{r.height()}",
                                rx: "{(r.height() * 0.2).min(3.0 / zoom)}",
                            }
                        }
                        for i in p.marked.iter() {
                            if let Some(r) = p.grid.rect(*i) {
                                rect {
                                    key: "m{i}",
                                    class: "px-mark",
                                    x: "{r.min.x - 1.5 / zoom}", y: "{r.min.y - 1.5 / zoom}",
                                    width: "{r.width() + 3.0 / zoom}", height: "{r.height() + 3.0 / zoom}",
                                    rx: "{1.5 / zoom}",
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn run_class(e: Emphasis) -> &'static str {
    match e {
        Emphasis::Lit => "lit",
        Emphasis::Quiet => "unpatched",
        other => emphasis_class(other),
    }
}

/// Every port's pixels, as the board shows them now. Its own component, so
/// a tick of the board redraws only this canvas.
#[component]
fn OutputPixels(
    face: UiOutputFace,
    w: f64,
    h: f64,
    zoom: f64,
    frame: ReadSignal<BoardFrame>,
) -> Element {
    let id = use_hook(|| canvas_id("pixels"));
    // Read here, so a new frame draws again; the drawing reads the latest.
    let _ = frame.read();
    let canvas = id.clone();
    spawn(async move {
        let mut raster = Raster::new(w, h, zoom);
        let frame = frame.peek();
        for p in &face.ports {
            let Some(pixels) = frame.port(&face.output, &p.key) else {
                continue;
            };
            let gap = if p.grid.side * zoom >= GAP_PIXEL_PX {
                1.0
            } else {
                0.0
            };
            for (i, px) in pixels.iter().enumerate() {
                if let Some(r) = p.grid.rect(i as u32) {
                    raster.rect((r.min.x, r.min.y), (r.max.x, r.max.y), gap, rgb(*px));
                }
            }
        }
        drop(frame);
        put_when_mounted(raster, canvas).await;
    });
    rsx! {
        canvas { id: "{id}", class: "face-canvas" }
    }
}
