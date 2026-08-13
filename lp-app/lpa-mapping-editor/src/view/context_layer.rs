//! Dimmed neighbour fixtures rendered UNDER the edited document — the
//! "dive into a fixture, others still visible" ruling. The host supplies
//! each neighbour's lamp points already transformed into the edited
//! document's own space (this crate stays project-unaware: it renders
//! points, it does not know what an arrangement is).

/// One neighbour fixture's render-only facts.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextFixture {
    /// Object colour (CSS), matching the fixture's colour everywhere else.
    pub color: String,
    /// Lamp points in the EDITED document's doc space.
    pub points: Vec<[f32; 2]>,
}
