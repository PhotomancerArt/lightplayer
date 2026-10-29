//! Spans: what you select on an output (plan D12, D13).
//!
//! A span is a view, never stored and never a placement's target: the
//! projection of one fixture object onto one port — the pixels from its
//! first to its last there — or a gap, a stretch of the port no object
//! covers. Spans may overlap, since two objects can land on the same
//! pixels (a mistake on the way to right, or two fixtures where one is
//! off); each leads back to its own object. LightPlayer's core thinks in
//! control products and never in spans: this is the editor's handle on the
//! wire, the thing the output card lets you point at.

use std::ops::Range;

use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;
use crate::domain::project::Project;
use crate::domain::target::Target;

/// One object's pixels on one port, or a gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub output: NodeId,
    pub port: String,
    pub pixels: Range<u32>,
    /// `Some`: an object's lamps on these pixels. `None`: a gap.
    pub object: Option<ObjectId>,
}

impl Span {
    /// The target that selects it: an object's span by its object (so it
    /// stays selected while the object moves along the port), a gap by
    /// where it starts.
    pub fn target(&self) -> Target {
        Target::Span {
            output: self.output.clone(),
            key: self.port.clone(),
            object: self.object.clone(),
            start: if self.object.is_some() {
                0
            } else {
                self.pixels.start
            },
        }
    }
}

impl Project {
    /// A port's spans, in pixel order: each object's projection onto it,
    /// and the gaps nothing covers.
    pub fn spans(&self, output: &NodeId, port: &str) -> Vec<Span> {
        let count = self.port_pixels(output, port);
        let mut objects: Vec<Span> = Vec::new();
        for r in self
            .effective_patch()
            .pixel_runs
            .iter()
            .filter(|r| &r.output == output && r.port == port)
        {
            let px = r.pixels();
            let px = px.start.min(count)..px.end.min(count);
            if px.is_empty() {
                continue;
            }
            match objects
                .iter_mut()
                .find(|s| s.object.as_ref() == Some(&r.object))
            {
                Some(s) => {
                    s.pixels = s.pixels.start.min(px.start)..s.pixels.end.max(px.end);
                }
                None => objects.push(Span {
                    output: output.clone(),
                    port: port.to_string(),
                    pixels: px,
                    object: Some(r.object.clone()),
                }),
            }
        }
        let mut covered = vec![false; count as usize];
        for s in &objects {
            for px in s.pixels.clone() {
                covered[px as usize] = true;
            }
        }
        let mut out = objects;
        let mut px = 0u32;
        while px < count {
            if covered[px as usize] {
                px += 1;
                continue;
            }
            let start = px;
            while px < count && !covered[px as usize] {
                px += 1;
            }
            out.push(Span {
                output: output.clone(),
                port: port.to_string(),
                pixels: start..px,
                object: None,
            });
        }
        out.sort_by_key(|s| s.pixels.start);
        out
    }

    /// The span a span target selects, as the patch is now.
    pub fn span_of(&self, t: &Target) -> Option<Span> {
        let Target::Span {
            output,
            key,
            object,
            start,
        } = t
        else {
            return None;
        };
        self.spans(output, key).into_iter().find(|s| match object {
            Some(_) => &s.object == object,
            None => s.object.is_none() && s.pixels.start == *start,
        })
    }
}
