//! Edges: the wiring between nodes, drawn — node to node, one per pair.
//!
//! - **render source → fixture**, over a visual channel (`visual.out`);
//! - **fixture → output**, over a control channel (`control.out`), or by
//!   the fixture's place rules: the patch (D42), drawn as ONE edge per
//!   (fixture, output) pair, whatever it carries.
//!
//! Detail follows selection (progressive disclosure, D56): an edge never
//! fans out per object or per port. Where a fixture's objects land is shown
//! on the output itself — each object's run of pixels, lit while the object
//! is selected (`port_runs.rs`) — not by more lines across the canvas.
//! (Per-object edges, D45/D46, were tried and were too busy on the choker.)
//!
//! Every edge carries its channel's name, always, small (D53). An edge that
//! belongs to the selection also says what it carries ("50 lamps on GPIO
//! 10"). Hovering an edge says it in full in the cursor note and lights it;
//! it never selects.
//!
//! The arrow itself — the curve, the head, where the words sit — is a
//! [`lab_arrows::Arrow`], built here ([`Edge::arrow`]) so the page draws
//! exactly the line the pointer hit-tests.

use lab_arrows::{Arrow, EndDirection, Head, Label, Side};

use crate::ux::camera::Camera;
use crate::ux::editor::Editor;
use crate::domain::geom::{Rect, Vec2};
use crate::layout::{CARD_HEADER, CARD_ROW};
use crate::domain::node::NodeId;
use crate::domain::project::Project;
use crate::domain::target::Target;

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// The node it leaves: a render source or a fixture.
    pub from: Target,
    /// The node it reaches: a fixture or an output.
    pub to: Target,
    /// The channel it carries (`visual.out`, `control.out`), or empty for a
    /// fixture that reaches an output only by place rules.
    pub channel: String,
    /// What it carries, short, for when it belongs to the selection:
    /// `50 lamps on GPIO 10`.
    pub label: String,
    /// What it carries in full, for the hover note:
    /// `tail → tail out · 50 lamps on GPIO 10 · control.out`.
    pub carries: String,
    /// Where it leaves and arrives, in world units: the right side of the
    /// source, the left side of the target.
    pub from_at: Vec2,
    pub to_at: Vec2,
    /// It touches the selection (something at either end, or inside
    /// either): drawn at full strength.
    pub selected: bool,
    /// It belongs to the selection itself (one of its nodes is selected, or
    /// something on the output it feeds): it shows what it carries.
    pub labelled: bool,
    /// Every edge today is live; the field is there for routes, later.
    pub active: bool,
    /// The pointer is on it.
    pub hovered: bool,
}

/// How near, in screen pixels, the pointer must be to an edge to hover it.
pub const EDGE_HOVER_PX: f64 = 6.0;
/// The arrowhead where an edge lands: length and width, in screen pixels.
pub const ARROW_LEN_PX: f64 = 7.0;
pub const ARROW_WIDTH_PX: f64 = 7.0;

impl Edge {
    /// The arrow on screen: it leaves toward its target and arrives from the
    /// left, with a head on the target. Its words: the channel, always,
    /// near the end; what it carries, when it belongs to the selection.
    pub fn arrow(&self, cam: &Camera) -> Arrow {
        let a = cam.to_screen(self.from_at);
        let b = cam.to_screen(self.to_at);
        let mut arrow = Arrow::new(av(a), av(b))
            .arrive(EndDirection::Side(Side::Left))
            .end_head(Some(Head::triangle(ARROW_LEN_PX, ARROW_WIDTH_PX)));
        if self.labelled && !self.label.is_empty() {
            arrow = arrow.label(Label::primary(self.label.clone()));
        }
        if !self.channel.is_empty() {
            arrow = arrow.label(Label::quiet(self.channel.clone()).near_end());
        }
        arrow
    }

    /// How far `p` (screen) is from the arrow's line.
    pub fn distance(&self, cam: &Camera, p: Vec2) -> f64 {
        let line = self.arrow(cam).centreline();
        lab_arrows::Path::of_cubic(&line, false).distance_to(av(p))
    }
}

impl Editor {
    /// Every edge between nodes — render sources into fixtures, then
    /// fixtures into outputs — one per pair, the hovered one marked.
    pub fn edges(&self) -> Vec<Edge> {
        let p = &self.project;
        let mut pairs: Vec<(NodeId, NodeId, String)> = Vec::new();
        let mut add = |from: &NodeId, to: &NodeId, channel: String| {
            if !pairs.iter().any(|(f, t, _)| f == from && t == to) {
                pairs.push((from.clone(), to.clone(), channel));
            }
        };
        let bindings = p.bindings();
        for b in bindings.iter().filter(|b| p.fixture(&b.from).is_none()) {
            add(&b.from, &b.to, b.channel.to_string());
        }
        for b in bindings.iter().filter(|b| p.fixture(&b.from).is_some()) {
            add(&b.from, &b.to, b.channel.to_string());
        }
        // A fixture reaching an output only by place rules still shows.
        for s in &p.effective_patch().spans {
            add(&s.fixture, &s.output, String::new());
        }
        let mut edges: Vec<Edge> = pairs
            .iter()
            .map(|(from, to, channel)| self.edge(from, to, channel))
            .collect();
        if let Some(i) = self.hovered_edge_of(&edges) {
            edges[i].hovered = true;
        }
        edges
    }

    /// The edge under the pointer, if hovering means anything right now and
    /// no thing is under it (things win: an edge's ends touch them).
    pub fn hovered_edge(&self) -> Option<Edge> {
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

    /// One pair's edge.
    fn edge(&self, from: &NodeId, to: &NodeId, channel: &str) -> Edge {
        let p = &self.project;
        let from_t = Target::Node(from.clone());
        let to_t = Target::Node(to.clone());
        let to_at = p.arrival(from, to);
        let from_at = match p.node_rect(from) {
            Some(r) if p.node(from).is_some_and(|n| n.is_card()) => {
                Vec2::new(r.max.x, r.min.y + CARD_HEADER + CARD_ROW / 2.0)
            }
            Some(r) => Vec2::new(r.max.x, to_at.y.clamp(r.min.y, r.max.y)),
            None => Vec2::default(),
        };
        let label = p.carried(from, to);
        let mut carries = format!("{} → {}", p.node_name(from), p.node_name(to));
        if !label.is_empty() {
            carries.push_str(&format!(" · {label}"));
        }
        if !channel.is_empty() {
            carries.push_str(&format!(" · {channel}"));
        }
        let touches = |t: &Target| {
            self.selection
                .iter()
                .any(|s| p.is_ancestor_or_self(s, t) || p.is_ancestor_or_self(t, s))
        };
        // On the output side, only what this fixture feeds belongs to it.
        let on_its_output = self.selection.iter().any(|s| match s {
            Target::Port { output, key } => {
                output == to
                    && p.ports_of(&from_t).contains(&Target::Port {
                        output: output.clone(),
                        key: key.clone(),
                    })
            }
            Target::Pixel { output, .. } => output == to,
            _ => false,
        });
        Edge {
            selected: touches(&from_t) || touches(&to_t),
            labelled: self.selection.contains(&from_t)
                || self.selection.contains(&to_t)
                || on_its_output,
            from: from_t,
            to: to_t,
            channel: channel.to_string(),
            label,
            carries,
            from_at,
            to_at,
            active: true,
            hovered: false,
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
}

impl Project {
    /// The box around a target's lamps, in world units.
    pub fn lamps_rect(&self, t: &Target) -> Option<Rect> {
        Rect::around(self.lamps_under(t))
    }

    /// Where an edge from `from` reaches `to`: the left side of `to`. On an
    /// output card, level with the middle of the port rows `from` lands on,
    /// so two fixtures on one output arrive apart; otherwise near the top.
    pub(crate) fn arrival(&self, from: &NodeId, to: &NodeId) -> Vec2 {
        let Some(r) = self.node_rect(to) else {
            return Vec2::default();
        };
        if self.output(to).is_some() {
            let rows: Vec<Rect> = self
                .ports_of(&Target::Node(from.clone()))
                .iter()
                .filter_map(|t| match t {
                    Target::Port { output, key } if output == to => self.port_rect(output, key),
                    _ => None,
                })
                .collect();
            if let Some(span) = Rect::around(rows.iter().flat_map(|r| [r.min, r.max])) {
                return Vec2::new(r.min.x, (span.min.y + span.max.y) / 2.0);
            }
            return Vec2::new(r.min.x, r.min.y + CARD_HEADER + CARD_ROW / 2.0);
        }
        Vec2::new(r.min.x, r.min.y + (r.height() / 2.0).min(30.0))
    }

    /// What a fixture → output edge carries, short: `50 lamps on GPIO 10`,
    /// `64 lamps on D0, D1`; empty for an edge with nothing patched (a
    /// render source's, or an empty fixture's).
    pub(crate) fn carried(&self, from: &NodeId, to: &NodeId) -> String {
        let mut lamps = 0u32;
        let mut ports: Vec<String> = Vec::new();
        for s in &self.effective_patch().spans {
            if &s.fixture == from && &s.output == to {
                lamps += s.len();
                let port = self.label(&Target::Port {
                    output: s.output.clone(),
                    key: s.port.clone(),
                });
                if !ports.contains(&port) {
                    ports.push(port);
                }
            }
        }
        if lamps == 0 {
            return String::new();
        }
        let s = if lamps == 1 { "" } else { "s" };
        format!("{lamps} lamp{s} on {}", ports.join(", "))
    }
}

fn av(p: Vec2) -> lab_arrows::Vec2 {
    lab_arrows::Vec2::new(p.x, p.y)
}
