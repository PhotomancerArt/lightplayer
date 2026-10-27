//! A circle: concentric rings round one centre (G1, 2026-10-27: one circle
//! part; today's ring set is a circle too).
//!
//! Each ring carries a key, minted by the circle and kept with the ring, so
//! a ring's id survives edits to the others (F17). By default each ring is
//! its own segment (`circle1/r3`); a circle split [`CircleSplit::One`] is
//! one segment for all its rings, which is what today's `ring` shape (a
//! ring set) is.
//!
//! **Where a ring's lamps sit.** Each ring's lamps are today's single-ring
//! arithmetic: lamp 0 at the start angle on the ring's radius, the rest
//! evenly round it. **A ring of radius 0 is the centre**, and holds one
//! lamp, at the circle's centre: a circular display's centre lamp is
//! its innermost ring, of radius 0. A ring of one lamp on a radius above 0
//! sits at the start angle on that radius, as every ring's lamp 0 does and
//! as today's ring set puts it, so a ring set becomes a circle with every
//! lamp where it was ([`Circle::from_ring_set`]).

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::map2d_doc::{Map2dShape, RingDir, RingOrder, RingShape};
use crate::map2d_resolve::shape_lamp_count;
use crate::part::circle_split::CircleSplit;
use crate::part::segment_key::SegmentKey;

/// Rings round a centre.
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
    /// One segment per ring, or one for them all.
    pub split: CircleSplit,
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
    /// 0 is the centre.
    pub radius: f32,
    /// Lamps on the ring. A ring at the centre holds one.
    pub count: u32,
}

impl Circle {
    /// A circle with one ring per `(radius, count)`, listed outside in, keys
    /// `r1`, `r2`…, lamp 0 at 12 o'clock, clockwise, one segment per ring.
    #[must_use]
    pub fn new(center: [f32; 2], rings: &[(f32, u32)]) -> Self {
        let mut circle = Self {
            center,
            start_angle_deg: -90.0,
            dir: RingDir::Cw,
            drawn_from: RingOrder::OuterFirst,
            split: CircleSplit::Rings,
            rings: Vec::new(),
            next_ring: 1,
        };
        for &(radius, count) in rings {
            circle.add_ring(radius, count);
        }
        circle
    }

    /// Today's ring set as a circle, every lamp where it was: one segment
    /// ([`CircleSplit::One`]), ring k at today's radius
    /// (`radius · (rings − k) / rings`, in today's arithmetic order) with
    /// today's count (its `counts` entry, else the count today derives from
    /// the circumference, read through today's [`shape_lamp_count`]), keys
    /// `r1`, `r2`…, its start angle, direction and order. `Err` says why
    /// today's resolver would refuse it.
    pub fn from_ring_set(ring: &RingShape) -> Result<Self, String> {
        if ring.outer_count == 0 {
            return Err(String::from("ring outer_count must be at least 1"));
        }
        if ring.radius <= 0.0 {
            return Err(String::from("ring radius must be positive"));
        }
        if ring.rings == 0 {
            return Err(String::from("ring needs at least one ring"));
        }
        let mut circle = Self {
            center: ring.center,
            start_angle_deg: ring.start_angle_deg,
            dir: ring.dir,
            drawn_from: ring.order,
            split: CircleSplit::One,
            rings: Vec::with_capacity(ring.rings as usize),
            next_ring: 1,
        };
        for k in 0..ring.rings {
            let radius = ring.radius * (ring.rings - k) as f32 / ring.rings as f32;
            circle.add_ring(radius, todays_ring_count(ring, k));
        }
        Ok(circle)
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
    /// this circle's centre, start and direction. Its lamps are the ring's
    /// (a ring at the centre excepted: today's ring set has none).
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

impl Ring {
    /// Is it the centre (radius 0)?
    #[must_use]
    pub fn is_centre(&self) -> bool {
        self.radius == 0.0
    }
}

/// Ring `k`'s lamp count in today's ring set, read through today's
/// [`shape_lamp_count`] rather than a copy of its arithmetic: every other
/// ring is given one lamp and ring `k` none (so today derives it), and the
/// others are taken off the total.
fn todays_ring_count(ring: &RingShape, k: u32) -> u32 {
    if let Some(&count) = ring.counts.get(k as usize).filter(|c| **c > 0) {
        return count;
    }
    let mut probe = ring.clone();
    probe.counts = (0..ring.rings).map(|i| u32::from(i != k)).collect();
    shape_lamp_count(&Map2dShape::Ring(probe)) - (ring.rings - 1)
}
