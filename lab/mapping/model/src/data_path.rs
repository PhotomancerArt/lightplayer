//! The data path: the order the wire runs through a fixture's objects.
//!
//! The patch says which lamp lands on which pixel; drawn inside the
//! fixture, it shows how the strip is wired. Wherever one object's lamps
//! stop on a port and the next object's start on the very next pixel, the
//! wire runs from the earlier object's LAST lamp to the next one's FIRST.
//! Where a port starts (its pixel 0), the data comes in.
//!
//! Like everything read off the patch, it is derived, never stored, so an
//! edit — a reversed path, a count, a deleted ring — redraws it at once.

use crate::geom::Vec2;
use crate::node::NodeId;
use crate::project::Project;

/// One fixture's wiring, in world units.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataPath {
    /// Wire from one object to the next, in patch order: from the earlier
    /// object's last lamp to the next object's first.
    pub links: Vec<(Vec2, Vec2)>,
    /// Where data comes into the fixture from a port: the lamp on the
    /// port's first pixel, and which way its object runs from there.
    pub entries: Vec<DataEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataEntry {
    pub at: Vec2,
    /// A unit vector along the object from its first lamp.
    pub along: Vec2,
}

impl Project {
    /// The data path through one fixture.
    pub fn data_path(&self, fixture: &NodeId) -> DataPath {
        let patch = self.effective_patch();
        let objects = self.objects_in(fixture);
        let lamp = |object: &crate::ObjectId, i: u32| -> Option<Vec2> {
            objects
                .iter()
                .find(|o| &o.id == object)
                .and_then(|o| o.lamps.get(i as usize))
                .map(|l| l.pos)
        };
        let mut out = DataPath::default();
        // Every span, in patch order (per port, by pixel).
        let spans: Vec<_> = patch.spans.iter().collect();
        for s in spans.iter().filter(|s| &s.fixture == fixture) {
            if s.start_pixel == 0
                && let Some(at) = lamp(&s.object, s.lamps.start)
            {
                let next = lamp(&s.object, s.lamps.start + 1).unwrap_or(at);
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
            // The span that continues this one on the next pixel.
            let end = s.start_pixel + s.lamps.len() as u32;
            let Some(next) = spans.iter().find(|n| {
                n.output == s.output
                    && n.port == s.port
                    && n.start_pixel == end
                    && &n.fixture == fixture
                    && n.object != s.object
            }) else {
                continue;
            };
            if let (Some(a), Some(b)) = (
                lamp(&s.object, s.lamps.end.saturating_sub(1)),
                lamp(&next.object, next.lamps.start),
            ) {
                out.links.push((a, b));
            }
        }
        out
    }
}
