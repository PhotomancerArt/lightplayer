//! The path-draft preview: the chain link from the previous object, the
//! draft polyline, resolved ghost lamps, and the placed draft vertices.

use dioxus::prelude::*;

use crate::view::canvas::palette::SELECTION_COLOR;

pub(crate) struct DraftLayerInput<'a> {
    /// Effective doc→screen scale (`camera.scale × placement.s`).
    pub eff: f32,
    pub radius: f32,
    pub chain_from: Option<[f32; 2]>,
    pub draft_points: &'a [[f32; 2]],
    pub draft_ghosts: &'a [[f32; 2]],
}

pub(crate) fn draft_layer(input: &DraftLayerInput<'_>) -> Element {
    let eff = input.eff;
    let radius = input.radius;
    rsx! {
        if let (Some(from), Some(first)) = (input.chain_from, input.draft_points.first()) {
            line {
                class: "lpme-arrow-chain",
                x1: "{from[0]}",
                y1: "{from[1]}",
                x2: "{first[0]}",
                y2: "{first[1]}",
                stroke_width: "{(radius * 0.14).clamp(0.4, 1.5)}",
            }
        }
        if input.draft_points.len() >= 2 {
            polyline {
                class: "lpme-draft",
                points: input.draft_points.iter().map(|p| format!("{},{}", p[0], p[1])).collect::<Vec<_>>().join(" "),
                stroke_width: "{1.5 / eff}",
            }
        }
        for (index, ghost) in input.draft_ghosts.iter().enumerate() {
            circle {
                key: "g{index}",
                cx: "{ghost[0]}",
                cy: "{ghost[1]}",
                r: "{radius}",
                fill: SELECTION_COLOR,
                opacity: "0.35",
            }
        }
        for (index, point) in input.draft_points.iter().enumerate() {
            circle {
                key: "dp{index}",
                cx: "{point[0]}",
                cy: "{point[1]}",
                r: "{3.5 / eff}",
                fill: SELECTION_COLOR,
            }
        }
    }
}
