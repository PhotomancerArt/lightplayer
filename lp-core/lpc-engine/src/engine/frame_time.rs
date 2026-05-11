//! Engine frame timing counters.

/// Millisecond timing for the current engine frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameTime {
    /// Time since last frame in milliseconds.
    pub delta_ms: u32,
    /// Total time since project start in milliseconds.
    pub total_ms: u32,
}

impl FrameTime {
    /// Create frame timing from a delta and accumulated total.
    pub fn new(delta_ms: u32, total_ms: u32) -> Self {
        Self { delta_ms, total_ms }
    }

    /// Create zeroed frame timing.
    pub fn zero() -> Self {
        Self {
            delta_ms: 0,
            total_ms: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_time_creation() {
        let time = FrameTime::new(16, 1000);
        assert_eq!(time.delta_ms, 16);
        assert_eq!(time.total_ms, 1000);
    }

    #[test]
    fn test_frame_time_zero() {
        let time = FrameTime::zero();
        assert_eq!(time.delta_ms, 0);
        assert_eq!(time.total_ms, 0);
    }

    #[test]
    fn test_frame_time_equality() {
        let time1 = FrameTime::new(16, 1000);
        let time2 = FrameTime::new(16, 1000);
        let time3 = FrameTime::new(17, 1000);

        assert_eq!(time1, time2);
        assert_ne!(time1, time3);
    }
}
