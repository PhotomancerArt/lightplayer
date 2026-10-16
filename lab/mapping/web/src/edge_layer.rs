//! Edges, routed by the page and drawn in the overlay, in screen pixels, so
//! heads and labels stay one size (Q3). Core says which edges show and what
//! each says (`UiCanvasEdge`); only the view knows where the cards landed,
//! so it routes each one with `lab-arrows`, lays their labels out together,
//! and hit-tests them, reporting the one under the pointer back to core as
//! a fact (`Editor::set_hovered_edge`).
//!
//! An edge leaves the right side of its source (a card at its first row; a
//! frame level with where it's going, clamped to the frame) and arrives at
//! the left side of its target, from the left: on an output, level with the
//! middle of the port rows it lands on, so two fixtures on one output
//! arrive apart; on anything else, near its top.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lab_arrows::{Arrow, ArrowGeometry, EndDirection, Head, Label, LabelRole, Side};
use lab_mapping_model::card_geometry::{CARD_HEADER, CARD_ROW, card_row};
use lab_mapping_model::{Camera, Editor, Emphasis, NodeId, Rect, UiCanvasEdge, Vec2};

/// How near, in screen pixels, the pointer must be to an edge to hover it.
pub const EDGE_HOVER_PX: f64 = 6.0;
/// The arrowhead where an edge lands: length and width, in screen pixels.
pub const ARROW_LEN_PX: f64 = 7.0;
pub const ARROW_WIDTH_PX: f64 = 7.0;

/// Each shown edge with its arrow on screen. An edge whose ends have no
/// place yet is left out.
pub fn route(
    edges: &[UiCanvasEdge],
    ed: &Editor,
    rects: &BTreeMap<NodeId, Rect>,
    cam: &Camera,
) -> Vec<(UiCanvasEdge, Arrow)> {
    edges
        .iter()
        .filter(|x| x.shown)
        .filter_map(|x| Some((x.clone(), arrow(x, ed, rects, cam)?)))
        .collect()
}

/// The edge under `at` (screen), if one is within [`EDGE_HOVER_PX`]: the
/// nearest. Routed from where the editor was told the nodes are.
pub fn hit(ed: &Editor, at: Vec2) -> Option<(NodeId, NodeId)> {
    let p = lab_arrows::Vec2::new(at.x, at.y);
    let rects = &ed.project.placement().rects;
    route(&ed.view_edges(), ed, rects, &ed.camera)
        .into_iter()
        .map(|(x, a)| {
            let d = lab_arrows::Path::of_cubic(&a.centreline(), false).distance_to(p);
            (x, d)
        })
        .filter(|(_, d)| *d <= EDGE_HOVER_PX)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(x, _)| (x.from, x.to))
}

/// The routed edges, drawn: line, head and labels lying along the line.
/// They fade while the cards ease to new places (`settling`) and come back
/// once they land.
#[component]
pub fn EdgeLayer(edges: Vec<(UiCanvasEdge, Arrow)>, settling: bool) -> Element {
    let arrows: Vec<Arrow> = edges.iter().map(|(_, a)| a.clone()).collect();
    let geometry: Vec<ArrowGeometry> = lab_arrows::layout_arrows(
        &arrows,
        &crate::text_measure::measure,
        &lab_arrows::LayoutOptions::default(),
    );
    rsx! {
        g { class: if settling { "edges settling" } else { "edges" },
            for ((x, _), g) in edges.iter().zip(geometry) {
                g {
                    key: "{x.from}→{x.to}",
                    class: "edge {edge_class(x)}",
                    path { class: "edge-line", d: "{g.path.to_svg()}" }
                    for (j, h) in g.heads.iter().enumerate() {
                        path { key: "h{j}", class: "edge-arrow", d: "{h.to_svg()}" }
                    }
                    for (j, l) in g.labels.iter().enumerate() {
                        if let Some(p) = l.placed {
                            g { key: "l{j}", transform: "{p.svg_transform()}",
                                rect {
                                    class: "edge-label-box",
                                    x: "{p.text_at.x - p.frame.width / 2.0}", y: "{p.text_at.y - p.frame.height / 2.0}",
                                    width: "{p.frame.width}", height: "{p.frame.height}",
                                    rx: "{p.frame.height / 2.0}",
                                }
                                text {
                                    class: if l.role == LabelRole::Primary { "edge-label" } else { "edge-channel" },
                                    x: "{p.text_at.x}", y: "{p.text_at.y}",
                                    text_anchor: "middle",
                                    dominant_baseline: "central",
                                    "{l.text}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn edge_class(x: &UiCanvasEdge) -> String {
    let mut c = String::from(match x.emphasis {
        Emphasis::Lit => "edge-selected",
        Emphasis::Quiet | Emphasis::Dimmed => "edge-quiet",
        Emphasis::Full => "",
    });
    if x.hovered {
        c.push_str(" edge-hover");
    }
    c
}

/// One edge's arrow on screen: it leaves toward its target and arrives from
/// the left, with a head on the target. Its words: the channel, always,
/// near the end; what it carries, when core gives it a label.
fn arrow(
    x: &UiCanvasEdge,
    ed: &Editor,
    rects: &BTreeMap<NodeId, Rect>,
    cam: &Camera,
) -> Option<Arrow> {
    let p = &ed.project;
    let target = *rects.get(&x.to)?;
    let source = *rects.get(&x.from)?;
    let rows: Vec<Rect> = match p.output(&x.to) {
        Some(o) => x
            .lands_on
            .iter()
            .filter_map(|key| o.ports.iter().position(|q| &q.key == key))
            .map(|i| {
                let r = card_row(i);
                Rect::new(r.min + target.min, r.max + target.min)
            })
            .collect(),
        None => Vec::new(),
    };
    let to_at = match Rect::around(rows.iter().flat_map(|r| [r.min, r.max])) {
        Some(span) => Vec2::new(target.min.x, (span.min.y + span.max.y) / 2.0),
        None if p.output(&x.to).is_some() => {
            Vec2::new(target.min.x, target.min.y + CARD_HEADER + CARD_ROW / 2.0)
        }
        None => Vec2::new(
            target.min.x,
            target.min.y + (target.height() / 2.0).min(30.0),
        ),
    };
    let from_at = if p.node(&x.from).is_some_and(|n| n.is_card()) {
        Vec2::new(source.max.x, source.min.y + CARD_HEADER + CARD_ROW / 2.0)
    } else {
        Vec2::new(source.max.x, to_at.y.clamp(source.min.y, source.max.y))
    };
    let (a, b) = (cam.to_screen(from_at), cam.to_screen(to_at));
    let mut arrow = Arrow::new(av(a), av(b))
        .arrive(EndDirection::Side(Side::Left))
        .end_head(Some(Head::triangle(ARROW_LEN_PX, ARROW_WIDTH_PX)));
    if let Some(label) = &x.label {
        arrow = arrow.label(Label::primary(label.clone()));
    }
    if !x.channel.is_empty() {
        arrow = arrow.label(Label::quiet(x.channel.clone()).near_end());
    }
    Some(arrow)
}

fn av(p: Vec2) -> lab_arrows::Vec2 {
    lab_arrows::Vec2::new(p.x, p.y)
}
