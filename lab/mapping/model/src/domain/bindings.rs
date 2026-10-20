//! Bindings: which node's product reaches which node, and over which
//! channel — the wiring the canvas draws as edges between nodes (D53,
//! D55). Derived from the nodes, never stored.
//!
//! - **Render source → fixture**, over a visual channel: a fixture reads
//!   the channel it names (`visual.out`), and the writer it gets is the
//!   nearest one — in its own module first, then outward, a module at a
//!   time (fit-check §6: reads look outward, writes stay local). Among
//!   writers in one module, the first in attach order (by name).
//! - **Fixture → output**, over a control channel: the lab's patch rules
//!   ARE this binding, read the product's way (fit-check §1, §6). A rule
//!   naming an output means the fixture writes the channel that output
//!   reads, so the rule's output decides the channel and the binding's
//!   name is the output's. A rule naming an output that is gone binds to
//!   nothing (and is a patch problem, `patch.rs`), never to another
//!   output.

use crate::domain::channel::Channel;
use crate::domain::node::{Node, NodeId};
use crate::domain::project::Project;

/// One node's product reaching another over a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The writer: a render source or a fixture.
    pub from: NodeId,
    /// The reader: a fixture or an output.
    pub to: NodeId,
    pub channel: Channel,
}

impl Project {
    /// Every binding: render sources to fixtures, then fixtures to
    /// outputs, each fixture in attach order.
    pub fn bindings(&self) -> Vec<Binding> {
        let fixtures: Vec<&Node> = self
            .attach_order()
            .into_iter()
            .filter(|n| n.fixture().is_some())
            .collect();
        let mut out = Vec::new();
        for n in &fixtures {
            if let (Some(source), Some(f)) = (self.visual_source(&n.id), n.fixture()) {
                out.push(Binding {
                    from: source,
                    to: n.id.clone(),
                    channel: f.reads.clone(),
                });
            }
        }
        for n in &fixtures {
            for output in self.control_outputs(&n.id) {
                if let Some(o) = self.output(&output) {
                    out.push(Binding {
                        from: n.id.clone(),
                        to: output.clone(),
                        channel: o.reads.clone(),
                    });
                }
            }
        }
        out
    }

    /// The render source whose picture a fixture samples: the nearest
    /// writer of the channel it reads, its own module first, then outward.
    pub fn visual_source(&self, fixture: &NodeId) -> Option<NodeId> {
        let reads = &self.fixture(fixture)?.reads;
        let mut module = self.parent_module(fixture).cloned();
        loop {
            let level = match &module {
                Some(m) => self.node(m)?.children(),
                None => self.nodes(),
            };
            let mut writers: Vec<&Node> = level
                .iter()
                .filter(|n| n.source().is_some_and(|s| &s.writes == reads))
                .collect();
            writers.sort_by(|a, b| a.name.cmp(&b.name));
            if let Some(w) = writers.first() {
                return Some(w.id.clone());
            }
            module = self.parent_module(module.as_ref()?).cloned();
        }
    }

    /// The outputs a fixture's control product goes to: every output its
    /// rules name that is still there, in rule order.
    pub fn control_outputs(&self, fixture: &NodeId) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = Vec::new();
        for r in self
            .fixture(fixture)
            .map(|f| f.rules.as_slice())
            .unwrap_or_default()
        {
            let o = r.output();
            if self.output(o).is_some() && !out.contains(o) {
                out.push(o.clone());
            }
        }
        out
    }
}
