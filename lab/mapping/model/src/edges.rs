//! Edges: the patch, drawn. An edge from a fixture to an output IS the
//! patch (D42) — there is no separate output panel. Derived from the
//! effective patch, never stored.
//!
//! **One edge per object** (per span, when an object runs on into the next
//! port), always (D46). It leaves from the object's FIRST lamp — where its
//! data enters — and lands on the PIXEL where its span starts, inside the
//! output's card (D45): the same thing the fixture side shows, read the
//! other way. The routing is really node to node; drawing it lamp to pixel
//! is a deliberate teaching choice.
//!
//! **Bundling is by density, never by selection.** A (fixture, output)
//! pair draws as one bundled edge when it carries more than
//! [`BUNDLE_OBJECTS`] objects (the dome), or when the fixture is drawn so
//! small that its objects' edges would run on top of each other: less than
//! [`EDGE_ROOM_PX`] screen pixels of fixture per object. A bundle leaves
//! from the point of the fixture's frame nearest where it lands, and lands
//! on the first pixel of its first span. Selecting something changes how
//! an edge is drawn (lit, labelled), never which edges there are.
//!
//! Every edge carries `active`. All of today's are: the field is there for
//! routes, later, which may switch a patch on and off.
//!
//! Data flows from the lamp to the pixel, so an edge is drawn as a curve
//! that leaves heading toward the output and arrives from the left, with an
//! arrowhead on the pixel. The curve is defined here, once
//! ([`Edge::curve`]): the page draws it and hovering hit-tests it, so what
//! you see is what the pointer finds. Hovering an edge says what it carries
//! in the cursor note, and lights the edge; it never selects.

use std::ops::Range;

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
    /// The pixels it lands on, on its first port.
    pub pixels: Range<u32>,
    pub label: String,
    pub active: bool,
    /// One edge standing for a whole fixture's lamps on one output.
    pub bundled: bool,
    /// It touches the selection: drawn at full strength.
    pub selected: bool,
    /// It is attached to a selected thing itself (not just near one): only
    /// these edges show their label, so the canvas stays quiet.
    pub labelled: bool,
    /// Where it leaves and arrives, in world units: the object's first
    /// lamp (a bundle: the fixture's frame), and the left side of the pixel
    /// its span starts on.
    pub from_at: Vec2,
    pub to_at: Vec2,
    /// Which way it leaves: a unit vector toward where it lands.
    pub leave: Vec2,
    /// What it carries, in words, for the hover note:
    /// `tail → GPIO 10 · lamps 0–49 at pixel 0`.
    pub carries: String,
    /// The pointer is on it.
    pub hovered: bool,
}

/// A (fixture, output) pair carrying more objects than this draws as one
/// bundled edge: the dome's panels, not one curve each.
pub const BUNDLE_OBJECTS: usize = 24;
/// Each object's edge wants this many screen pixels of its fixture; drawn
/// smaller (zoomed far out), the pair bundles, since its edges would run on
/// top of each other.
pub const EDGE_ROOM_PX: f64 = 8.0;
/// How near, in screen pixels, the pointer must be to an edge to hover it.
pub const EDGE_HOVER_PX: f64 = 6.0;
/// The arrowhead where an edge lands: its length and half its width, in
/// screen pixels.
pub const ARROW_LEN_PX: f64 = 7.0;
pub const ARROW_HALF_PX: f64 = 3.5;
/// Samples along the curve when hit-testing it.
const CURVE_SAMPLES: usize = 32;

impl Edge {
    /// The curve on screen: a cubic Bézier `[start, c1, c2, end]` that
    /// leaves heading toward where it lands and arrives horizontally, from
    /// the left, so the arrowhead at its end points right, onto the pixel.
    pub fn curve(&self, cam: &Camera) -> [Vec2; 4] {
        let a = cam.to_screen(self.from_at);
        let b = cam.to_screen(self.to_at);
        let reach = (a.distance(b) * 0.4).max(20.0);
        [a, a + self.leave * reach, b - Vec2::new(reach, 0.0), b]
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
        // Cheap questions first: no edge is hovered while a thing is.
        if self.hover_point().is_none() || self.hover_target().is_some() {
            return None;
        }
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
        // The spans of each (fixture, output) pair, in the order the pair's
        // first span comes.
        let mut pairs: Vec<(NodeId, NodeId, Vec<&Span>)> = Vec::new();
        for s in &patch.spans {
            match pairs
                .iter_mut()
                .find(|(f, o, _)| *f == s.fixture && *o == s.output)
            {
                Some((_, _, spans)) => spans.push(s),
                None => pairs.push((s.fixture.clone(), s.output.clone(), vec![s])),
            }
        }
        let mut out: Vec<Edge> = Vec::new();
        for (fixture, output, spans) in pairs {
            if self.bundles(&fixture, &spans) {
                out.push(self.bundled_edge(&fixture, &output, &spans));
            } else {
                out.extend(spans.iter().map(|s| self.span_edge(s)));
            }
        }
        out
    }

    /// Whether a (fixture, output) pair draws as one edge: too many objects,
    /// or too little room on screen for each (D46).
    pub fn bundles(&self, fixture: &NodeId, spans: &[&Span]) -> bool {
        let mut objects: Vec<&crate::ObjectId> = Vec::new();
        for s in spans {
            if !objects.contains(&&s.object) {
                objects.push(&s.object);
            }
        }
        let n = objects.len().max(1);
        let drawn = self
            .project
            .node_rect(fixture)
            .map_or(0.0, |r| r.width().max(r.height()) * self.camera.scale);
        n > BUNDLE_OBJECTS || drawn / (n as f64) < EDGE_ROOM_PX
    }

    /// One span's edge: from its first lamp to the pixel it starts on.
    fn span_edge(&self, s: &Span) -> Edge {
        let p = &self.project;
        let from = p.object_target(&s.object);
        let to = Target::Port {
            output: s.output.clone(),
            key: s.port.clone(),
        };
        let pixels = s.pixels();
        let from_at = p
            .object(&s.object)
            .and_then(|o| o.lamps.get(s.lamps.start as usize).map(|l| l.pos))
            .unwrap_or_default();
        let to_at = p.pixel_left(&s.output, &s.port, s.start_pixel);
        Edge {
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
            from_at,
            to_at,
            leave: toward(from_at, to_at),
            selected: self.touches_selection(&from, &to, &pixels),
            labelled: self.selection.contains(&from)
                || self.selection.contains(&to)
                || self.selection.contains(&Target::Node(s.output.clone()))
                || self.selects_pixel_in(&to, &pixels),
            from,
            to,
            pixels,
            active: true,
            bundled: false,
        }
    }

    /// A pair's one edge: from the fixture's frame to its first pixel.
    fn bundled_edge(&self, fixture: &NodeId, output: &NodeId, spans: &[&Span]) -> Edge {
        let p = &self.project;
        let lamps: u32 = spans.iter().map(|s| s.lamps.len() as u32).sum();
        let from = Target::Node(fixture.clone());
        let to = Target::Node(output.clone());
        let first = spans[0];
        let to_at = p.pixel_left(output, &first.port, first.start_pixel);
        let from_at = p
            .node_rect(fixture)
            .map_or(Vec2::default(), |r| nearest_on_border(r, to_at));
        let port_of = |s: &Span| Target::Port {
            output: s.output.clone(),
            key: s.port.clone(),
        };
        let span_pixels = |s: &Span| s.pixels();
        Edge {
            label: format!("{lamps} lamp{}", if lamps == 1 { "" } else { "s" }),
            carries: bundle_carries(p, fixture, spans),
            hovered: false,
            from_at,
            to_at,
            leave: toward(from_at, to_at),
            // Lit when the selection touches one of its spans — a port it
            // doesn't land on leaves it quiet.
            selected: spans.iter().any(|s| {
                self.touches_selection(&p.object_target(&s.object), &port_of(s), &span_pixels(s))
            }),
            labelled: self.selection.contains(&from)
                || self.selection.contains(&to)
                || spans.iter().any(|s| {
                    self.selection.contains(&port_of(s))
                        || self.selects_pixel_in(&port_of(s), &span_pixels(s))
                }),
            pixels: span_pixels(first),
            from,
            to,
            active: true,
            bundled: true,
        }
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

    /// Whether the selection touches an edge from `from` to the port `to`,
    /// landing on `pixels`: something at either end, above it or below it —
    /// except that a selected pixel touches only the edge landing on it.
    fn touches_selection(&self, from: &Target, to: &Target, pixels: &Range<u32>) -> bool {
        let p = &self.project;
        self.selection.iter().any(|s| {
            if let Target::Pixel { index, .. } = s {
                return s.port_of_pixel().as_ref() == Some(to) && pixels.contains(index);
            }
            [from, to]
                .iter()
                .any(|e| p.is_ancestor_or_self(s, e) || p.is_ancestor_or_self(e, s))
        })
    }

    /// Whether a selected pixel of `port` lies in `pixels`.
    fn selects_pixel_in(&self, port: &Target, pixels: &Range<u32>) -> bool {
        self.selection.iter().any(|s| match s {
            Target::Pixel { index, .. } => {
                s.port_of_pixel().as_ref() == Some(port) && pixels.contains(index)
            }
            _ => false,
        })
    }
}

impl Project {
    /// The box around a target's lamps, in world units.
    pub fn lamps_rect(&self, t: &Target) -> Option<Rect> {
        Rect::around(self.lamps_under(t))
    }

    /// The left middle of a pixel's square: where an edge arrives.
    fn pixel_left(&self, output: &NodeId, key: &str, index: u32) -> Vec2 {
        self.pixel_rect(output, key, index)
            .or_else(|| self.port_rect(output, key))
            .map(|r| Vec2::new(r.min.x, (r.min.y + r.max.y) / 2.0))
            .unwrap_or_default()
    }
}

/// The point on a rectangle's border nearest `p` (outside it): where a
/// bundle leaves its fixture's frame.
fn nearest_on_border(r: Rect, p: Vec2) -> Vec2 {
    let c = Vec2::new(p.x.clamp(r.min.x, r.max.x), p.y.clamp(r.min.y, r.max.y));
    if c != p {
        return c;
    }
    // Inside it: straight out of the nearest side.
    let sides = [
        (p.x - r.min.x, Vec2::new(r.min.x, p.y)),
        (r.max.x - p.x, Vec2::new(r.max.x, p.y)),
        (p.y - r.min.y, Vec2::new(p.x, r.min.y)),
        (r.max.y - p.y, Vec2::new(p.x, r.max.y)),
    ];
    sides
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(p, |s| s.1)
}

/// The unit vector from `a` toward `b` (rightward if they coincide).
fn toward(a: Vec2, b: Vec2) -> Vec2 {
    let d = b - a;
    let len = d.length();
    if len < 1e-9 {
        Vec2::new(1.0, 0.0)
    } else {
        d * (1.0 / len)
    }
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
            Some(r) if r.0 == port && r.2 == lamp && r.4 == s.start_pixel && !s.reversed => {
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
