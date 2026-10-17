//! The data path: the order the wire runs through a fixture's objects.
//!
//! The patch says which lamp lands on which pixel; drawn inside the
//! fixture, it shows how the strip is wired. Walking a port pixel by pixel,
//! wherever the lamp on one pixel belongs to one object and the lamp on the
//! next pixel to another, the wire runs from the first to the second. Where
//! a port starts (its pixel 0), the data comes in.
//!
//! Like everything read off the patch, it is derived, never stored, so an
//! edit — a reversed path, a count, a deleted ring — redraws it at once.

use std::collections::HashMap;

use crate::domain::geom::Vec2;
use crate::domain::node::NodeId;
use crate::domain::patch::Span;
use crate::domain::project::Project;

/// One fixture's wiring, in world units.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataPath {
    /// Wire from one object to the next, in patch order: from the lamp on
    /// one object's last pixel to the lamp on the next object's first.
    pub links: Vec<(Vec2, Vec2)>,
    /// Where data comes into the fixture from a port: the lamp on the
    /// port's first pixel, and which way the wire runs on from there.
    pub entries: Vec<DataEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataEntry {
    pub at: Vec2,
    /// A unit vector toward the lamp on the next pixel.
    pub along: Vec2,
}

impl Project {
    /// The data path through one fixture.
    pub fn data_path(&self, fixture: &NodeId) -> DataPath {
        let patch = self.effective_patch();
        let lamp = |object: &crate::ObjectId, i: u32| -> Option<Vec2> {
            self.object(object)
                .and_then(|o| o.lamps.get(i as usize))
                .map(|l| l.pos)
        };
        let on = |s: &Span, pixel: u32| s.lamp_at(pixel).and_then(|i| lamp(&s.object, i));
        let mut out = DataPath::default();
        let mine: Vec<&Span> = patch
            .spans
            .iter()
            .filter(|s| &s.fixture == fixture)
            .collect();
        // Where each span starts: (output, port, first pixel) → its spans.
        let mut starting: HashMap<(&NodeId, &str, u32), Vec<&Span>> = HashMap::new();
        for s in &mine {
            starting
                .entry((&s.output, s.port.as_str(), s.pixels().start))
                .or_default()
                .push(s);
        }
        for s in &mine {
            let pixels = s.pixels();
            if pixels.start == 0
                && let Some(at) = on(s, 0)
            {
                let next = on(s, 1).unwrap_or(at);
                let d = next - at;
                let len = d.length();
                out.entries.push(DataEntry {
                    at,
                    along: if len > 1e-9 {
                        d * (1.0 / len)
                    } else {
                        Vec2::new(1.0, 0.0)
                    },
                });
            }
            // The span that carries on from this one's last pixel.
            let Some(next) = starting
                .get(&(&s.output, s.port.as_str(), pixels.end))
                .and_then(|ns| ns.iter().find(|n| n.object != s.object))
            else {
                continue;
            };
            if let (Some(a), Some(b)) = (on(s, pixels.end - 1), on(next, pixels.end)) {
                out.links.push((a, b));
            }
        }
        out
    }
}
