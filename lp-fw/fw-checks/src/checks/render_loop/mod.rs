//! The `render-loop` payload: the shipped server firmware rendering a real
//! LightPlayer project, N frames, with RMT output on.
//!
//! # Why this payload exists
//!
//! Every other measurement on the emulator speed ladder turned out to be
//! something other than the workload the product runs.
//! `shader-compile-stress` compiles and never executes; `jit-math-perf` runs
//! JIT'd Q32 kernels with no pipeline around them; `boot-idle` is `wfi` with
//! no project loaded at all; `rmt-chase` drives the output path with a
//! hand-built pattern and no shader. The render loop — a project loaded from
//! the device filesystem, its shader compiled once and then executed per
//! frame as JIT'd native code, the fixture sampled, the display pipeline run,
//! the frame handed to RMT — was measured by nothing.
//!
//! This payload is that loop, bounded. It is deliberately **not** a harness:
//! the firmware boots the way it always boots, `boot::auto_load_project`
//! loads the project the way it always does, and `run_server_loop` runs the
//! way it always runs. The only differences are that the in-memory
//! filesystem arrives pre-seeded and the loop stops after a fixed number of
//! frames instead of never.
//!
//! # What is here and what is not
//!
//! Here: arithmetic over bytes — the frame-timing accumulator, the
//! cycles-to-microseconds conversion, the record shapes, the done marker.
//! `no_std`, `alloc`-free, host-tested.
//!
//! In `fw-esp32c6`: the chip. Board init, the PMU cycle counter behind CSR
//! 0x7E2, the `include_bytes!` project payload, the seeding call, and the
//! frame budget woven into `fw-esp32-common`'s server loop.
//!
//! # The one rule this payload is built around
//!
//! **Nothing is printed inside the loop.** Every prior benchmark on this
//! ladder was console-bound — `jit-math-perf`'s 2.4× speedup under the M4
//! poll skip was, measured by address, 100 % of the time spent waiting for
//! UART TX to drain its own logging. So [`FrameStats`] keeps min, max and a
//! running sum in registers, and one summary record is emitted after the last
//! frame. A per-frame record would make this payload measure the serial link
//! again, which is the mistake it exists to stop making.
//!
//! # Why the timings are a floor, not a silicon prediction
//!
//! On the emulator neither time grade models instruction-cache or flash-miss
//! cost, so the per-frame microseconds this payload reports under
//! `lp-emu:esp32c6:*` are the best case for the same code on the part. That
//! is fine for the ladder, which compares *host* seconds between emulator
//! binaries running identical guest work, and it is why a silicon run of this
//! same payload is worth taking separately rather than inferred from an
//! emulated one.

use crate::emit_record_json;

/// The sentinel. `--exit-on` matches it; `lp-emu-validate` replays to it.
pub const DONE_MARKER: &str = "[render-loop] === DONE ===";

/// Per-frame timing, accumulated without allocating and without printing.
///
/// Cycles come from a 32-bit free-running counter (the C6's Andes PMU
/// `mpccr`, CSR 0x7E2), so a per-frame delta is a `u32` computed with
/// `wrapping_sub` by the caller; that counter wraps every ~26.8 s at 160 MHz
/// and a frame is milliseconds, so the delta is always exact. The running
/// total is `u64` because the run is not.
#[derive(Debug, Clone, Copy)]
pub struct FrameStats {
    frames: u32,
    total_cycles: u64,
    min_cycles: u32,
    max_cycles: u32,
    first_cycles: u32,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameStats {
    pub const fn new() -> Self {
        Self {
            frames: 0,
            total_cycles: 0,
            min_cycles: u32::MAX,
            max_cycles: 0,
            first_cycles: 0,
        }
    }

    /// Fold one frame's cycle cost in. Branch-light on purpose: this runs
    /// inside the loop being measured.
    #[inline]
    pub fn record(&mut self, cycles: u32) {
        if self.frames == 0 {
            self.first_cycles = cycles;
        }
        self.frames = self.frames.saturating_add(1);
        self.total_cycles = self.total_cycles.saturating_add(cycles as u64);
        if cycles < self.min_cycles {
            self.min_cycles = cycles;
        }
        if cycles > self.max_cycles {
            self.max_cycles = cycles;
        }
    }

    pub const fn frames(&self) -> u32 {
        self.frames
    }

    pub const fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// `0` before the first frame, so a summary from an empty run reads as
    /// zero rather than as `u32::MAX`.
    pub const fn min_cycles(&self) -> u32 {
        if self.frames == 0 { 0 } else { self.min_cycles }
    }

    pub const fn max_cycles(&self) -> u32 {
        self.max_cycles
    }

    /// The first frame, separately — because it is not like the others.
    ///
    /// The engine compiles a shader **lazily**, on the first frame that
    /// samples it, not during `load_project`. So frame 1 carries the whole
    /// compile (52 ms for `projects/test/basic`, against a ~15 ms steady
    /// frame) and is reliably the run's `max`. Reporting it on its own is what
    /// keeps `max` meaning "the worst steady frame" to a reader who knows to
    /// look, instead of meaning "the compile" every single time.
    pub const fn first_cycles(&self) -> u32 {
        self.first_cycles
    }

    pub const fn mean_cycles(&self) -> u64 {
        if self.frames == 0 {
            0
        } else {
            self.total_cycles / self.frames as u64
        }
    }
}

/// Integer cycles to microseconds at a given CPU clock.
///
/// Multiplies first, in `u64`: at 160 MHz a 5-second run is 8e8 cycles and
/// 8e14 after the multiply, four orders of magnitude inside `u64`.
pub const fn cycles_to_us(cycles: u64, cpu_hz: u64) -> u64 {
    if cpu_hz == 0 {
        0
    } else {
        (cycles * 1_000_000) / cpu_hz
    }
}

/// Frames per second implied by the mean frame cost, scaled by 100 so the
/// record carries two decimals without a float.
pub const fn fps_centi(mean_cycles: u64, cpu_hz: u64) -> u64 {
    if mean_cycles == 0 {
        0
    } else {
        (cpu_hz * 100) / mean_cycles
    }
}

/// What the project cost to load — the compile is the bulk of it, and it is
/// the one part of the run that does not repeat.
#[allow(clippy::too_many_arguments)]
pub fn emit_load_record(
    project: &str,
    lamps: u32,
    outputs: u32,
    load_us: u64,
    heap_free: u32,
    heap_used: u32,
) {
    emit_record_json(format_args!(
        r#"{{"kind":"render-loop-load","project":"{project}","lamps":{lamps},"outputs":{outputs},"load_us":{load_us},"heap_free":{heap_free},"heap_used":{heap_used}}}"#
    ));
}

/// The run's whole result, in one line, after the last frame.
#[allow(clippy::too_many_arguments)]
pub fn emit_summary_record(
    stats: &FrameStats,
    cpu_hz: u64,
    delta_ms: u32,
    uptime_us: u64,
    heap_free: u32,
    heap_used: u32,
    largest_free_block: u32,
) {
    let mean = stats.mean_cycles();
    emit_record_json(format_args!(
        r#"{{"kind":"render-loop-summary","frames":{frames},"delta_ms":{delta_ms},"uptime_us":{uptime_us},"render_us_total":{total},"render_us_first":{first},"render_us_min":{min},"render_us_max":{max},"render_us_mean":{mean_us},"fps_centi":{fps},"heap_free":{heap_free},"heap_used":{heap_used},"largest_free_block":{largest_free_block}}}"#,
        frames = stats.frames(),
        first = cycles_to_us(stats.first_cycles() as u64, cpu_hz),
        total = cycles_to_us(stats.total_cycles(), cpu_hz),
        min = cycles_to_us(stats.min_cycles() as u64, cpu_hz),
        max = cycles_to_us(stats.max_cycles() as u64, cpu_hz),
        mean_us = cycles_to_us(mean, cpu_hz),
        fps = fps_centi(mean, cpu_hz),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_run_reports_zeroes_rather_than_sentinels() {
        let stats = FrameStats::new();
        assert_eq!(stats.frames(), 0);
        assert_eq!(stats.min_cycles(), 0, "u32::MAX must never reach a record");
        assert_eq!(stats.max_cycles(), 0);
        assert_eq!(stats.mean_cycles(), 0);
    }

    #[test]
    fn the_first_frame_is_kept_apart_because_it_carries_the_compile() {
        let mut stats = FrameStats::new();
        stats.record(10_000_000);
        stats.record(2_400_000);
        stats.record(2_500_000);
        assert_eq!(stats.first_cycles(), 10_000_000);
        assert_eq!(
            stats.max_cycles(),
            10_000_000,
            "max still reports the truth; `first` is what lets a reader discount it"
        );
    }

    #[test]
    fn min_max_and_mean_track_the_frames_folded_in() {
        let mut stats = FrameStats::new();
        for cycles in [2_600_000u32, 2_400_000, 2_500_000] {
            stats.record(cycles);
        }
        assert_eq!(stats.frames(), 3);
        assert_eq!(stats.min_cycles(), 2_400_000);
        assert_eq!(stats.max_cycles(), 2_600_000);
        assert_eq!(stats.mean_cycles(), 2_500_000);
        assert_eq!(stats.total_cycles(), 7_500_000);
    }

    #[test]
    fn cycles_convert_at_the_c6s_clock() {
        // 160 MHz: a 16.4 ms frame is 2,624,000 cycles.
        assert_eq!(cycles_to_us(2_624_000, 160_000_000), 16_400);
        assert_eq!(cycles_to_us(160_000_000, 160_000_000), 1_000_000);
    }

    #[test]
    fn a_zero_clock_cannot_divide_by_zero() {
        assert_eq!(cycles_to_us(1_000, 0), 0);
        assert_eq!(fps_centi(1_000, 0), 0);
    }

    #[test]
    fn fps_carries_two_decimals_without_a_float() {
        // 2,624,000 cycles a frame at 160 MHz = 60.97 fps.
        assert_eq!(fps_centi(2_624_000, 160_000_000), 6_097);
    }

    #[test]
    fn the_accumulator_saturates_rather_than_wrapping() {
        let mut stats = FrameStats::new();
        stats.frames = u32::MAX;
        stats.record(1);
        assert_eq!(stats.frames(), u32::MAX);
    }
}
