//! Edges, routed by the page. Core says which edges there are and what
//! each says (`Editor::edges`); only the view knows where the cards landed,
//! so it routes each one with `lab-arrows`, in screen pixels, and hit-tests
//! them, reporting the one under the pointer back to core as a fact
//! (`Editor::set_hovered_edge`).
//!
//! An edge leaves the right side of its source (a card at its first row; a
//! frame level with where it's going, clamped to the frame) and arrives at
//! the left side of its target, from the left: on an output, level with the
//! middle of the port rows it lands on, so two fixtures on one output
//! arrive apart; on anything else, near its top.

use lab_arrows::{Arrow, EndDirection, Head, Label, Side};
use lab_mapping_model::card_geometry::{CARD_HEADER, CARD_ROW};
use lab_mapping_model::{Edge, Editor, NodeId, Rect, Target, Vec2};

/// How near, in screen pixels, the pointer must be to an edge to hover it.
pub const EDGE_HOVER_PX: f64 = 6.0;
/// The arrowhead where an edge lands: length and width, in screen pixels.
pub const ARROW_LEN_PX: f64 = 7.0;
pub const ARROW_WIDTH_PX: f64 = 7.0;

/// Every edge with its arrow on screen. An edge whose ends have no place
/// yet is left out.
pub fn route(e: &Editor) -> Vec<(Edge, Arrow)> {
    e.edges()
        .into_iter()
        .filter_map(|x| {
            let arrow = arrow(e, &x)?;
            Some((x, arrow))
        })
        .collect()
}

/// The edge under `at` (screen), if any is within [`EDGE_HOVER_PX`]: the
/// nearest.
pub fn hit(e: &Editor, at: Vec2) -> Option<(NodeId, NodeId)> {
    let p = lab_arrows::Vec2::new(at.x, at.y);
    route(e)
        .into_iter()
        .map(|(x, a)| {
            let d = lab_arrows::Path::of_cubic(&a.centreline(), false).distance_to(p);
            (x, d)
        })
        .filter(|(_, d)| *d <= EDGE_HOVER_PX)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .and_then(|(x, _)| Some((node(&x.from)?, node(&x.to)?)))
}

/// One edge's arrow on screen: it leaves toward its target and arrives from
/// the left, with a head on the target. Its words: the channel, always,
/// near the end; what it carries, when it belongs to the selection.
fn arrow(e: &Editor, x: &Edge) -> Option<Arrow> {
    let p = &e.project;
    let (from, to) = (node(&x.from)?, node(&x.to)?);
    let target = p.node_rect(&to)?;
    let source = p.node_rect(&from)?;
    let rows: Vec<Rect> = x
        .lands_on
        .iter()
        .filter_map(|key| p.port_rect(&to, key))
        .collect();
    let to_at = match Rect::around(rows.iter().flat_map(|r| [r.min, r.max])) {
        Some(span) => Vec2::new(target.min.x, (span.min.y + span.max.y) / 2.0),
        None if p.output(&to).is_some() => {
            Vec2::new(target.min.x, target.min.y + CARD_HEADER + CARD_ROW / 2.0)
        }
        None => Vec2::new(
            target.min.x,
            target.min.y + (target.height() / 2.0).min(30.0),
        ),
    };
    let from_at = if p.node(&from).is_some_and(|n| n.is_card()) {
        Vec2::new(source.max.x, source.min.y + CARD_HEADER + CARD_ROW / 2.0)
    } else {
        Vec2::new(source.max.x, to_at.y.clamp(source.min.y, source.max.y))
    };
    let (a, b) = (e.camera.to_screen(from_at), e.camera.to_screen(to_at));
    let mut arrow = Arrow::new(av(a), av(b))
        .arrive(EndDirection::Side(Side::Left))
        .end_head(Some(Head::triangle(ARROW_LEN_PX, ARROW_WIDTH_PX)));
    if x.labelled && !x.label.is_empty() {
        arrow = arrow.label(Label::primary(x.label.clone()));
    }
    if !x.channel.is_empty() {
        arrow = arrow.label(Label::quiet(x.channel.clone()).near_end());
    }
    Some(arrow)
}

fn node(t: &Target) -> Option<NodeId> {
    match t {
        Target::Node(id) => Some(id.clone()),
        _ => None,
    }
}

fn av(p: Vec2) -> lab_arrows::Vec2 {
    lab_arrows::Vec2::new(p.x, p.y)
}
