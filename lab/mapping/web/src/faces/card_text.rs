//! A card's words, in world units so they zoom with the card: the title in
//! its header, a row's name and fill. Shown only at full detail (core says
//! when, from how big the text would be on screen).

use dioxus::prelude::*;
use lab_mapping_model::card_geometry::CARD_HEADER;

/// The card font's size, in world units: 4.5 px at the zoom where words
/// first show.
pub fn card_font() -> f64 {
    3.9
}

#[component]
pub fn CardTitle(name: String) -> Element {
    let font = card_font();
    rsx! {
        text {
            class: "card-title",
            style: "font-size: {font * 1.1}px",
            x: "{font * 0.7}",
            y: "{(CARD_HEADER * 0.62).min(font * 1.75)}",
            "{name}"
        }
    }
}
