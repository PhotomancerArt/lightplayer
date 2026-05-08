//! Memory pressure level communicated to
//! [`NodeRuntime::handle_memory_pressure`](crate::node::NodeRuntime::handle_memory_pressure).

/// Coarse memory pressure tier for runtime shedding decisions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PressureLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_level_total_ordering() {
        assert!(PressureLevel::Low < PressureLevel::Medium);
        assert!(PressureLevel::Medium < PressureLevel::High);
        assert!(PressureLevel::High < PressureLevel::Critical);
    }
}
