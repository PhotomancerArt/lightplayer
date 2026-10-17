//! A ring set: the product's ring of several concentric rings (`map2d`
//! `ring` with `rings` > 1, or with its own `counts`, or a lamp 0 away from
//! 12 o'clock), a whole object (plan M4).
//!
//! In the product a ring set is ONE strand: its rings run one after the
//! other, outer first (or inner first), on one wire. The lab's circle makes
//! one object per ring instead; which the lab should draw new rings as is
//! the deferred format question (plan Q1). A ring set opened from a file
//! stays as the product resolves it (plan D9).
//!
//! Ring `k` of `n` sits at `radius · (n − k) / n`; its lamp count is the
//! `counts` entry for it, or else the outer count scaled by circumference
//! (`resolve::ring_set_lamps`).

use crate::domain::geom::Vec2;

#[derive(Debug, Clone, PartialEq)]
pub struct RingSet {
    pub center: Vec2,
    /// The outer ring's radius.
    pub radius: f64,
    /// Lamps on the outer ring.
    pub outer_lamps: u32,
    /// How many rings, spaced evenly in from `radius`.
    pub rings: u32,
    /// Lamps per ring, outer to inner, where a ring has its own; a missing
    /// or zero entry falls back to the circumference-derived count.
    pub counts: Vec<u32>,
    /// The inner ring is wired first.
    pub inner_first: bool,
    /// Where lamp 0 sits on each ring, in degrees; −90 is 12 o'clock
    /// (y-down).
    pub start_angle_deg: f64,
    /// Data runs counter-clockwise.
    pub reversed: bool,
}

impl RingSet {
    /// The outer ring's lamps: its own `counts` entry when it has one,
    /// else `outer_lamps`.
    pub fn outer_ring_lamps(&self) -> u32 {
        self.counts
            .first()
            .copied()
            .filter(|c| *c > 0)
            .unwrap_or(self.outer_lamps)
    }

    /// Give the outer ring `n` lamps: `outer_lamps`, and its own `counts`
    /// entry when it has one (that entry is what the product reads).
    pub fn set_outer_ring_lamps(&mut self, n: u32) {
        self.outer_lamps = n;
        if let Some(c) = self.counts.first_mut().filter(|c| **c > 0) {
            *c = n;
        }
    }

    /// Each ring's radius, outer first.
    pub fn radii(&self) -> Vec<f64> {
        let n = self.rings.max(1);
        (0..n)
            .map(|k| self.radius * (n - k) as f64 / n as f64)
            .collect()
    }
}
