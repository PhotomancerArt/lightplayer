//! One node on the canvas: a flat, keyed DOM element in the world layer,
//! placed by `transform: translate(x, y)` in world units, as wide and tall
//! as core declares. Its props are world-space only, never the camera, so
//! panning never renders it again; zooming does, a step at a time, for the
//! things that stay one size on screen.
//!
//! A card (an output, a shader, a plain node) is filled; a module and a
//! fixture are see-through frames whose title tabs sit in the overlay at
//! one size. A relayout eases each node to its new place (`lab.css`).

use dioxus::prelude::*;
use lab_mapping_model::{BoardFrame, DetailLevel, FrameKind, Rect, UiCanvasNode, UiNodeFace, Vec2};

use crate::faces::{FixtureFace, OutputFace, PlainFace, SourceFace, emphasis_class};
use crate::graph_editor::GraphEditor;

#[component]
pub fn NodeCard(
    node: UiCanvasNode,
    rect: Rect,
    zoom: f64,
    revision: u64,
    frame: ReadSignal<BoardFrame>,
    ed: GraphEditor,
) -> Element {
    let (w, h) = (rect.width(), rect.height());
    let frame_class = match node.frame {
        FrameKind::Card => "card",
        FrameKind::Module => "frame module",
        FrameKind::Fixture => "frame fixture",
    };
    let emphasis = emphasis_class(node.emphasis);
    let selected = if node.selected { " selected" } else { "" };
    let style = format!(
        "transform: translate({}px, {}px); width: {w}px; height: {h}px",
        rect.min.x, rect.min.y
    );
    let name = node.header.name.clone();
    let face = match (&node.face, node.detail) {
        (Some(_), DetailLevel::Dot) if node.frame == FrameKind::Card => None,
        (Some(face), detail) => Some((face.clone(), detail)),
        (None, _) => None,
    };
    rsx! {
        div { class: "node {frame_class}{selected}", style: "{style}",
            // The frame's box wears the emphasis; a fixture's face carries
            // its own, object by object.
            div { class: "node-box {emphasis}" }
            if let Some((face, detail)) = face {
                div { class: if node.frame == FrameKind::Card { "node-face {emphasis}" } else { "node-face" },
                    match face {
                        UiNodeFace::Fixture(face) => rsx! {
                            FixtureFace {
                                face,
                                size: node.size,
                                w,
                                zoom,
                                origin: Vec2::new(rect.min.x, rect.min.y),
                                revision,
                                frame,
                                ed: ed.clone(),
                            }
                        },
                        UiNodeFace::Output(face) => rsx! {
                            OutputFace { face, name, w, h, detail, zoom, frame }
                        },
                        UiNodeFace::Source(face) => rsx! {
                            SourceFace { face, name, w, h, detail }
                        },
                        UiNodeFace::Plain(face) => rsx! {
                            PlainFace { face, name, w, h, detail }
                        },
                    }
                }
            }
        }
    }
}
