//! When the server lets the filesystem clean up (`LpFs::idle_step`): on a
//! **quiet** tick — no request that changes files for [`FS_IDLE_QUIET_MS`]
//! and no fs batch open — at most one step every [`FS_IDLE_STEP_EVERY_MS`],
//! after the frame. Reads and polls do not end the quiet: Studio reads a
//! board all the time it is open, and a step only delays a read by its own
//! few tens of ms. A step is bounded (on the tree store: a mark, then one sector
//! forgotten or collected, tens of ms on a C6), so a quiet board spends at
//! most about a tenth of its time on it, and only while there is cleanup to
//! do: a filesystem with none answers at once.
//!
//! The clock is the frames' own `delta_ms` (sans-IO: no clock is read
//! here), counted as the batch counts its idle time: the delta after a tick
//! that handled a file-changing request is that request's time, not quiet
//! time.

/// No file-changing request for this long (handling time excluded) makes a
/// tick quiet.
pub const FS_IDLE_QUIET_MS: u64 = 2_000;

/// At most one idle step per this much quiet time.
pub const FS_IDLE_STEP_EVERY_MS: u64 = 500;

/// The quiet clock.
#[derive(Debug, Default)]
pub struct FsIdle {
    quiet_ms: u64,
    since_step_ms: u64,
    /// The tick that is ending handled a file-changing request: the next
    /// delta is that request's time.
    last_tick_busy: bool,
}

impl FsIdle {
    pub fn new() -> Self {
        Self::default()
    }

    /// One tick: `busy` = it handled a request that changes files, or a
    /// batch is open. `true` when this tick should run one idle step.
    pub fn tick(&mut self, delta_ms: u32, busy: bool) -> bool {
        let after_busy = core::mem::replace(&mut self.last_tick_busy, busy);
        if busy {
            self.quiet_ms = 0;
            self.since_step_ms = 0;
            return false;
        }
        if !after_busy {
            self.quiet_ms = self.quiet_ms.saturating_add(u64::from(delta_ms));
            self.since_step_ms = self.since_step_ms.saturating_add(u64::from(delta_ms));
        }
        if self.quiet_ms < FS_IDLE_QUIET_MS || self.since_step_ms < FS_IDLE_STEP_EVERY_MS {
            return false;
        }
        self.since_step_ms = 0;
        true
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn quiet_for_two_seconds_then_a_step_every_half_second() {
        let mut idle = FsIdle::new();
        let mut steps = Vec::new();
        for t in 0..200u32 {
            if idle.tick(25, false) {
                steps.push((t + 1) * 25);
            }
        }
        assert_eq!(&steps[..4], &[2000, 2500, 3000, 3500]);
    }

    #[test]
    fn a_request_starts_the_quiet_over_and_its_time_is_not_quiet() {
        let mut idle = FsIdle::new();
        for _ in 0..79 {
            assert!(!idle.tick(25, false));
        }
        assert!(!idle.tick(25, true));
        // The delta after the busy tick is the request's own time.
        assert!(!idle.tick(5_000, false));
        let mut n = 0;
        while !idle.tick(25, false) {
            n += 1;
        }
        assert_eq!(n, 79);
    }
}
