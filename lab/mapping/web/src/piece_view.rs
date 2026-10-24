//! The piece (plan D4): the install as it is really wired, lit by what the
//! board sends, so a hand patch can be walked with no hardware. It stands
//! where a person would look up at the dome: each lamp takes the colour of
//! the pixel it is really on (`Piece::lamp`), drawn as Studio's live view
//! draws a fixture (`lamp_cells.rs`).
//!
//! The lab's page mounts it at the foot of the right column, under the
//! inspector; Studio has the real install instead. It
//! shows the selection's fixture (a span's object's), or the first
//! fixture the piece wires.

use dioxus::prelude::*;
use lab_mapping_model::{BoardFrame, NodeId, PatchTarget, Target};

use crate::dense_canvas::{Raster, canvas_id, put_when_mounted};
use crate::faces::{DRAFT_PITCH, lamp_radius_px, rgb};
use crate::graph_editor::GraphEditor;
use crate::lamp_cells::CellMap;

/// How wide the piece is drawn, in CSS pixels: the right column's width
/// inside its padding.
const WIDTH: f64 = 252.0;

#[component]
pub fn PieceView(ed: GraphEditor, frame: ReadSignal<BoardFrame>) -> Element {
    let id = use_hook(|| canvas_id("piece"));
    let e = ed.read();
    let Some(piece) = e.piece.as_ref() else {
        return rsx! {};
    };
    let p = &e.project;
    let shown: Option<NodeId> = e
        .selection
        .first()
        .and_then(|t| match t {
            // A span shows the fixture of the object on it.
            Target::Span {
                object: Some(o), ..
            } => p.fixture_of_place(&PatchTarget::Object(o.clone())),
            t => p.fixture_of_target(t),
        })
        .or_else(|| {
            p.fixtures()
                .iter()
                .find(|n| {
                    p.objects_in(&n.id)
                        .iter()
                        .any(|o| piece.landing(&o.id, 0).is_some())
                })
                .map(|n| n.id.clone())
        });
    let Some(fixture) = shown else {
        return rsx! {};
    };
    let Some(size) = p.fixture(&fixture).map(|f| f.size) else {
        return rsx! {};
    };
    let name = p.node_name(&fixture);
    let wired_as = piece.wired_as.clone();
    let revision = p.revision();
    drop(e);
    let zoom = WIDTH / size.x.max(1e-9);
    let height = size.y * zoom;
    let _ = frame.read();
    let canvas = id.clone();
    let _ = revision;
    spawn(async move {
        let e = ed.peek();
        let Some(piece) = e.piece.as_ref() else {
            return;
        };
        let frame = frame.peek();
        let units = e.project.frame(&fixture);
        let mut raster = Raster::new(size.x, size.y, zoom);
        let mut seeds = Vec::new();
        let mut colours = Vec::new();
        for o in e.project.objects_in(&fixture) {
            for (i, l) in o.lamps.iter().enumerate() {
                let at = units.to_local(l.pos);
                seeds.push((at.x * raster.k, at.y * raster.k));
                colours.push(match piece.lamp(&frame, &o.id, i as u32) {
                    Some(c) => rgb(c),
                    None => [156.0, 156.0, 171.0, 0.2],
                });
            }
        }
        let r = lamp_radius_px(zoom, DRAFT_PITCH) * crate::dense_canvas::device_pixel_ratio();
        CellMap::new(raster.w, raster.h, &seeds, r).paint(&mut raster, &colours);
        drop((e, frame));
        put_when_mounted(raster, canvas).await;
    });
    rsx! {
        div { class: "piece-view",
            div { class: "piece-title", "Hardware preview · {name}" }
            div { class: "piece-box", style: "aspect-ratio: {WIDTH} / {height}",
                canvas { id: "{id}", class: "face-canvas" }
            }
            div { class: "piece-note", "Wired as {wired_as}" }
        }
    }
}
