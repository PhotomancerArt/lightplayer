#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RadioChannelId(u32);

impl RadioChannelId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RadioDeviceId(u32);

impl RadioDeviceId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RadioEventId(u32);

impl RadioEventId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RadioDrainReport {
    drained_count: usize,
    dropped_count: u32,
    overflowed: bool,
}

impl RadioDrainReport {
    pub const fn new(drained_count: usize, dropped_count: u32, overflowed: bool) -> Self {
        Self {
            drained_count,
            dropped_count,
            overflowed,
        }
    }

    pub const fn empty() -> Self {
        Self::new(0, 0, false)
    }

    pub const fn drained_count(self) -> usize {
        self.drained_count
    }

    pub const fn dropped_count(self) -> u32 {
        self.dropped_count
    }

    pub const fn overflowed(self) -> bool {
        self.overflowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_report_exposes_overflow_state() {
        let report = RadioDrainReport::new(3, 2, true);

        assert_eq!(report.drained_count(), 3);
        assert_eq!(report.dropped_count(), 2);
        assert!(report.overflowed());
    }
}
