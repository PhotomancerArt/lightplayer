//! A port's pixels as runs: one run per object patched to it, and the
//! unpatched stretches between them as runs of their own.
//!
//! lp2014 drew an output's channels in groups, one per mapped shape, with
//! the rest in plain chunks, and that is what made a long strip readable:
//! you could see where one panel stopped and the next began. The card does
//! the same. Each run starts on a fresh square with a gap before it, gets an
//! outline, and is where a patch edge lands — on the run, never on top of a
//! pixel.

use std::ops::Range;

use crate::domain::node::NodeId;
use crate::domain::object::ObjectId;
use crate::domain::project::Project;

/// One run of a port's pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct PortRun {
    /// The pixels, in wire order.
    pub pixels: Range<u32>,
    /// The object patched there, or `None` for unpatched pixels.
    pub object: Option<ObjectId>,
}

impl Project {
    /// A port's pixels, split into runs in wire order. Every pixel belongs
    /// to exactly one run.
    pub fn port_runs(&self, output: &NodeId, key: &str) -> Vec<PortRun> {
        let count = self.port_pixels(output, key);
        let mut spans: Vec<(u32, u32, ObjectId)> = self
            .effective_patch()
            .spans
            .iter()
            .filter(|s| &s.output == output && s.port == key)
            .map(|s| {
                // A reversed span runs down the wire from its start pixel.
                let lo = if s.reversed {
                    (s.start_pixel + 1).saturating_sub(s.len())
                } else {
                    s.start_pixel
                };
                (lo, lo + s.len(), s.object.clone())
            })
            .collect();
        spans.sort_by_key(|(lo, _, _)| *lo);

        let mut runs = Vec::new();
        let mut at = 0;
        for (lo, hi, object) in spans {
            let lo = lo.max(at).min(count);
            let hi = hi.min(count);
            if lo > at {
                runs.push(PortRun {
                    pixels: at..lo,
                    object: None,
                });
            }
            if hi > lo {
                runs.push(PortRun {
                    pixels: lo..hi,
                    object: Some(object),
                });
                at = hi;
            }
        }
        if at < count {
            runs.push(PortRun {
                pixels: at..count,
                object: None,
            });
        }
        runs
    }
}
