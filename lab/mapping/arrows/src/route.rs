//! How an arrow gets from one end to the other.
//!
//! Straight, or a smooth cubic whose handles leave and arrive the way the
//! [`EndDirection`](crate::EndDirection)s say. Orthogonal (elbow) routes
//! are the next kind to add; they will come back as line segments in the
//! same [`Path`](crate::Path).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Route {
    /// A straight line; the end directions are ignored.
    Straight,
    /// A cubic Bézier.
    Curve(Bend),
}

/// How far a curve's handles reach out from its ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bend {
    /// A fraction of the distance between the ends.
    pub reach: f64,
    /// Never less than this, in the caller's units, so short arrows still
    /// leave and arrive the way they should.
    pub min: f64,
}

impl Default for Bend {
    /// The lab's edges: 0.4 of the distance, at least 20.
    fn default() -> Self {
        Bend {
            reach: 0.4,
            min: 20.0,
        }
    }
}

impl Default for Route {
    fn default() -> Self {
        Route::Curve(Bend::default())
    }
}
