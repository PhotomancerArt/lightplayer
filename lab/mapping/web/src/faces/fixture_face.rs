//! A fixture's face: the mapping editor, in place. What's drawn in it (each
//! object's line, where its data enters and which way it runs), its
//! wiring, and faintly behind them the picture its lamps sample — in SVG,
//! in the fixture's own units — and its lamps on a canvas (Q4), each in its
//! colour in the fixture's control product.

use dioxus::prelude::*;
use lab_mapping_model::{
    BoardFrame, Editor, Emphasis, NodeId, ObjectId, UiFixtureFace, UiObject, UiShape, UiWire, Vec2,
};

use crate::dense_canvas::{Raster, Stamp, canvas_id, put_when_mounted};
use crate::faces::{emphasis_alpha, emphasis_class, lamp_radius_px, rgb};

#[component]
pub fn FixtureFace(
    face: UiFixtureFace,
    w: f64,
    h: f64,
    zoom: f64,
    /// Where the fixture's `(0, 0)` is in world units, and the project's
    /// revision: the lamps, read by id, move when either does.
    origin: Vec2,
    revision: u64,
    frame: Memo<BoardFrame>,
    ed: Signal<Editor>,
) -> Element {
    let px = 1.0 / zoom;
    let lamp_r = lamp_radius_px(zoom) * px;
    let dimmed: Vec<ObjectId> = face
        .objects
        .iter()
        .filter(|o| o.emphasis == Emphasis::Dimmed)
        .map(|o| o.id.clone())
        .collect();
    let wire_class = if face.wires_emphasis == Emphasis::Lit {
        "wire wire-on"
    } else {
        "wire"
    };
    rsx! {
        svg { class: "face-svg", view_box: "0 0 {w} {h}", preserve_aspect_ratio: "none",
            g { class: "pictures",
                for (i, (r, c)) in face.picture.iter().enumerate() {
                    rect {
                        key: "{i}",
                        x: "{r.min.x}", y: "{r.min.y}",
                        width: "{r.width() + 0.5 * px}", height: "{r.height() + 0.5 * px}",
                        fill: "rgb({c.r},{c.g},{c.b})",
                    }
                }
            }
            for (i, wire) in face.wires.iter().enumerate() {
                g { key: "w{i}", class: "{wire_class}", {wire_view(wire, lamp_r, px)} }
            }
            for o in face.objects.iter() {
                g { key: "{o.id}", class: "obj {emphasis_class(o.emphasis)}", {object_view(o, lamp_r, px)} }
            }
        }
        FixtureLamps { fixture: face.fixture.clone(), w, h, zoom, origin, revision, dimmed, frame, ed }
    }
}

/// One object: its line, a small arrow for which way its data runs, and a
/// ring where it enters.
fn object_view(o: &UiObject, lamp_r: f64, px: f64) -> Element {
    let stroke = match &o.shape {
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
/// board frame. A lamp patched to no pixel is fainter; one with no colour
/// (outside its box, or nothing renders for its fixture) a dim hollow dot.
#[component]
fn FixtureLamps(
    fixture: NodeId,
    w: f64,
    h: f64,
    zoom: f64,
    origin: Vec2,
    revision: u64,
    dimmed: Vec<ObjectId>,
    frame: Memo<BoardFrame>,
    ed: Signal<Editor>,
) -> Element {
    let id = use_hook(|| canvas_id("lamps"));
    // Read here, so a new frame draws again; the drawing reads the latest.
    let _ = frame.read();
    let _ = revision;
    let canvas = id.clone();
    spawn(async move {
        let e = ed.peek();
        let frame = frame.peek();
        let r = lamp_radius_px(zoom) * crate::dense_canvas::device_pixel_ratio();
        let mut raster = Raster::new(w, h, zoom);
        let stamp = Stamp::disc(r, Some([255.0, 255.0, 255.0, 0.3]));
        let dark = [156.0, 156.0, 171.0, 1.0];
        for o in e.project.objects_in(&fixture) {
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
                let at = l.pos - origin;
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
