//! What the frame plans did this frame.

/// Plan work during one frame, reset when the next frame begins (like
/// [`crate::dataflow::resolver::ResolveFrameCounters`]). A steady frame over
/// an unchanged graph has `builds == 0`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FramePlanCounters {
    /// Plans built because their key moved (or the mode rebuilds every
    /// frame).
    pub builds: u32,
    /// Plans replayed without rebuilding.
    pub replays: u32,
    /// Replays that were also rebuilt and compared ([`super::PlanMode::Check`]).
    pub checks: u32,
    /// Checks whose rebuilt plan differed from the replayed one: missed
    /// invalidations.
    pub check_mismatches: u32,
}

impl FramePlanCounters {
    /// Work only an edit should cause: plans built.
    pub fn structural_work(&self) -> u32 {
        self.builds
    }
}
