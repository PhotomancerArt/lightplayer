//! Document content layers: reference image, authored canvas rect, the
//! fit-preview overlay, object bodies (aligned outline + lamp cells),
//! wiring arrows, repeat ghosts, jumper gap lines, path hit lines, lamps
//! (with hit rings), and wiring numbers.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use lpc_mapping::{Bounds2d, ResolvedMap2d};

use crate::editor_core::map_selection::MapSelection;
use crate::editor_core::map_tool::MapTool;
use crate::editor_core::view_geometry::MapArrowOverlay;
use crate::view::canvas::layers::bodies::{ObjectBody, loops_path_d};
use crate::view::canvas::layers::hull::hull_path_d;
use crate::view::canvas::palette::{OBJECT_COLORS, SELECTION_COLOR, object_color};
use crate::view::canvas::{CanvasInteract, secondary_button, select_and_start_move};
use crate::view::reference::ReferenceImage;
use crate::view::view_options::EditorViewOptions;

/// Everything the doc layers render from, derived once by the canvas.
pub(crate) struct DocLayersInput<'a> {
    pub interact: CanvasInteract,
    pub opts: EditorViewOptions,
    pub reference: Option<&'a ReferenceImage>,
    pub canvas_rect: Option<Bounds2d>,
    pub fit_rect: Option<Bounds2d>,
    pub arrows: Option<&'a MapArrowOverlay>,
    pub ghost_outlines: &'a [Vec<[f32; 2]>],
    pub gap_segments: &'a [[[f32; 2]; 2]],
    pub path_objects: &'a [(usize, Vec<[f32; 2]>)],
    /// Each object instance's BODY — the aligned outline and lamp cells the
    /// Arrange view draws, derived from THIS document so the properties
    /// panel's `align` control repaints as it is edited.
    pub bodies: &'a [ObjectBody],
    pub resolved: &'a ResolvedMap2d,
    /// Each object's authored strand as `(start, count)` — the wiring
    /// numbers' coverage (arrows are pre-filtered to the same grain).
    pub annotation_spans: &'a [(u32, u32)],
    pub selection: &'a MapSelection,
    /// Objects rendering instance-by-instance (selected/scoped repeats).
    pub tessellating: &'a BTreeSet<usize>,
    pub scoped_object: Option<usize>,
    /// Per-span `(start, count, instance)` for the tessellating objects.
    pub span_instances: &'a [(u32, u32, usize)],
    pub tool_is_select: bool,
    /// Effective doc→screen scale (`camera.scale × placement.s`) —
    /// screen-constant sizing divides by this.
    pub eff: f32,
    pub radius: f32,
    pub hit_radius: f32,
    pub show_numbers: bool,
}

pub(crate) fn doc_layers(input: &DocLayersInput<'_>) -> Element {
    let interact = input.interact;
    let eff = input.eff;
    let radius = input.radius;
    let hit_radius = input.hit_radius;
    let tool_is_select = input.tool_is_select;
    let span_instances = input.span_instances;
    // A body wears its object's colour — or, while a repeat tessellates,
    // its instance's hue, exactly as that instance's lamp dots do.
    let body_color = |body: &ObjectBody| -> String {
        if input.tessellating.contains(&body.object) {
            OBJECT_COLORS[(body.object + body.instance) % OBJECT_COLORS.len()].to_string()
        } else {
            object_color(body.object).to_string()
        }
    };
    // While DESCENDED into a repeat, the non-primary instances are inert
    // previews (the dots dim the same way).
    let body_inert =
        |body: &ObjectBody| input.scoped_object == Some(body.object) && body.instance > 0;
    let instance_of = |lamp_index: u32| -> Option<usize> {
        let slot = span_instances.partition_point(|(start, count, _)| start + count <= lamp_index);
        span_instances
            .get(slot)
            .filter(|(start, _, _)| *start <= lamp_index)
            .map(|(_, _, instance)| *instance)
    };
    rsx! {
        // Tracing layer: under everything authored, over the grid.
        // Explicit width/height — a viewBox-only SVG has no usable
        // intrinsic size (see `ReferenceImage::size`). `<image>`
        // executes no scripts; foreign SVG stays out of the DOM.
        if let Some(image) = input.reference.filter(|_| input.opts.reference) {
            // dioxus-html's svg `image` element carries no typed
            // attributes; quoted names emit them verbatim.
            image {
                "href": "{image.data_url}",
                "x": "0",
                "y": "0",
                "width": "{image.size[0]}",
                "height": "{image.size[1]}",
                "opacity": "{image.opacity}",
                "pointer-events": "none",
            }
        }
        if let Some(rect) = input.canvas_rect {
            rect {
                class: "lpme-canvas-rect",
                x: "{rect.min_x}",
                y: "{rect.min_y}",
                width: "{rect.width}",
                height: "{rect.height}",
                stroke_width: "{1.5 / eff}",
            }
        }
        if let Some(rect) = input.fit_rect {
            rect {
                class: "lpme-fit-rect",
                x: "{rect.min_x}",
                y: "{rect.min_y}",
                width: "{rect.width}",
                height: "{rect.height}",
                stroke_width: "{2.0 / eff}",
            }
            text {
                class: "lpme-fit-label",
                x: "{rect.min_x + 6.0 / eff}",
                y: "{rect.min_y - 8.0 / eff}",
                font_size: "{12.0 / eff}",
                "texture frame (square target)"
            }
            text {
                class: "lpme-fit-label",
                x: "{rect.min_x + 6.0 / eff}",
                y: "{rect.min_y + 16.0 / eff}",
                font_size: "{11.0 / eff}",
                "0,0"
            }
            text {
                class: "lpme-fit-label",
                x: "{rect.min_x + rect.width - 30.0 / eff}",
                y: "{rect.min_y + 16.0 / eff}",
                font_size: "{11.0 / eff}",
                "1,0"
            }
            text {
                class: "lpme-fit-label",
                x: "{rect.min_x + 6.0 / eff}",
                y: "{rect.min_y + rect.height - 8.0 / eff}",
                font_size: "{11.0 / eff}",
                "0,1"
            }
            text {
                class: "lpme-fit-label",
                x: "{rect.min_x + rect.width - 30.0 / eff}",
                y: "{rect.min_y + rect.height - 8.0 / eff}",
                font_size: "{11.0 / eff}",
                "1,1"
            }
        }
        // The object BODIES, under every authored mark and every editing
        // handle: the language the Arrange view speaks — the aligned band
        // the lamp strand sweeps, plus its voronoi lamp cells. Here they are
        // CONTEXT (the dots stay the editing affordance), so the cells go in
        // at reduced opacity and nothing takes pointer events: the canvas
        // root still owns every gesture, and the inline `pointer-events`
        // beats `.lpme-obj-hull`'s own `pointer-events: all`.
        //
        // Outlines first, then cells, so no object's cells are tinted by a
        // neighbour's band fill.
        for (index, body) in input.bodies.iter().enumerate() {
            if !body.outline.is_empty() {
                {
                    let selected = input.selection.object_selected(body.object);
                    let inert = body_inert(body);
                    let color = body_color(body);
                    rsx! {
                        path {
                            key: "body{index}",
                            class: if selected { "lpme-obj-hull lpme-obj-hull-on" } else { "lpme-obj-hull" },
                            d: loops_path_d(&body.outline),
                            // Overlapping strand loops merge and a closed
                            // shape's hole stays a hole.
                            fill_rule: "nonzero",
                            stroke_width: if selected { "{1.4 / eff}" } else { "{1.0 / eff}" },
                            opacity: if inert { "0.5" } else { "1" },
                            // The object's own colour tints the at-rest
                            // stroke (`.lpme-obj-hull` mixes it down).
                            style: "--lpme-obj-c: {color}; pointer-events: none;",
                        }
                    }
                }
            }
        }
        for (index, body) in input.bodies.iter().enumerate() {
            {
                let inert = body_inert(body);
                let color = body_color(body);
                // Calm enough that the lamp dots keep reading through them.
                let fill_opacity = if inert { "0.22" } else { "0.4" };
                rsx! {
                    for cell in body.cells.iter() {
                        path {
                            key: "cell{index}-{cell.lamp}",
                            d: hull_path_d(&cell.polygon),
                            fill: "{color}",
                            fill_opacity,
                            pointer_events: "none",
                        }
                    }
                }
            }
        }
        if let Some(overlay) = input.arrows {
            for (index, seg) in overlay.segs.iter().enumerate() {
                line {
                    key: "{index}",
                    class: if seg.chain { "lpme-arrow-chain" } else { "lpme-arrow-wire" },
                    x1: "{seg.x1}",
                    y1: "{seg.y1}",
                    x2: "{seg.x2}",
                    y2: "{seg.y2}",
                    stroke_width: "{(radius * 0.14).clamp(0.4, 1.5)}",
                    marker_end: if seg.chain { "url(#lpme-arrow-head-chain)" } else { "url(#lpme-arrow-head)" },
                }
            }
        }
        // Where the other instances of a selected repeated path run.
        for (instance, outline) in input.ghost_outlines.iter().enumerate() {
            polyline {
                key: "ghost{instance}",
                class: "lpme-ghost",
                points: outline.iter().map(|p| format!("{},{}", p[0], p[1])).collect::<Vec<_>>().join(" "),
                stroke_width: "{1.2 / eff}",
            }
        }
        // Jumper wire: the segments an author marked inert.
        for (index, segment) in input.gap_segments.iter().enumerate() {
            line {
                key: "gap{index}",
                class: "lpme-gapline",
                x1: "{segment[0][0]}",
                y1: "{segment[0][1]}",
                x2: "{segment[1][0]}",
                y2: "{segment[1][1]}",
                stroke_width: "{(1.6 / eff).max(radius * 0.22)}",
            }
        }
        // Wide invisible hit lines make skinny paths clickable.
        if tool_is_select {
            for (object_index, points) in input.path_objects.iter() {
                {
                    let object_index = *object_index;
                    rsx! {
                        polyline {
                            key: "hit{object_index}",
                            class: "lpme-hitline",
                            points: points.iter().map(|p| format!("{},{}", p[0], p[1])).collect::<Vec<_>>().join(" "),
                            stroke_width: "{14.0 / eff}",
                            onpointerdown: move |evt| {
                                if secondary_button(&evt) {
                                    return;
                                }
                                evt.stop_propagation();
                                select_and_start_move(interact, object_index, &evt);
                            },
                        }
                    }
                }
            }
        }
        for lamp in input
            .resolved
            .lamps
            .iter()
            .filter(|lamp| !input.selection.object_selected(lamp.object as usize))
            .chain(
                input
                    .resolved
                    .lamps
                    .iter()
                    .filter(|lamp| input.selection.object_selected(lamp.object as usize)),
            )
        {
            {
                let object_index = lamp.object as usize;
                let selected = input.selection.object_selected(object_index);
                // Instance-by-instance rendering for a selected /
                // scoped repeat; while scoped, instance 0 (the
                // authored primary) is the only interactive geometry
                // — the rest are inert live previews.
                let instance = input
                    .tessellating
                    .contains(&object_index)
                    .then(|| instance_of(lamp.index))
                    .flatten();
                let inert =
                    input.scoped_object == Some(object_index) && instance.is_some_and(|i| i > 0);
                // The ATTRIBUTE fill is the non-live look (instance
                // hue > object palette); live output
                // rides an inline-style override written by the
                // live-color effect, so color frames never re-render
                // this tree.
                let fill = if let Some(instance) = instance {
                    OBJECT_COLORS[(object_index + instance) % OBJECT_COLORS.len()]
                        .to_string()
                } else {
                    object_color(object_index).to_string()
                };
                rsx! {
                    circle {
                        key: "l{lamp.index}",
                        "data-lamp": "{lamp.index}",
                        cx: "{lamp.pos[0]}",
                        cy: "{lamp.pos[1]}",
                        r: "{radius}",
                        fill,
                        opacity: if inert { "0.6" } else { "1" },
                        pointer_events: if inert { "none" } else { "auto" },
                        stroke: if selected && !inert { SELECTION_COLOR } else { "#000" },
                        stroke_width: if selected && !inert {
                            "{(radius * 0.28).clamp(0.6, 2.4)}"
                        } else {
                            "{(radius * 0.12).clamp(0.2, 1.5)}"
                        },
                        cursor: if inert { "default" } else if tool_is_select { "move" } else { "crosshair" },
                        onpointerdown: move |evt| {
                            if secondary_button(&evt) {
                                return;
                            }
                            if matches!(interact.session.read().tool, MapTool::Select) {
                                evt.stop_propagation();
                                select_and_start_move(interact, object_index, &evt);
                            }
                        },
                        ondoubleclick: move |evt| {
                            // Double-click descends into the group
                            // under the cursor (selection/tree ADR);
                            // a leaf object no-ops.
                            evt.stop_propagation();
                            let mut session = interact.session;
                            let mut s = session.write();
                            if !s.selection.object_selected(object_index) {
                                s.selection.select_only(object_index);
                            }
                            s.descend();
                        },
                    }
                    if hit_radius > radius * 1.15 && !inert {
                        circle {
                            key: "lh{lamp.index}",
                            class: "lpme-lamp-hit",
                            cx: "{lamp.pos[0]}",
                            cy: "{lamp.pos[1]}",
                            r: "{hit_radius}",
                            cursor: if tool_is_select { "move" } else { "crosshair" },
                            onpointerdown: move |evt| {
                                if secondary_button(&evt) {
                                    return;
                                }
                                if matches!(interact.session.read().tool, MapTool::Select) {
                                    evt.stop_propagation();
                                    select_and_start_move(interact, object_index, &evt);
                                }
                            },
                            ondoubleclick: move |evt| {
                                evt.stop_propagation();
                                let mut session = interact.session;
                                let mut s = session.write();
                                if !s.selection.object_selected(object_index) {
                                    s.selection.select_only(object_index);
                                }
                                s.descend();
                            },
                        }
                    }
                }
            }
        }
        if input.show_numbers {
            // Authored grain: numbers cover each object's authored strand
            // only — a repeat's expanded instances stay unnumbered.
            for lamp in input.annotation_spans.iter().flat_map(|&(start, count)| {
                input
                    .resolved
                    .lamps
                    .iter()
                    .skip(start as usize)
                    .take(count as usize)
            }) {
                {
                    let font = if lamp.index + 1 >= 100 {
                        radius * 0.62
                    } else {
                        radius * 0.85
                    };
                    rsx! {
                        text {
                            key: "n{lamp.index}",
                            class: "lpme-lamp-num",
                            x: "{lamp.pos[0]}",
                            y: "{lamp.pos[1] + font * 0.34}",
                            font_size: "{font}",
                            stroke_width: "{font * 0.16}",
                            text_anchor: "middle",
                            "{lamp.index + 1}"
                        }
                    }
                }
            }
        }
    }
}
