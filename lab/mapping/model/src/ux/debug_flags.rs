//! The lab's debug switches: things being tried, reachable from the Debug
//! control at the toolbar's right end, never in the way.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DebugFlags {
    /// Dragging a node pins it, and the layout honours pins (D48). Off by
    /// default: the layout is automatic only, on trial (D64), and every
    /// reach for a drag is a finding.
    pub pinning: bool,
}
