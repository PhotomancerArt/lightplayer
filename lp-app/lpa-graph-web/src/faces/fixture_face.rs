//! A fixture's face. While you work in it, the mapping editor in place:
//! what's drawn in it (each object's line, where its data enters and which
//! way it runs) and its wiring, in SVG in the fixture's own units; faintly
//! behind them the picture its lamps sample, from the board frame
//! (`picture_canvas.rs`); and its lamps as dots on a canvas (Q4), each in
//! its colour in the fixture's control product.
//! Otherwise Studio's live view: its lamps alone, as cells of light
//! (`lamp_cells.rs`).
//!
//! A fixture is drawn in its own units, `size` its box; placed `w` wide
//! (the stack fills the column with it) it is drawn that much bigger,
//! whole.

use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use lab_mapping_model::{
    BoardFrame, Emphasis, FixtureLook, NodeId, Rect, TreeRef, UiFixtureFace, UiObject, UiShape,
    UiWire, Vec2, picture_rect,
};

use crate::picture_canvas::PictureCanvas;

use crate::dense_canvas::{Raster, Stamp, canvas_id, put_when_mounted};
use crate::faces::{emphasis_alpha, emphasis_class, lamp_radius_px, rgb};
use crate::graph_editor::GraphEditor;
use crate::lamp_cells::CellMap;

#[component]
pub fn FixtureFace(
    face: UiFixtureFace,
    /// Its own box, in its own units.
    size: Vec2,
    /// How wide it was placed, in world units.
    w: f64,
    zoom: f64,
    /// Where the fixture's `(0, 0)` is in world units, and the project's
    /// revision: the lamps, read by id, move when either does.
    origin: Vec2,
    revision: u64,
    frame: ReadSignal<BoardFrame>,
    ed: GraphEditor,
) -> Element {
    // Its units on screen: the camera's zoom times how much bigger than its
    // box it is drawn.
    let scale = w / size.x.max(1e-9);
    let zoom = zoom * scale;
    let (w, h) = (size.x, size.y);
    let px = 1.0 / zoom;
    let pitch = face.pitch;
    let lamp_r = lamp_radius_px(zoom, pitch) * px;
    let dimmed: Vec<TreeRef> = face
        .objects
        .iter()
        .filter(|o| o.emphasis == Emphasis::Dimmed)
        .map(|o| o.id.clone())
        .collect();
    let editing = face.look == FixtureLook::Editing;
    // The picture its lamps sample, where the product's fit puts it, as
    // percentages of its box (the face clips what runs past it).
    let picture = face.samples.clone().and_then(|source| {
        let p = frame.read().picture(&source).map(|p| (p.width, p.height))?;
        let (pw, ph) = face.render_size.unwrap_or(p);
        let r = picture_rect(Rect::new(Vec2::default(), Vec2::new(w, h)), pw, ph);
        let pct = |v: f64, of: f64| 100.0 * v / of.max(1e-9);
        Some((
            source,
            format!(
                "left: {}%; top: {}%; width: {}%; height: {}%",
                pct(r.min.x, w),
                pct(r.min.y, h),
                pct(r.width(), w),
                pct(r.height(), h)
            ),
        ))
    });
    rsx! {
        if let Some((source, style)) = picture {
            div { class: "pictures",
                PictureCanvas { source, frame, class: "picture", style }
            }
        }
        svg { class: "face-svg", view_box: "0 0 {w} {h}", preserve_aspect_ratio: "none",
            for (i, wire) in face.wires.iter().enumerate() {
                g { key: "w{i}", class: "wire wire-on", {wire_view(wire, lamp_r, px)} }
            }
            if editing {
                for o in face.objects.iter() {
                    g { key: "{o.id}", class: "obj {emphasis_class(o.emphasis)}", {object_view(o, lamp_r, px)} }
                }
            }
        }
        FixtureLamps { fixture: face.fixture.clone(), look: face.look, w, h, zoom, pitch, origin, revision, dimmed, frame, ed }
        if let Some(why) = face.not_shown.as_ref() {
            div { class: "not-shown", style: "font-size: {12.0 / zoom}px; padding: {10.0 / zoom}px", "{why}" }
        }
    }
}

/// One object: its line, a small arrow for which way its data runs, and a
/// ring where it enters.
fn object_view(o: &UiObject, lamp_r: f64, px: f64) -> Element {
    let stroke = match &o.shape {
        // A path with jumpers: its lit segments as one line, each jumper
        // dashed (wire, no lamps).
        UiShape::Polyline(points) if !o.jumpers.is_empty() => {
            let mut lit = String::new();
            let mut dashed = String::new();
            for (i, w) in points.windows(2).enumerate() {
                let seg = format!("M{},{} L{},{} ", w[0].x, w[0].y, w[1].x, w[1].y);
                if o.jumpers.contains(&i) {
                    dashed.push_str(&seg);
                } else {
                    lit.push_str(&seg);
                }
            }
            rsx! {
                path { class: "stroke", d: "{lit}" }
                path { class: "stroke jumper", d: "{dashed}" }
            }
        }
        UiShape::Polyline(points) => {
            let d: String = points
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{}{},{} ", if i == 0 { "M" } else { "L" }, p.x, p.y))
                .collect();
            rsx! { path { class: "stroke", d: "{d}" } }
        }
        UiShape::Ring { center, radius } => rsx! {
            circle { class: "stroke", cx: "{center.x}", cy: "{center.y}", r: "{radius}" }
        },
        UiShape::Rings { center, radii } => rsx! {
            for r in radii.iter() {
                circle { class: "stroke", cx: "{center.x}", cy: "{center.y}", r: "{r}" }
            }
        },
        UiShape::Outline(points) => {
            let d: String = points
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{}{},{} ", if i == 0 { "M" } else { "L" }, p.x, p.y))
                .collect();
            rsx! { path { class: "stroke", d: "{d}Z" } }
        }
    };
    let arrow = o.arrow.map(|(at, d)| {
        let n = Vec2::new(-d.y, d.x);
        let tip = at + d * (6.0 * px);
        let l = at - d * (4.0 * px) + n * (4.5 * px);
        let r = at - d * (4.0 * px) - n * (4.5 * px);
        format!("M{},{} L{},{} L{},{} Z", tip.x, tip.y, l.x, l.y, r.x, r.y)
    });
    rsx! {
        {stroke}
        if let Some(d) = arrow {
            path { class: "arrow", d: "{d}" }
        }
        if let Some(p) = o.first_lamp {
            circle { class: "lamp-start", cx: "{p.x}", cy: "{p.y}", r: "{lamp_r + 3.0 * px}" }
        }
    }
}

/// One stretch of wiring: a dashed wire from one object's last lamp to the
/// next one's first, or the short lead where a port's data comes in.
fn wire_view(w: &UiWire, lamp_r: f64, px: f64) -> Element {
    let dash = format!("{} {}", 3.0 * px, 3.0 * px);
    let (line, arrow, label) = match w {
        UiWire::Link { from, to } => {
            let d = *to - *from;
            let len = d.length().max(1e-9);
            let u = d * (1.0 / len);
            // Stop short of the lamps, so the wire reads as between them.
            let a = *from + u * (lamp_r + 1.5 * px).min(len / 3.0);
            let tip = *to - u * (lamp_r + 1.5 * px).min(len / 3.0);
            (
                format!("M{},{} L{},{}", a.x, a.y, tip.x, tip.y),
                arrowhead(tip, u, px),
                None,
            )
        }
        UiWire::Entry { at, along } => {
            let tip = *at - *along * (lamp_r + 1.5 * px);
            let tail = *at - *along * (lamp_r + 16.0 * px);
            let label = *at - *along * (lamp_r + 23.0 * px) + Vec2::new(0.0, 3.5 * px);
            (
                format!("M{},{} L{},{}", tail.x, tail.y, tip.x, tip.y),
                arrowhead(tip, *along, px),
                Some(label),
            )
        }
    };
    rsx! {
        path { class: "wire-line", d: "{line}", style: "stroke-dasharray: {dash}" }
        path { class: "wire-arrow", d: "{arrow}" }
        if let Some(at) = label {
            text { class: "wire-in", style: "font-size: {9.0 * px}px", x: "{at.x}", y: "{at.y}", text_anchor: "middle", "in" }
        }
    }
}

/// A small arrowhead with its tip at `tip`, pointing along `u`.
fn arrowhead(tip: Vec2, u: Vec2, px: f64) -> String {
    let n = Vec2::new(-u.y, u.x);
    let l = tip - u * (5.0 * px) + n * (2.6 * px);
    let r = tip - u * (5.0 * px) - n * (2.6 * px);
    format!("M{},{} L{},{} L{},{} Z", tip.x, tip.y, l.x, l.y, r.x, r.y)
}

/// Every lamp of the fixture, in its control product's colour, from the
/// board frame: a dot while editing, a cell in the live view. A lamp
/// patched to no pixel is fainter; one with no colour (outside its box, or
/// nothing renders for its fixture) a dim hollow dot, or a dim cell.
#[component]
fn FixtureLamps(
    fixture: NodeId,
    look: FixtureLook,
    w: f64,
    h: f64,
    zoom: f64,
    /// Its usual lamp spacing, in its own units: lamps are drawn a share
    /// of it.
    pitch: f64,
    /// Where it was placed: a move draws it again.
    origin: Vec2,
    revision: u64,
    dimmed: Vec<TreeRef>,
    frame: ReadSignal<BoardFrame>,
    ed: GraphEditor,
) -> Element {
    let id = use_hook(|| canvas_id("lamps"));
    // The live view's cells, kept while the layout and zoom stay put.
    let cells = use_hook(|| Rc::new(RefCell::new(None::<(CellsKey, CellMap)>)));
    // Read here, so a new frame draws again; the drawing reads the latest.
    let _ = frame.read();
    let _ = (revision, origin);
    let canvas = id.clone();
    spawn(async move {
        let e = ed.peek();
        let frame = frame.peek();
        // The lamps in the fixture's own units, through the editor's own
        // frame for it: the same placement its lamps were put by.
        let units = e.project.frame(&fixture);
        let r = lamp_radius_px(zoom, pitch) * crate::dense_canvas::device_pixel_ratio();
        let mut raster = Raster::new(w, h, zoom);
        if look == FixtureLook::Live {
            let mut seeds: Vec<(f64, f64)> = Vec::new();
            let mut colours: Vec<[f64; 4]> = Vec::new();
            for o in e.project.segments_in(&fixture) {
                let dim = if dimmed.contains(&o.id) {
                    emphasis_alpha(Emphasis::Dimmed)
                } else {
                    1.0
                };
                for (i, (l, c)) in o
                    .lamps
                    .iter()
                    .zip(frame.object_lamps(&o.id, o.lamps.len()))
                    .enumerate()
                {
                    let at = units.to_local(l.pos);
                    seeds.push((at.x * raster.k, at.y * raster.k));
                    colours.push(match c {
                        Some(c) => {
                            let mut c = rgb(c);
                            c[3] = dim
                                * if frame.lands(&o.id, i as u32) {
                                    1.0
                                } else {
                                    0.45
                                };
                            c
                        }
                        None => [156.0, 156.0, 171.0, 0.25 * dim],
                    });
                }
            }
            let key = CellsKey::new(revision, origin, &raster);
            let mut kept = cells.borrow_mut();
            if kept.as_ref().is_none_or(|(k, _)| *k != key) {
                *kept = Some((key, CellMap::new(raster.w, raster.h, &seeds, r)));
            }
            if let Some((_, map)) = kept.as_ref() {
                map.paint(&mut raster, &colours);
            }
            drop((kept, e, frame));
            put_when_mounted(raster, canvas).await;
            return;
        }
        let stamp = Stamp::disc(r, Some([255.0, 255.0, 255.0, 0.3]));
        let dark = [156.0, 156.0, 171.0, 1.0];
        for o in e.project.segments_in(&fixture) {
            let dim = if dimmed.contains(&o.id) {
                emphasis_alpha(Emphasis::Dimmed)
            } else {
                1.0
            };
            for (i, (l, c)) in o
                .lamps
                .iter()
                .zip(frame.object_lamps(&o.id, o.lamps.len()))
                .enumerate()
            {
                let at = units.to_local(l.pos);
                match c {
                    Some(c) => {
                        let mut c = rgb(c);
                        let patched = frame.lands(&o.id, i as u32);
                        c[3] = dim * if patched { 1.0 } else { 0.45 };
                        raster.stamp(at.x, at.y, &stamp, c);
                    }
                    None => {
                        let mut c = dark;
                        c[3] = 0.5 * dim;
                        raster.ring(at.x, at.y, r, 1.0, c);
                    }
                }
            }
        }
        drop((e, frame));
        put_when_mounted(raster, canvas).await;
    });
    rsx! {
        canvas { id: "{id}", class: "face-canvas" }
    }
}

/// What a fixture's cells were worked out for: its lamps (by the project's
/// revision and where it was placed) and its raster.
#[derive(Clone, Copy, PartialEq)]
struct CellsKey {
    revision: u64,
    origin: (u64, u64),
    raster: (u32, u32),
    k: u64,
}

impl CellsKey {
    fn new(revision: u64, origin: Vec2, raster: &Raster) -> CellsKey {
        CellsKey {
            revision,
            origin: (origin.x.to_bits(), origin.y.to_bits()),
            raster: (raster.w, raster.h),
            k: raster.k.to_bits(),
        }
    }
}
