//! Whether two mappings draw the same lamps: everything [`render`] reads is
//! equal, whatever their patching says.
//!
//! One file holds both a fixture's layout and its patching (G1, Q14), so an
//! edit to a start, a flow or an override rewrites the whole document. A
//! reader holding a rendered tree asks this to keep it: the same layout
//! renders the same tree, and only the patching needs doing again.
//!
//! The layout is the sample diameter, the canvas, the fixture's lamp facts,
//! and every part's id, name, lamp facts and kind (its shape, a group's
//! parts, a repeat's centre, count and part), compared in order. The
//! patching is the fixture's and every part's start and flow, the
//! overrides, and the id counters (which mint ids, and draw nothing).
//!
//! [`render`]: crate::part::render

use crate::part::mapping::Mapping;
use crate::part::part::Part;
use crate::part::part_kind::PartKind;

impl Mapping {
    /// Whether `other` draws the same lamps as this mapping (module doc).
    #[must_use]
    pub fn same_layout(&self, other: &Mapping) -> bool {
        self.sample_diameter == other.sample_diameter
            && self.canvas == other.canvas
            && self.lamp == other.lamp
            && same_parts(&self.parts, &other.parts)
    }
}

fn same_parts(a: &[Part], b: &[Part]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_part(a, b))
}

fn same_part(a: &Part, b: &Part) -> bool {
    a.id == b.id
        && a.name == b.name
        && a.lamp == b.lamp
        && match (&a.kind, &b.kind) {
            (PartKind::Shape(a), PartKind::Shape(b)) => a == b,
            (PartKind::Group { parts: a }, PartKind::Group { parts: b }) => same_parts(a, b),
            (PartKind::Repeat(a), PartKind::Repeat(b)) => {
                a.center == b.center && a.count == b.count && same_part(&a.part, &b.part)
            }
            _ => false,
        }
}
