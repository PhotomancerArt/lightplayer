//! Where along its arrow a label would like to sit.
//!
//! A preference, not an order: if that spot is bent, crowded or too near
//! a head, placement slides the label to the nearest spot that works and
//! says it moved.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Halfway along.
    Centre,
    /// As near the start as fits.
    NearStart,
    /// As near the end as fits.
    NearEnd,
    /// This fraction of the way along (0 start, 1 end).
    At(f64),
}

impl Placement {
    /// The fraction of the arrow's length it prefers.
    pub fn fraction(self) -> f64 {
        match self {
            Placement::Centre => 0.5,
            Placement::NearStart => 0.0,
            Placement::NearEnd => 1.0,
            Placement::At(t) => t.clamp(0.0, 1.0),
        }
    }
}
