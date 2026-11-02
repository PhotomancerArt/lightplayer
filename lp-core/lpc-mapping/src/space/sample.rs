//! What T makes of one lamp.

/// Where a lamp lands in a source (Q16.16 source coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sample {
    /// The lamp is outside the fixture's box: dark, never sampled (theory
    /// D12, D14).
    Dark,
    /// Inside the source: a view's whole plane, or within a bounded
    /// extent (edges included).
    At([i32; 2]),
    /// Past a bounded extent, before the fixture's outside rule decides
    /// (`SpaceTransform::resolve`).
    Outside([i32; 2]),
}
