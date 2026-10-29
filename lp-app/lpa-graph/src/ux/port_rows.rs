//! A port as one row of spans (names.html "Span", "Gap"; lean Q84): every
//! fixture's segments on that output and port, merged, and the gaps
//! between them recomputed over the union.
//!
//! The patcher answers each fixture on its own (`Patching::ports`), but an
//! output may carry several fixtures, and a person reads the port, not the
//! fixture: so the row is the union of every fixture's segment spans on it.
//! Where two segments share channels (two fixtures, or one fixture's hand
//! patch on its way to right) both stay in the row, and the shared
//! channels are marked as an overlap (`problem_words.rs` lists it on both
//! fixtures).
//!
//! Widths are channels, as the port counts them; what a person reads is
//! lamps (`docs/ux/README.md` P15): a segment's own lamps, a gap's lamps of
//! the port's three channels (`PORT_CHANNELS_PER_LAMP`).

use std::ops::Range;

use lpc_mapping::patching::Span;

use crate::domain::node::NodeId;
use crate::domain::part_ref::TreeRef;
use crate::domain::port_tables::PORT_CHANNELS_PER_LAMP;
use crate::domain::project::Project;

/// One port, as a row of spans in channel order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortRow {
    pub output: NodeId,
    pub port: String,
    /// Segments and gaps, by first channel. Segments may overlap one
    /// another; a gap is what no segment covers.
    pub spans: Vec<RowSpan>,
    /// Where two segments share channels.
    pub overlaps: Vec<RowOverlap>,
    /// How many channels the port has: its count, or for a last port that
    /// takes the rest, as far as anything reaches on it.
    pub channels: u32,
}

/// A stretch of a port's row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowSpan {
    pub channels: Range<u32>,
    /// How many lamps it is: a segment's own, a gap's at the port's three
    /// channels a lamp.
    pub lamps: u32,
    /// The segment patched there; `None` for a gap.
    pub segment: Option<TreeRef>,
}

/// Channels of one port two segments both land on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowOverlap {
    pub channels: Range<u32>,
    /// In the patch's order (fixtures in attach order, then channel).
    pub segments: [TreeRef; 2],
}

impl RowSpan {
    /// The span's pixels on the port's strip: its channels in lamps of the
    /// port's three channels, a part lamp counted whole.
    pub fn pixels(&self) -> Range<u32> {
        pixels_of(&self.channels)
    }

    /// "ring r3 · 12 lamps", "gap · 16 lamps".
    pub fn words(&self, p: &Project) -> String {
        let lamps = lamps_words(self.lamps);
        match &self.segment {
            Some(s) => format!("{} · {lamps}", p.segment_words(s)),
            None => format!("gap · {lamps}"),
        }
    }
}

impl Project {
    /// One port of an output as a row of spans, every fixture on it merged.
    pub fn port_row(&self, output: &NodeId, key: &str) -> PortRow {
        let name = self.output(output).and_then(|o| o.label.clone());
        let patch = self.effective_patch();
        let mut segments: Vec<RowSpan> = Vec::new();
        for (fixture, patching) in &patch.patchings {
            let Some(name) = name.as_deref() else { break };
            for row in patching
                .ports
                .iter()
                .filter(|r| r.output.as_str() == name && r.port.0.to_string() == key)
            {
                for span in &row.spans {
                    let Span::Segment { id, at, channels } = span else {
                        continue;
                    };
                    if *channels == 0 {
                        continue;
                    }
                    let lamps = patching
                        .segment(id)
                        .map_or(0, |s| (*channels).div_ceil(s.channels_per_lamp.max(1)));
                    segments.push(RowSpan {
                        channels: *at..at + channels,
                        lamps,
                        segment: Some(TreeRef::new(fixture, id.clone())),
                    });
                }
            }
        }
        // The patch's order breaks a tie at one channel.
        segments.sort_by_key(|s| s.channels.start);

        let mut overlaps = Vec::new();
        for (i, a) in segments.iter().enumerate() {
            for b in &segments[i + 1..] {
                let shared =
                    a.channels.start.max(b.channels.start)..a.channels.end.min(b.channels.end);
                if shared.is_empty() {
                    continue;
                }
                if let (Some(sa), Some(sb)) = (&a.segment, &b.segment) {
                    overlaps.push(RowOverlap {
                        channels: shared,
                        segments: [sa.clone(), sb.clone()],
                    });
                }
            }
        }

        let reach = segments.iter().map(|s| s.channels.end).max().unwrap_or(0);
        let channels = match self.output(output).and_then(|o| o.port(key)) {
            Some(port) => port
                .pixels
                .map_or(reach, |n| n.saturating_mul(PORT_CHANNELS_PER_LAMP)),
            None => 0,
        };
        let mut spans = Vec::new();
        let mut at = 0u32;
        for s in segments {
            if s.channels.start > at && at < channels {
                let end = s.channels.start.min(channels);
                spans.push(gap(at..end));
            }
            at = at.max(s.channels.end);
            spans.push(s);
        }
        if at < channels {
            spans.push(gap(at..channels));
        }
        PortRow {
            output: output.clone(),
            port: key.to_string(),
            spans,
            overlaps,
            channels,
        }
    }

    /// Every overlap on every port of every output, each once.
    pub fn row_overlaps(&self) -> Vec<(NodeId, String, RowOverlap)> {
        let mut out = Vec::new();
        for n in self.outputs() {
            let Some(o) = n.output() else { continue };
            for port in &o.ports {
                for x in self.port_row(&n.id, &port.key).overlaps {
                    out.push((n.id.clone(), port.key.clone(), x));
                }
            }
        }
        out
    }
}

fn gap(channels: Range<u32>) -> RowSpan {
    RowSpan {
        lamps: (channels.end - channels.start).div_ceil(PORT_CHANNELS_PER_LAMP),
        channels,
        segment: None,
    }
}

/// Channels as the strip's pixels: a part pixel counted whole.
pub fn pixels_of(channels: &Range<u32>) -> Range<u32> {
    channels.start / PORT_CHANNELS_PER_LAMP..channels.end.div_ceil(PORT_CHANNELS_PER_LAMP)
}

/// "1 lamp", "12 lamps".
pub fn lamps_words(n: u32) -> String {
    if n == 1 {
        "1 lamp".into()
    } else {
        format!("{n} lamps")
    }
}
