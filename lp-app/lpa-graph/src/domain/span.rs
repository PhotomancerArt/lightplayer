//! Spans: what you select on an output (plan D12, D13).
//!
//! A span is a view, never stored and never a placement's target: the
//! projection of one fixture segment onto one port — the pixels from its
//! first to its last there — or a gap, a stretch of the port no segment
//! covers. Spans may overlap, since two segments can land on the same
//! pixels (a mistake on the way to right, or two fixtures where one is
//! off); each leads back to its own segment. LightPlayer's core thinks in
//! control products and never in spans: this is the editor's handle on the
//! wire, the thing the output card lets you point at.

use std::ops::Range;

use crate::domain::node::NodeId;
use crate::domain::part_ref::TreeRef;
use crate::domain::project::Project;
use crate::domain::target::Target;

/// One segment's pixels on one port, or a gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub output: NodeId,
    pub port: String,
    pub pixels: Range<u32>,
    /// `Some`: a segment's lamps on these pixels. `None`: a gap.
    pub segment: Option<TreeRef>,
}

impl Span {
    /// The target that selects it: a segment's span by its segment (so it
    /// stays selected while the segment moves along the port), a gap by
    /// where it starts.
    pub fn target(&self) -> Target {
        Target::Span {
            output: self.output.clone(),
            key: self.port.clone(),
            segment: self.segment.clone(),
            start: if self.segment.is_some() {
                0
            } else {
                self.pixels.start
            },
        }
    }
}

impl Project {
    /// A port's spans, in pixel order: each segment's projection onto it,
    /// and the gaps nothing covers.
    pub fn spans(&self, output: &NodeId, port: &str) -> Vec<Span> {
        let count = self.port_pixels(output, port);
        let mut segments: Vec<Span> = Vec::new();
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
            match segments
                .iter_mut()
                .find(|s| s.segment.as_ref() == Some(&r.segment))
            {
                Some(s) => {
                    s.pixels = s.pixels.start.min(px.start)..s.pixels.end.max(px.end);
                }
                None => segments.push(Span {
                    output: output.clone(),
                    port: port.to_string(),
                    pixels: px,
                    segment: Some(r.segment.clone()),
                }),
            }
        }
        let mut covered = vec![false; count as usize];
        for s in &segments {
            for px in s.pixels.clone() {
                covered[px as usize] = true;
            }
        }
        let mut out = segments;
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
                segment: None,
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
            segment,
            start,
        } = t
        else {
            return None;
        };
        self.spans(output, key).into_iter().find(|s| match segment {
            Some(_) => &s.segment == segment,
            None => s.segment.is_none() && s.pixels.start == *start,
        })
    }
}
