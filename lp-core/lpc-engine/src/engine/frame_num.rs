//! Monotonic engine tick counter.

/// Number of engine frames/ticks that have been processed.
///
/// `FrameNum` is local execution time: it answers "which engine tick are we
/// on?" It is deliberately separate from `lpc_model::Revision`, which stamps
/// changes in synchronized state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrameNum(u64);

impl FrameNum {
    /// Create a frame number from a raw monotonic value.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Return the raw monotonic frame number.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Return the next frame number.
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Default for FrameNum {
    fn default() -> Self {
        Self(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_advances_frame_number() {
        assert_eq!(FrameNum::new(7).next(), FrameNum::new(8));
    }
}
