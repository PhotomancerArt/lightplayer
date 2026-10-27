//! A circle: concentric rings round one centre, each ring its own segment
//! (`circle1/r3`).
//!
//! Beside today's ring set ([`RingShape`], concentric rings on ONE run, one
//! segment), the circle gives each ring a key, minted by the circle and kept
//! with the ring, so a ring's id survives edits to the others (F17). Each
//! ring's lamps are today's single-ring arithmetic; **a ring of one lamp is
//! one lamp at the centre, whatever its radius** (F7, D7): a circular
//! display's centre is its innermost ring.

use alloc::vec;
use alloc::vec::Vec;

use crate::map2d_doc::{RingDir, RingOrder, RingShape};
use crate::part::segment_key::SegmentKey;

/// Rings round a centre, one segment each.
#[derive(Debug, Clone, PartialEq)]
pub struct Circle {
    pub center: [f32; 2],
    /// Angle of each ring's lamp 0, in degrees; -90 is 12 o'clock
    /// (screen coordinates, y-down).
    pub start_angle_deg: f32,
    /// Clockwise or counter-clockwise, every ring.
    pub dir: RingDir,
    /// Which ring comes first ("drawn from" the outside or the inside).
    /// The rings are listed outside in; drawn from the inside, the list is
    /// taken last to first.
    pub drawn_from: RingOrder,
    /// Listed outside in.
    pub rings: Vec<Ring>,
    /// The number the next ring's key gets (`r<n>`). Never goes down.
    pub next_ring: u32,
}

/// One ring of a circle.
#[derive(Debug, Clone, PartialEq)]
pub struct Ring {
    /// Minted by its circle (`r1`, `r2`…) and kept.
    pub key: SegmentKey,
    pub radius: f32,
    /// Lamps on the ring. One lamp sits at the circle's centre.
    pub count: u32,
}

impl Circle {
    /// A circle with one ring per `(radius, count)`, listed outside in, keys
    /// `r1`, `r2`…, lamp 0 at 12 o'clock, clockwise.
    #[must_use]
    pub fn new(center: [f32; 2], rings: &[(f32, u32)]) -> Self {
        let mut circle = Self {
            center,
            start_angle_deg: -90.0,
            dir: RingDir::Cw,
            drawn_from: RingOrder::OuterFirst,
            rings: Vec::new(),
            next_ring: 1,
        };
        for &(radius, count) in rings {
            circle.add_ring(radius, count);
        }
        circle
    }

    /// Add a ring at the end of the list, with the next key. Returns it.
    pub fn add_ring(&mut self, radius: f32, count: u32) -> SegmentKey {
        let key = SegmentKey::ring(self.next_ring.max(1));
        self.next_ring = self.next_ring.max(1).saturating_add(1);
        self.rings.push(Ring {
            key: key.clone(),
            radius,
            count,
        });
        key
    }

    /// The rings in the order their lamps come: the list outside in, or
    /// last to first when drawn from the inside.
    pub fn rings_in_order(&self) -> impl Iterator<Item = &Ring> {
        let inner_first = self.drawn_from == RingOrder::InnerFirst;
        let n = self.rings.len();
        (0..n).map(move |i| &self.rings[if inner_first { n - 1 - i } else { i }])
    }

    /// One ring as today's single-ring ring set: the same radius and count,
    /// this circle's centre, start and direction. Its lamps are the ring's,
    /// unless it has one lamp (that one sits at the centre).
    #[must_use]
    pub fn ring_shape(&self, ring: &Ring) -> RingShape {
        RingShape {
            center: self.center,
            radius: ring.radius,
            outer_count: ring.count,
            rings: 1,
            counts: vec![ring.count],
            order: RingOrder::OuterFirst,
            start_angle_deg: self.start_angle_deg,
            dir: self.dir,
        }
    }
}
