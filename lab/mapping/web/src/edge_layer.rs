//! Edges, routed by the page and drawn in the overlay, in screen pixels, so
//! heads and labels stay one size (Q3). Core says which edges show and what
//! each says (`UiCanvasEdge`); only the view knows where the cards landed,
//! so it routes each one with `lab-arrows`, lays their labels out together,
//! and hit-tests them, reporting the one under the pointer back to core as
//! a fact (`Editor::set_hovered_edge`).
//!
//! On the canvas an edge leaves the right side of its source (a card at its
//! first row; a frame level with where it's going, clamped to the frame)
//! and arrives at the left side of its target, from the left: on an
//! output, level with the middle of the port rows it lands on, so two
//! fixtures on one output arrive apart; on anything else, near its top.
//!
//! In the stack (architecture §6.2) every edge runs in the gutter at the
//! column's left: out of its source's left side, along a lane, into its
//! target's left side. Core shows only the selection's edges there; the
//! shorter an edge's span, the nearer the column its lane, so they don't
//! cross one another.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use lab_arrows::{Arrow, ArrowGeometry, EndDirection, Gutter, Head, Label, LabelRole, Route, Side};
use lab_mapping_model::card_geometry::{CARD_HEADER, CARD_ROW, card_row};
use lab_mapping_model::{Camera, Editor, Emphasis, LayoutKind, NodeId, Rect, UiCanvasEdge, Vec2};

/// How near, in screen pixels, the pointer must be to an edge to hover it.
pub const EDGE_HOVER_PX: f64 = 6.0;
/// The arrowhead where an edge lands: length and width, in screen pixels.
pub const ARROW_LEN_PX: f64 = 7.0;
pub const ARROW_WIDTH_PX: f64 = 7.0;
/// The stack's gutter: its innermost lane this far left of the column, and
/// each next lane this much further out, in screen pixels.
const FIRST_LANE_PX: f64 = 8.0;
const LANE_STEP_PX: f64 = 6.0;
/// A gutter edge's corners, in screen pixels.
const LANE_CORNER_PX: f64 = 5.0;

/// Each shown edge with its arrow on screen. An edge whose ends have no
/// place yet is left out.
pub fn route(
    edges: &[UiCanvasEdge],
    ed: &Editor,
    rects: &BTreeMap<NodeId, Rect>,
    cam: &Camera,
) -> Vec<(UiCanvasEdge, Arrow)> {
    let shown = edges
        .iter()
        .filter(|x| x.shown)
        .filter_map(|x| Some((x.clone(), ends(x, ed, rects, cam)?)));
    match ed.layout() {
        LayoutKind::Canvas => shown
            .map(|(x, (a, b))| {
                let arrow = Arrow::new(av(a), av(b)).arrive(EndDirection::Side(Side::Left));
                let arrow = words(arrow, &x);
                (x, arrow)
            })
            .collect(),
        LayoutKind::Stack => {
            // The column's left edge on screen; lanes to its left, the
            // shortest spans nearest.
            let column = cam.to_screen(Vec2::new(0.0, 0.0)).x;
            let mut shown: Vec<(UiCanvasEdge, (Vec2, Vec2))> = shown.collect();
            shown.sort_by(|p, q| {
                let span = |e: &(Vec2, Vec2)| (e.1.y - e.0.y).abs();
                span(&p.1).total_cmp(&span(&q.1))
            });
            shown
                .into_iter()
                .enumerate()
                .map(|(i, (x, (a, b)))| {
                    let lane = (column - FIRST_LANE_PX - i as f64 * LANE_STEP_PX).max(2.0);
                    let arrow = Arrow::new(av(a), av(b)).route(Route::Gutter(Gutter {
                        lane_x: lane,
                        radius: LANE_CORNER_PX,
                    }));
                    (x.clone(), words(arrow, &x))
                })
                .collect()
        }
    }
}

/// The edge under `at` (screen), if one is within [`EDGE_HOVER_PX`]: the
/// nearest. Routed from where the editor was told the nodes are.
pub fn hit(ed: &Editor, at: Vec2) -> Option<(NodeId, NodeId)> {
    let p = lab_arrows::Vec2::new(at.x, at.y);
    let rects = &ed.project.placement().rects;
    route(&ed.view_edges(), ed, rects, &ed.camera)
        .into_iter()
        .map(|(x, a)| (x, a.line().distance_to(p)))
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

/// Where an edge leaves and arrives, on screen. On the canvas it leaves a
/// source's right side; in the stack its left side (the gutter's).
fn ends(
    x: &UiCanvasEdge,
    ed: &Editor,
    rects: &BTreeMap<NodeId, Rect>,
    cam: &Camera,
) -> Option<(Vec2, Vec2)> {
    let p = &ed.project;
    let target = *rects.get(&x.to)?;
    let source = *rects.get(&x.from)?;
    let rows: Vec<Rect> = match p.output(&x.to) {
        Some(o) => x
            .lands_on
            .iter()
            .filter_map(|key| o.ports.iter().position(|q| &q.key == key))
            .map(|i| {
                let r = card_row(i, target.width());
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
    let side = match ed.layout() {
        LayoutKind::Canvas => source.max.x,
        LayoutKind::Stack => source.min.x,
    };
    let from_at = if p.node(&x.from).is_some_and(|n| n.is_card()) {
        Vec2::new(side, source.min.y + CARD_HEADER + CARD_ROW / 2.0)
    } else {
        let y = match ed.layout() {
            LayoutKind::Canvas => to_at.y.clamp(source.min.y, source.max.y),
            LayoutKind::Stack => source.min.y + (source.height() / 2.0).min(30.0),
        };
        Vec2::new(side, y)
    };
    Some((cam.to_screen(from_at), cam.to_screen(to_at)))
}

/// An edge's head and words: the channel, always, near the end; what it
/// carries, when core gives it a label.
fn words(arrow: Arrow, x: &UiCanvasEdge) -> Arrow {
    let mut arrow = arrow.end_head(Some(Head::triangle(ARROW_LEN_PX, ARROW_WIDTH_PX)));
    if let Some(label) = &x.label {
        arrow = arrow.label(Label::primary(label.clone()));
    }
    if !x.channel.is_empty() {
        arrow = arrow.label(Label::quiet(x.channel.clone()).near_end());
    }
    arrow
}

fn av(p: Vec2) -> lab_arrows::Vec2 {
    lab_arrows::Vec2::new(p.x, p.y)
}
