//! How an arrow gets from one end to the other.
//!
//! Straight, a smooth cubic whose handles leave and arrive the way the
//! [`EndDirection`](crate::EndDirection)s say, or a gutter route: out
//! sideways to a vertical lane, along it, and back in sideways, with
//! rounded corners (the stack layout's edges, architecture §6.2). Each
//! comes back as segments in a [`Path`](crate::Path).

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Route {
    /// A straight line; the end directions are ignored.
    Straight,
    /// A cubic Bézier.
    Curve(Bend),
    /// Out of the start sideways to a lane, along it, and into the end
    /// sideways; the end directions are ignored.
    Gutter(Gutter),
}

/// Where a gutter route's lane runs, and how round its corners are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gutter {
    /// The lane's x, in the caller's units.
    pub lane_x: f64,
    /// The corners' radius; smaller where the legs are too short for it.
    pub radius: f64,
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
