//! A node the lab doesn't simulate: a card that says what it is.

use dioxus::prelude::*;
use lab_mapping_model::card_geometry::card_row;
use lab_mapping_model::{DetailLevel, UiPlainFace};

use crate::faces::{CardTitle, card_font};

#[component]
pub fn PlainFace(face: UiPlainFace, name: String, w: f64, h: f64, detail: DetailLevel) -> Element {
    let row = card_row(0, w);
    let font = card_font();
    rsx! {
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
            }
        }
    }
}
