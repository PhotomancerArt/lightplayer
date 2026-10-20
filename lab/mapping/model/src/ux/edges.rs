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
//! Core says which edges there are and what each says; it never says where
//! they run. The view routes them from where the cards landed (a curve on
//! the canvas, D57), and since only it knows where its lines are, it
//! hit-tests them and reports the one under the pointer as a fact
//! ([`Editor::set_hovered_edge`]).

use crate::domain::geom::Rect;
use crate::domain::node::NodeId;
use crate::domain::project::Project;
use crate::domain::target::Target;
use crate::ux::editor::Editor;

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
    /// The port rows of the target it lands beside (an output's ports this
    /// fixture feeds), by key: the view brings the edge in level with them,
    /// so two fixtures on one output arrive apart. Empty for any other
    /// target.
    pub lands_on: Vec<String>,
    /// It touches the selection (something at either end, or inside
    /// either): drawn at full strength.
    pub selected: bool,
    /// Every edge today is live; the field is there for routes, later.
    pub active: bool,
    /// The pointer is on it.
    pub hovered: bool,
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
        let hovered = self.edge_hover_now();
        pairs
            .iter()
            .map(|(from, to, channel)| {
                let mut e = self.edge(from, to, channel);
                e.hovered = hovered.as_ref() == Some(&(from.clone(), to.clone()));
                e
            })
            .collect()
    }

    /// The edge under the pointer, as the view reported it, while hovering
    /// means anything and no thing is under the pointer (things win: an
    /// edge's ends touch them).
    pub fn hovered_edge(&self) -> Option<Edge> {
        self.edge_hover_now()?;
        self.edges().into_iter().find(|e| e.hovered)
    }

    /// The view's report of the edge under the pointer, `(from, to)`, or
    /// `None`. A fact, like the pointer itself: core can't hit-test lines it
    /// doesn't route.
    pub fn set_hovered_edge(&mut self, edge: Option<(NodeId, NodeId)>) {
        self.edge_hover = edge;
    }

    /// The reported edge, if it counts right now.
    pub(crate) fn edge_hover_now(&self) -> Option<(NodeId, NodeId)> {
        if self.hover_point().is_none() || self.hover_target().is_some() {
            return None;
        }
        self.edge_hover.clone()
    }

    /// One pair's edge.
    fn edge(&self, from: &NodeId, to: &NodeId, channel: &str) -> Edge {
        let p = &self.project;
        let from_t = Target::Node(from.clone());
        let to_t = Target::Node(to.clone());
        let mut lands_on: Vec<String> = Vec::new();
        for s in &p.effective_patch().spans {
            if &s.fixture == from && &s.output == to && !lands_on.contains(&s.port) {
                lands_on.push(s.port.clone());
            }
        }
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
        Edge {
            selected: touches(&from_t) || touches(&to_t),
            from: from_t,
            to: to_t,
            channel: channel.to_string(),
            label,
            carries,
            lands_on,
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

    /// What a fixture → output edge carries, short: `50 lamps on GPIO 10`,
    /// `64 lamps on D0, D1`; empty for an edge with nothing patched (a
    /// render source's, or an empty fixture's).
    pub(crate) fn carried(&self, from: &NodeId, to: &NodeId) -> String {
        let mut lamps = 0u32;
        let mut keys: Vec<&str> = Vec::new();
        for s in &self.effective_patch().spans {
            if &s.fixture == from && &s.output == to {
                lamps += s.len();
                if !keys.contains(&s.port.as_str()) {
                    keys.push(&s.port);
                }
            }
        }
        if lamps == 0 {
            return String::new();
        }
        let ports: Vec<String> = keys
            .into_iter()
            .map(|key| {
                self.label(&Target::Port {
                    output: to.clone(),
                    key: key.to_string(),
                })
            })
            .collect();
        let s = if lamps == 1 { "" } else { "s" };
        format!("{lamps} lamp{s} on {}", ports.join(", "))
    }
}
