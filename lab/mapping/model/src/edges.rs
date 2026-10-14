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

use crate::editor::Editor;
use crate::geom::{Rect, Vec2};
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
    /// Where it leaves and arrives, in world units: the right side of what
    /// it comes from, the left edge of the port row it lands on.
    pub from_at: Vec2,
    pub to_at: Vec2,
}

impl Editor {
    pub fn edges(&self) -> Vec<Edge> {
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
                    from_at: right_middle(p.lamps_rect(&from)),
                    to_at: p.port_left(&s.output, &s.port),
                    selected: self.touches_selection(&from, &to),
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
            out.push(Edge {
                label: format!("{lamps} lamp{}", if lamps == 1 { "" } else { "s" }),
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
