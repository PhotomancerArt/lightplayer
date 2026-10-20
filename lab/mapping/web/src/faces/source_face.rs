//! A shader, or another render source: a card saying what it is and the
//! channel it writes its picture to, and below that the picture itself,
//! live from the board frame when there is one (plan D11).

use dioxus::prelude::*;
use lab_mapping_model::card_geometry::card_row;
use lab_mapping_model::{BoardFrame, DetailLevel, UiSourceFace};

use crate::faces::{CardTitle, card_font};
use crate::picture_canvas::PictureCanvas;

#[component]
pub fn SourceFace(
    face: UiSourceFace,
    name: String,
    w: f64,
    h: f64,
    detail: DetailLevel,
    frame: ReadSignal<BoardFrame>,
) -> Element {
    let row = card_row(0, w);
    let font = card_font();
    // The picture fills the card under its row, keeping its shape.
    let top = 100.0 * row.max.y / h.max(1e-9);
    let picture_style = format!(
        "left: 0; top: {top}%; width: 100%; height: {}%",
        100.0 - top
    );
    rsx! {
        if detail > DetailLevel::Dot {
            PictureCanvas {
                source: face.source.clone(),
                frame,
                class: "source-picture",
                style: picture_style,
            }
        }
        svg { class: "face-svg", view_box: "0 0 {w} {h}", preserve_aspect_ratio: "none",
            if detail > DetailLevel::Dot {
                line { class: "port-sep", x1: "0", y1: "{row.min.y}", x2: "{w}", y2: "{row.min.y}" }
            }
            if detail == DetailLevel::Full {
                CardTitle { name }
                text {
                    class: "port-name",
                    style: "font-size: {font}px",
                    x: "{font * 0.7}", y: "{row.min.y + row.height() * 0.28}",
                    "{face.kind}"
                }
                text {
                    class: "port-fill",
                    style: "font-size: {font * 0.9}px",
                    x: "{w - font * 0.7}", y: "{row.min.y + row.height() * 0.28}",
                    text_anchor: "end",
                    "{face.writes}"
                }
            }
        }
    }
}
