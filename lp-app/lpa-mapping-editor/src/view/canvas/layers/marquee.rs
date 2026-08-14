//! The marquee rectangle. Doc-space by design (Q6): under a rotated
//! placement it renders rotated with the document — accepted v1 behavior,
//! tracked as debt.

use dioxus::prelude::*;

pub(crate) fn marquee_layer(eff: f32, marquee_rect: Option<([f32; 2], [f32; 2])>) -> Element {
    rsx! {
        if let Some((origin, size)) = marquee_rect {
            rect {
                class: "lpme-marquee",
                x: "{origin[0]}",
                y: "{origin[1]}",
                width: "{size[0]}",
                height: "{size[1]}",
                stroke_width: "{1.2 / eff}",
            }
        }
    }
}
