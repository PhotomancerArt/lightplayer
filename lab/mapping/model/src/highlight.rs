//! Highlights: what the selection lights up across the patch, both ways.
//!
//! Select a port and the lamps it carries light up; select an object, a
//! fixture or a module and the ports it lands on light up. Both are read
//! off the effective patch, so they are always what the patch really does.

use crate::node::NodeId;
use crate::object::ObjectId;
use crate::patch::Span;
use crate::project::Project;
use crate::target::Target;

impl Project {
    /// Every lamp one port carries, as (object, lamp index), in pixel order.
    pub fn lamps_on_port(&self, output: &NodeId, key: &str) -> Vec<(ObjectId, u32)> {
        self.effective_patch()
            .spans
            .iter()
            .filter(|s| &s.output == output && s.port == key)
            .flat_map(|s| s.lamps.clone().map(move |i| (s.object.clone(), i)))
            .collect()
    }

    /// The ports a target's lamps land on, in patch order. A port or an
    /// output lands on nothing.
    pub fn ports_of(&self, t: &Target) -> Vec<Target> {
        let mut out: Vec<Target> = Vec::new();
        for s in self.effective_patch().spans {
            if !self.span_belongs_to(&s, t) {
                continue;
            }
            let port = Target::Port {
                output: s.output.clone(),
                key: s.port.clone(),
            };
            if !out.contains(&port) {
                out.push(port);
            }
        }
        out
    }

    /// The outputs a fixture's lamps land on, in patch order.
    pub fn outputs_of(&self, fixture: &NodeId) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = Vec::new();
        for s in self.effective_patch().spans {
            if &s.fixture == fixture && !out.contains(&s.output) {
                out.push(s.output);
            }
        }
        out
    }

    fn span_belongs_to(&self, s: &Span, t: &Target) -> bool {
        match t {
            Target::Lamp { object, index } => &s.object == object && s.lamps.contains(index),
            Target::Port { .. } => false,
            Target::Node(id) if self.output(id).is_some() => false,
            // The span's object, and everything above it up to the root.
            _ => self.is_ancestor_or_self(t, &self.object_target(&s.object)),
        }
    }
}
