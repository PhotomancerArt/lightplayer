//! The lab's debug switches: things being tried, reachable from the Debug
//! control at the toolbar's right end, never in the way.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugFlags {
    /// Dragging a node pins it, and the layout honours pins (D48). Off,
    /// the layout is automatic only: the trial of D64.
    pub pinning: bool,
}

impl Default for DebugFlags {
    fn default() -> Self {
        Self { pinning: true }
    }
}
