//! Edges: the patch, drawn. An edge from a fixture to an output IS the
//! patch (D42) — there is no separate output panel. Derived from the
//! effective patch, never stored.
//!
//! How much an edge says depends on where you are working:
//!
//! - **a closed fixture** (you are not inside it) shows one bundled edge
//!   per output it feeds: `50 lamps`;
//! - **an open fixture** (the selection or the level you're in is inside
//!   it) shows one edge per span, from the object to the port:
//!   `lamps 0–24 → GPIO 10 @0`.
//!
//! Every edge carries `active`. All of today's are: the field is there for
//! routes, later, which may switch a patch on and off.
//!
//! Data flows from the fixture (or object) to the port, so an edge is drawn
//! as a curve with an arrowhead where it lands. The curve is defined here,
//! once ([`Edge::curve`]): the page draws it and hovering hit-tests it, so
//! what you see is what the pointer finds. Hovering an edge says what it
//! carries in the cursor note, and lights the edge; it never selects.

use crate::camera::Camera;
use crate::editor::Editor;
use crate::geom::{Rect, Vec2, distance_to_segment};
use crate::node::NodeId;
use crate::patch::Span;
use crate::project::Project;
use crate::target::Target;

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// A fixture (bundled) or an object's target (per span).
    pub from: Target,
    /// An output (bundled) or a port (per span).
    pub to: Target,
    pub label: String,
    pub active: bool,
    /// One edge standing for a whole fixture's lamps on one output.
    pub bundled: bool,
    /// It touches the selection: drawn at full strength.
    pub selected: bool,
    /// It is attached to a selected thing itself (not just near one): only
    /// these edges show their label, so the canvas stays quiet.
    pub labelled: bool,
    /// Where it leaves and arrives, in world units: the right side of what
    /// it comes from, the left edge of the port row it lands on.
    pub from_at: Vec2,
    pub to_at: Vec2,
    /// What it carries, in words, for the hover note:
    /// `tail → GPIO 10 · lamps 0–49 at pixel 0`.
    pub carries: String,
    /// The pointer is on it.
    pub hovered: bool,
}

/// How near, in screen pixels, the pointer must be to an edge to hover it.
pub const EDGE_HOVER_PX: f64 = 6.0;
/// The arrowhead where an edge lands: its length and half its width, in
/// screen pixels.
pub const ARROW_LEN_PX: f64 = 7.0;
pub const ARROW_HALF_PX: f64 = 3.5;
/// Samples along the curve when hit-testing it.
const CURVE_SAMPLES: usize = 32;

impl Edge {
    /// The curve on screen: a cubic Bézier `[start, c1, c2, end]` that leaves
    /// and lands horizontally, so the arrowhead at its end points right,
    /// into the port.
    pub fn curve(&self, cam: &Camera) -> [Vec2; 4] {
        let a = cam.to_screen(self.from_at);
        let b = cam.to_screen(self.to_at);
        let dx = ((b.x - a.x).abs() * 0.5).max(30.0);
        [a, a + Vec2::new(dx, 0.0), b - Vec2::new(dx, 0.0), b]
    }

    /// The point `t` (0–1) along the curve, on screen.
    pub fn point_at(&self, cam: &Camera, t: f64) -> Vec2 {
        let [a, c1, c2, b] = self.curve(cam);
        let u = 1.0 - t;
        a * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + b * (t * t * t)
    }

    /// How far `p` (screen) is from the curve.
    pub fn distance(&self, cam: &Camera, p: Vec2) -> f64 {
        let points: Vec<Vec2> = (0..=CURVE_SAMPLES)
            .map(|i| self.point_at(cam, i as f64 / CURVE_SAMPLES as f64))
            .collect();
        points
            .windows(2)
            .map(|w| distance_to_segment(p, w[0], w[1]))
            .fold(f64::INFINITY, f64::min)
    }

    /// The curve as an SVG path.
    pub fn path(&self, cam: &Camera) -> String {
        let [a, c1, c2, b] = self.curve(cam);
        format!(
            "M{},{} C{},{} {},{} {},{}",
            a.x, a.y, c1.x, c1.y, c2.x, c2.y, b.x, b.y
        )
    }

    /// The arrowhead where it lands, as an SVG path: a small triangle whose
    /// tip is the curve's end.
    pub fn arrow_path(&self, cam: &Camera) -> String {
        let b = cam.to_screen(self.to_at);
        let back = b.x - ARROW_LEN_PX;
        format!(
            "M{},{} L{},{} L{},{} Z",
            b.x,
            b.y,
            back,
            b.y - ARROW_HALF_PX,
            back,
            b.y + ARROW_HALF_PX
        )
    }
}

impl Editor {
    /// The patch, drawn: every edge, with the hovered one marked.
    pub fn edges(&self) -> Vec<Edge> {
        let mut edges = self.patch_edges();
        if let Some(i) = self.hovered_edge_of(&edges) {
            edges[i].hovered = true;
        }
        edges
    }

    /// The edge under the pointer, if hovering means anything right now and
    /// no thing is under it (things win: an edge's ends touch them).
    pub fn hovered_edge(&self) -> Option<Edge> {
        self.edges().into_iter().find(|e| e.hovered)
    }

    fn hovered_edge_of(&self, edges: &[Edge]) -> Option<usize> {
        let p = self.hover_point()?;
        if self.hover_target().is_some() {
            return None;
        }
        edges
            .iter()
            .enumerate()
            .map(|(i, e)| (i, e.distance(&self.camera, p)))
            .filter(|(_, d)| *d <= EDGE_HOVER_PX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn patch_edges(&self) -> Vec<Edge> {
        let p = &self.project;
        let patch = p.effective_patch();
        let open = self.open_fixtures();
        let mut out: Vec<Edge> = Vec::new();
        // Bundles, in the order their first span comes.
        let mut bundles: Vec<(NodeId, NodeId, Vec<&Span>)> = Vec::new();
        for s in &patch.spans {
            if open.contains(&s.fixture) {
                let from = p.object_target(&s.object);
                let to = Target::Port {
                    output: s.output.clone(),
                    key: s.port.clone(),
                };
                out.push(Edge {
                    label: format!(
                        "lamps {}–{} → {} @{}",
                        s.lamps.start,
                        s.lamps.end.saturating_sub(1),
                        p.label(&to),
                        s.start_pixel
                    ),
                    carries: format!(
                        "{} → {} · {} at pixel {}",
                        p.label(&from),
                        p.label(&to),
                        lamp_range(s.lamps.start, s.lamps.end),
                        s.start_pixel
                    ),
                    hovered: false,
                    from_at: right_middle(p.lamps_rect(&from)),
                    to_at: p.port_left(&s.output, &s.port),
                    selected: self.touches_selection(&from, &to),
                    labelled: self.selection.contains(&from) || self.selection.contains(&to),
                    from,
                    to,
                    active: true,
                    bundled: false,
                });
                continue;
            }
            match bundles
                .iter_mut()
                .find(|(f, o, _)| *f == s.fixture && *o == s.output)
            {
                Some((_, _, spans)) => spans.push(s),
                None => bundles.push((s.fixture.clone(), s.output.clone(), vec![s])),
            }
        }
        for (fixture, output, spans) in bundles {
            let lamps: u32 = spans.iter().map(|s| s.lamps.len() as u32).sum();
            let from = Target::Node(fixture.clone());
            let to = Target::Node(output.clone());
            let carries = bundle_carries(p, &fixture, &spans);
            out.push(Edge {
                label: format!("{lamps} lamp{}", if lamps == 1 { "" } else { "s" }),
                carries,
                hovered: false,
                from_at: right_middle(p.node_rect(&fixture)),
                to_at: p.port_left(&output, &spans[0].port),
                // Lit when the selection touches one of its spans — a port
                // it doesn't land on leaves it quiet.
                selected: spans.iter().any(|s| {
                    let port = Target::Port {
                        output: s.output.clone(),
                        key: s.port.clone(),
                    };
                    self.touches_selection(&p.object_target(&s.object), &port)
                }),
                labelled: self.selection.contains(&from)
                    || self.selection.contains(&to)
                    || spans.iter().any(|s| {
                        self.selection.contains(&Target::Port {
                            output: s.output.clone(),
                            key: s.port.clone(),
                        })
                    }),
                from,
                to,
                active: true,
                bundled: true,
            });
        }
        out
    }

    /// Fixtures you are working inside: the selection, or the level it is
    /// in, lies inside them.
    pub fn open_fixtures(&self) -> Vec<NodeId> {
        let p = &self.project;
        let mut out: Vec<NodeId> = Vec::new();
        for t in self.selection.iter().chain(self.context().iter()) {
            if let Some(f) = p.fixture_of_target(t)
                && Target::Node(f.clone()) != *t
                && !out.contains(&f)
            {
                out.push(f);
            }
        }
        out
    }

    fn touches_selection(&self, from: &Target, to: &Target) -> bool {
        let p = &self.project;
        self.selection.iter().any(|s| {
            [from, to]
                .iter()
                .any(|e| p.is_ancestor_or_self(s, e) || p.is_ancestor_or_self(e, s))
        })
    }
}

impl Project {
    /// The box around a target's lamps, in world units.
    pub fn lamps_rect(&self, t: &Target) -> Option<Rect> {
        Rect::around(self.lamps_under(t))
    }

    /// The left middle of a port's row: where an edge arrives.
    fn port_left(&self, output: &NodeId, key: &str) -> Vec2 {
        self.port_rect(output, key)
            .map(|r| Vec2::new(r.min.x, (r.min.y + r.max.y) / 2.0))
            .unwrap_or_default()
    }
}

fn right_middle(r: Option<Rect>) -> Vec2 {
    r.map(|r| Vec2::new(r.max.x, (r.min.y + r.max.y) / 2.0))
        .unwrap_or_default()
}

/// What a bundled edge carries: the fixture's lamps, numbered across the
/// whole fixture, per port they land on, runs that continue on the next
/// pixel merged: `tail → GPIO 10 · lamps 0–49 at pixel 0`.
fn bundle_carries(p: &Project, fixture: &NodeId, spans: &[&Span]) -> String {
    // (port label, first lamp, end lamp, first pixel, next pixel)
    let mut runs: Vec<(String, u32, u32, u32, u32)> = Vec::new();
    let mut lamp = 0;
    for s in spans {
        let n = s.lamps.len() as u32;
        let port = p.label(&Target::Port {
            output: s.output.clone(),
            key: s.port.clone(),
        });
        match runs.last_mut() {
            Some(r) if r.0 == port && r.2 == lamp && r.4 == s.start_pixel => {
                r.2 += n;
                r.4 += n;
            }
            _ => runs.push((port, lamp, lamp + n, s.start_pixel, s.start_pixel + n)),
        }
        lamp += n;
    }
    let runs: Vec<String> = runs
        .into_iter()
        .map(|(port, a, b, px, _)| format!("{port} · {} at pixel {px}", lamp_range(a, b)))
        .collect();
    format!("{} → {}", p.node_name(fixture), runs.join("; "))
}

/// Lamps `start..end` in words: `lamps 0–24`, or `lamp 3` for one.
fn lamp_range(start: u32, end: u32) -> String {
    if end <= start + 1 {
        format!("lamp {start}")
    } else {
        format!("lamps {start}–{}", end - 1)
    }
}
