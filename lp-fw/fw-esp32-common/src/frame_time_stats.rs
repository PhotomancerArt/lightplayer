//! Frame times for the `[perf]` line: the slowest frame of the interval
//! and where it went, a coarse p50/p99, and how many frames crossed the
//! hiccup bounds (Yona's frame-rate budget, PR B: idle hiccups ≤ 100 ms,
//! editing ≤ 1 s).
//!
//! A frame is one whole server-loop iteration (receive, tick and send,
//! heartbeats, upkeep, the 1 ms yield): what a viewer of the LEDs waits
//! for. The cost is a handful of integer operations per frame and one
//! 14-entry histogram; nothing allocates, and nothing goes on the wire.

/// Upper bounds of the histogram's buckets, milliseconds; the last bucket
/// holds everything slower.
const BUCKET_MS: [u32; 13] = [8, 16, 25, 33, 50, 75, 100, 150, 200, 300, 500, 1000, 2000];

/// The idle hiccup bound (frames slower than this are counted).
pub const HICCUP_IDLE_MS: u32 = 100;
/// The editing hiccup bound.
pub const HICCUP_EDIT_MS: u32 = 1000;

/// One `[perf]` interval's frame times.
#[derive(Clone, Debug, Default)]
pub struct FrameTimeStats {
    buckets: [u32; BUCKET_MS.len() + 1],
    frames: u32,
    max: SlowFrame,
}

/// The slowest frame and its phases, milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlowFrame {
    pub total_ms: u32,
    pub recv_ms: u32,
    pub tick_ms: u32,
    pub send_ms: u32,
    /// Requests answered in that frame.
    pub responses: u32,
}

impl FrameTimeStats {
    /// Count one frame.
    pub fn record(&mut self, frame: SlowFrame) {
        let bucket = BUCKET_MS
            .iter()
            .position(|&bound| frame.total_ms <= bound)
            .unwrap_or(BUCKET_MS.len());
        self.buckets[bucket] += 1;
        self.frames += 1;
        if frame.total_ms > self.max.total_ms {
            self.max = frame;
        }
    }

    /// The bucket bound under which `percent` of the frames fell (`None`
    /// when no frame was counted, or the share lies in the open last bucket).
    pub fn percentile_ms(&self, percent: u32) -> Option<u32> {
        if self.frames == 0 {
            return None;
        }
        let wanted = (u64::from(self.frames) * u64::from(percent)).div_ceil(100);
        let mut seen = 0u64;
        for (index, count) in self.buckets.iter().enumerate() {
            seen += u64::from(*count);
            if seen >= wanted {
                return BUCKET_MS.get(index).copied();
            }
        }
        None
    }

    /// Frames slower than `bound_ms` (a bound from [`BUCKET_MS`]).
    pub fn over(&self, bound_ms: u32) -> u32 {
        BUCKET_MS
            .iter()
            .position(|&bound| bound == bound_ms)
            .map(|index| self.buckets[index + 1..].iter().sum())
            .unwrap_or(0)
    }

    /// The slowest frame.
    pub fn slowest(&self) -> SlowFrame {
        self.max
    }

    /// Start the next interval.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl core::fmt::Display for FrameTimeStats {
    /// `p50≤16ms p99≤100ms max=187ms(recv=0 tick=186 send=1 resp=1) >100ms=2 >1s=0`
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let bound = |p| match self.percentile_ms(p) {
            Some(ms) => alloc::format!("≤{ms}ms"),
            None => alloc::string::String::from(">2s"),
        };
        let max = self.max;
        write!(
            f,
            "p50{} p99{} max={}ms(recv={} tick={} send={} resp={}) >{HICCUP_IDLE_MS}ms={} >1s={}",
            bound(50),
            bound(99),
            max.total_ms,
            max.recv_ms,
            max.tick_ms,
            max.send_ms,
            max.responses,
            self.over(HICCUP_IDLE_MS),
            self.over(HICCUP_EDIT_MS),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(total_ms: u32) -> SlowFrame {
        SlowFrame {
            total_ms,
            tick_ms: total_ms,
            ..SlowFrame::default()
        }
    }

    #[test]
    fn the_interval_names_its_slowest_frame_and_its_hiccups() {
        let mut stats = FrameTimeStats::default();
        for _ in 0..98 {
            stats.record(frame(15));
        }
        stats.record(frame(120));
        stats.record(frame(1_500));
        assert_eq!(stats.percentile_ms(50), Some(16));
        assert_eq!(stats.percentile_ms(99), Some(150));
        assert_eq!(stats.over(HICCUP_IDLE_MS), 2);
        assert_eq!(stats.over(HICCUP_EDIT_MS), 1);
        assert_eq!(stats.slowest().total_ms, 1_500);
        let line = alloc::format!("{stats}");
        assert!(
            line.contains("max=1500ms") && line.contains(">100ms=2"),
            "{line}"
        );
        stats.reset();
        assert_eq!(stats.percentile_ms(50), None);
    }
}
