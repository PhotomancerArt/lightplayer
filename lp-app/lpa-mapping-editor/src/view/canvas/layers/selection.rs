//! Selection furniture: the bbox outline, corner resize handles, the
//! repeat-center crosshair, and path vertex handles.

use dioxus::prelude::*;
use lpc_mapping::Bounds2d;

use crate::view::canvas::canvas_anchor::capture_pointer;
use crate::view::canvas::{CanvasDrag, CanvasInteract, event_doc_point, secondary_button};

pub(crate) struct SelectionLayerInput<'a> {
    pub interact: CanvasInteract,
    /// Effective doc→screen scale (`camera.scale × placement.s`).
    pub eff: f32,
    pub selection_bounds: Option<Bounds2d>,
    pub selection_margin: f32,
    pub handle_half: f32,
    pub repeat_center: Option<[f32; 2]>,
    pub vertex_points: &'a [[f32; 2]],
    pub selected_vertex: Option<usize>,
}

pub(crate) fn selection_layer(input: &SelectionLayerInput<'_>) -> Element {
    let interact = input.interact;
    let eff = input.eff;
    let selection_margin = input.selection_margin;
    let handle_half = input.handle_half;
    rsx! {
        if let Some(bounds) = input.selection_bounds {
            rect {
                class: "lpme-sel-outline",
                x: "{bounds.min_x - selection_margin}",
                y: "{bounds.min_y - selection_margin}",
                width: "{bounds.width + 2.0 * selection_margin}",
                height: "{bounds.height + 2.0 * selection_margin}",
                rx: "{6.0 / eff}",
                stroke_width: "{1.5 / eff}",
            }
            for (name, corner_x, corner_y, anchor_x, anchor_y) in [
                (
                    "tl",
                    bounds.min_x - selection_margin,
                    bounds.min_y - selection_margin,
                    bounds.min_x + bounds.width + selection_margin,
                    bounds.min_y + bounds.height + selection_margin,
                ),
                (
                    "tr",
                    bounds.min_x + bounds.width + selection_margin,
                    bounds.min_y - selection_margin,
                    bounds.min_x - selection_margin,
                    bounds.min_y + bounds.height + selection_margin,
                ),
                (
                    "bl",
                    bounds.min_x - selection_margin,
                    bounds.min_y + bounds.height + selection_margin,
                    bounds.min_x + bounds.width + selection_margin,
                    bounds.min_y - selection_margin,
                ),
                (
                    "br",
                    bounds.min_x + bounds.width + selection_margin,
                    bounds.min_y + bounds.height + selection_margin,
                    bounds.min_x - selection_margin,
                    bounds.min_y - selection_margin,
                ),
            ] {
                rect {
                    key: "h{name}",
                    class: "lpme-handle",
                    x: "{corner_x - handle_half}",
                    y: "{corner_y - handle_half}",
                    width: "{2.0 * handle_half}",
                    height: "{2.0 * handle_half}",
                    stroke_width: "{1.4 / eff}",
                    onpointerdown: move |evt| {
                        if secondary_button(&evt) {
                            return;
                        }
                        evt.stop_propagation();
                        capture_pointer(&evt);
                        let doc_point = event_doc_point(&interact, &evt);
                        let mut session = interact.session;
                        session.write().begin_gesture();
                        let mut drag = interact.drag;
                        drag.set(Some(CanvasDrag::Resize {
                            anchor: [anchor_x, anchor_y],
                            start: doc_point,
                            moved: false,
                        }));
                    },
                }
            }
        }
        // The point a selected repeat turns about: a crosshair, not a
        // handle — it moves with the object (drag) and by its own
        // number fields, and a draggable dot here would collide with
        // the marquee.
        if let Some(center) = input.repeat_center {
            {
                let arm = 9.0 / eff;
                let stroke = 1.4 / eff;
                rsx! {
                    circle {
                        class: "lpme-repeat-center",
                        cx: "{center[0]}",
                        cy: "{center[1]}",
                        r: "{arm * 0.55}",
                        stroke_width: "{stroke}",
                    }
                    line {
                        class: "lpme-repeat-center",
                        x1: "{center[0] - arm}",
                        y1: "{center[1]}",
                        x2: "{center[0] + arm}",
                        y2: "{center[1]}",
                        stroke_width: "{stroke}",
                    }
                    line {
                        class: "lpme-repeat-center",
                        x1: "{center[0]}",
                        y1: "{center[1] - arm}",
                        x2: "{center[0]}",
                        y2: "{center[1] + arm}",
                        stroke_width: "{stroke}",
                    }
                }
            }
        }
        for (vertex_index, point) in input.vertex_points.iter().enumerate() {
            {
                let hot = input.selected_vertex == Some(vertex_index);
                let half = 4.5 / eff;
                rsx! {
                    rect {
                        key: "v{vertex_index}",
                        class: if hot { "lpme-vertex lpme-vertex-hot" } else { "lpme-vertex" },
                        x: "{point[0] - half}",
                        y: "{point[1] - half}",
                        width: "{2.0 * half}",
                        height: "{2.0 * half}",
                        stroke_width: "{1.4 / eff}",
                        onpointerdown: move |evt| {
                            if secondary_button(&evt) {
                                return;
                            }
                            evt.stop_propagation();
                            capture_pointer(&evt);
                            {
                                let mut session = interact.session;
                                let mut s = session.write();
                                s.selection.vertex = Some(vertex_index);
                                s.begin_gesture();
                            }
                            let mut drag = interact.drag;
                            drag.set(Some(CanvasDrag::Vertex {
                                index: vertex_index,
                                moved: false,
                            }));
                        },
                    }
                }
            }
        }
    }
}
