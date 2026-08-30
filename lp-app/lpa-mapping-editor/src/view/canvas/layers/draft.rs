//! The drawing-tool draft preview: the chain link from the previous object,
//! the draft polyline, the polygon's implicit closing edge and close target,
//! the ghost lamps the tool would commit, and the placed draft vertices.

use dioxus::prelude::*;

use crate::view::canvas::palette::SELECTION_COLOR;

pub(crate) struct DraftLayerInput<'a> {
    /// Effective doc→screen scale (`camera.scale × placement.s`).
    pub eff: f32,
    pub radius: f32,
    pub chain_from: Option<[f32; 2]>,
    pub draft_points: &'a [[f32; 2]],
    pub draft_ghosts: &'a [[f32; 2]],
    /// The polygon draft's first vertex, once a click on it would CLOSE the
    /// outline. `None` for the path tool and for a draft too short to close.
    pub draft_close_target: Option<[f32; 2]>,
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
        // The closing edge a polygon HAS but its author has not drawn: from
        // the last placed vertex back to the first, in a lighter dash than
        // the drawn chain so "this edge is implied" reads before the tooltip
        // does.
        if let (Some(target), Some(last)) = (input.draft_close_target, input.draft_points.last()) {
            line {
                class: "lpme-draft-close",
                x1: "{last[0]}",
                y1: "{last[1]}",
                x2: "{target[0]}",
                y2: "{target[1]}",
                stroke_width: "{1.2 / eff}",
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
        // The close target, ringed: the one vertex a click means something
        // different on. Drawn last so it reads over its own draft dot.
        if let Some(target) = input.draft_close_target {
            circle {
                class: "lpme-draft-target",
                cx: "{target[0]}",
                cy: "{target[1]}",
                r: "{7.0 / eff}",
                stroke_width: "{1.4 / eff}",
            }
        }
    }
}
