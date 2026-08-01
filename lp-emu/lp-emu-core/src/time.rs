//! Time mode for emulator time control

/// Time mode for controlling how time advances in the emulator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeMode {
    /// Use real wall-clock time (default behavior)
    RealTime,
    /// Use simulated time that can be advanced manually
    Simulated(u32), // Current simulated time in milliseconds
}

impl Default for TimeMode {
    fn default() -> Self {
        TimeMode::RealTime
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_mode_default() {
        assert_eq!(TimeMode::default(), TimeMode::RealTime);
    }

    #[test]
    fn test_time_mode_simulated() {
        let mode = TimeMode::Simulated(100);
        match mode {
            TimeMode::Simulated(ms) => assert_eq!(ms, 100),
            _ => panic!("Expected Simulated mode"),
        }
    }
}
