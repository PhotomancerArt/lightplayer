//! The one [`lp_ws281x::Ws281xDriver`] instance on this chip, the RMT
//! interrupt that feeds it, and the optional telemetry tap.
//!
//! There is exactly one RMT peripheral, one RMT interrupt line, and therefore
//! one driver: the TX channels are *channels of it*, not two drivers. Every
//! per-channel decision (timing, frame in flight, statistics) already lives
//! inside [`lp_ws281x::ChannelState`], so nothing here needs a second layer of
//! per-channel state — which is why this module holds a `static` and the
//! endpoint-facing driver holds none.
//!
//! `Ws281xDriver::new` is `const` and every field of `ChannelState` is
//! an atomic, so this needs neither `static mut` nor a `StaticCell`: the
//! handler and thread context share a `&'static`. The memory-block plan the
//! backend reads is published at driver init through
//! [`super::c6_rmt::TX_PLAN`], before the interrupt is installed and before
//! any channel is configured.

use core::sync::atomic::{AtomicBool, Ordering};

use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::rmt::Rmt;
use esp_hal::time::{Duration, Rate};
use lp_ws281x::Ws281xDriver;

use super::c6_rmt::{C6Rmt, TX_CHANNELS};

/// RMT source clock. Divider 1 makes one tick 12.5 ns, which is what
/// [`lp_ws281x::PulseCodes::DEFAULT_CLOCK_HZ`] assumes — and the rate every
/// `Rmt::new` call site in this firmware already passes.
pub const RMT_CLOCK: Rate = Rate::from_mhz(80);

/// A frame that has not completed within this long has hung; abort it and
/// report rather than spinning forever. The longest frame the output provider
/// can ask for (256 LEDs) is ~7.7 ms on the wire.
pub const FRAME_TIMEOUT: Duration = Duration::from_millis(50);

/// The driver, shared between thread context and the interrupt handler.
pub static DRIVER: Ws281xDriver<C6Rmt, TX_CHANNELS> = Ws281xDriver::new(C6Rmt::new());

/// Set once the RMT interrupt handler has been bound.
///
/// The legacy C6 driver re-registers its handler on every channel construction
/// and says in a comment that it should not. Here it does not: with several
/// endpoints opening independently, rebinding is not a rare accident but the
/// normal case, and a handler swapped while a frame is in flight loses that
/// frame's refills.
static ISR_INSTALLED: AtomicBool = AtomicBool::new(false);

/// The RMT interrupt entry point: a trampoline and nothing else.
///
/// Placed in IRAM with `#[ram]` — a flash-cache miss here is exactly the
/// latency the guard word exists to survive, so it should not be
/// self-inflicted. No logging, no allocation: the core documents
/// [`Ws281xDriver::on_interrupt`] as the whole of the handler's work.
///
/// One entry services both channels. With a memory block each they cross their
/// half boundaries within microseconds of one another, so coincident causes are
/// the rule rather than the exception — and dispatching them in one pass is
/// `on_interrupt`'s job, not this trampoline's.
#[esp_hal::ram]
extern "C" fn rmt_isr() {
    DRIVER.on_interrupt();
}

/// Bind [`rmt_isr`] at the highest priority esp-hal can dispatch, exactly once.
///
/// `Priority::max()` is where every backend in this project already runs, and
/// on RISC-V it is the ceiling: there is no non-maskable tier above the
/// dispatched levels to escape to. Raising priority is therefore not a lever
/// this chip has left — see the plan's F1 correction.
pub fn install_isr(rmt: &mut Rmt<'_, esp_hal::Blocking>) {
    if ISR_INSTALLED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    rmt.set_interrupt_handler(InterruptHandler::new(rmt_isr, Priority::max()));
}

/// Emit the per-channel WS281x counters, at most once per
/// [`telemetry::PERIOD`], from the frame-write path.
///
/// A no-op — and not even a timer read — unless the `ws281x_telemetry` feature
/// is on. See [`telemetry`] for what it prints and why the numbers are the
/// ones the stress matrix needs.
#[cfg(not(feature = "ws281x_telemetry"))]
#[inline(always)]
pub fn report_telemetry_if_due() {}

#[cfg(feature = "ws281x_telemetry")]
pub use telemetry::report_telemetry_if_due;

/// Runtime telemetry over the serial link, off by default.
///
/// This is the classic's `fw-esp32v3` telemetry module, port for port: same
/// counters, same period, same field order, so a C6 capture and a classic
/// capture are read by the same eye (and the same grep). What the two chips
/// are being compared *on* is the C6's 29 % truncation under a WiFi scan
/// against the classic's 69 % — measured with these numbers, in both
/// block configurations.
///
/// * one `[WS281X]` line per configured channel, at most every [`PERIOD`] — a
///   period long enough that the print cost cannot itself perturb the thing
///   being measured;
/// * integer formatting only (mean lag is reported in tenths of a word), so no
///   float-formatting machinery is linked;
/// * emitted from the *frame-write* path, never from the ISR;
/// * compiled out entirely when the feature is off, so the shipping image pays
///   nothing — not even the `Instant::now()`.
///
/// The line carries `refills` (the refills that happened) next to `wanted`
/// (the refills an untruncated frame set would have needed). Those two are the
/// starvation signal: a refill that never arrives leaves no lag sample behind
/// at all, so `lag_max` can look comfortable while a third of the frames
/// truncate. `trips` is the direct truncation count. Read `refills` vs
/// `wanted` first, `trips` second, `lag_*` last.
#[cfg(feature = "ws281x_telemetry")]
pub mod telemetry {
    use core::sync::atomic::{AtomicU32, Ordering};

    use esp_hal::time::{Duration, Instant};
    use lp_ws281x::LAG_BUCKETS;

    use super::DRIVER;
    use crate::output::rmt::c6_rmt::TX_CHANNELS;

    /// How often the report line is emitted. Long enough not to perturb the
    /// measurement, short enough that a capacity sweep gets several samples
    /// per cell.
    pub const PERIOD: Duration = Duration::from_secs(10);

    /// Millisecond timestamp of the last report.
    ///
    /// 32-bit to match the classic's line for line (the LX6 has no 64-bit
    /// atomics); wrapping arithmetic below keeps it correct across the
    /// ~49.7-day rollover, and the initial `0` simply means the first report
    /// lands one [`PERIOD`] into uptime.
    static LAST_REPORT_MS: AtomicU32 = AtomicU32::new(0);

    /// Print one line per configured channel if [`PERIOD`] has elapsed.
    ///
    /// Cheap to call on every frame: one timer read and one relaxed load in
    /// the common case.
    pub fn report_telemetry_if_due() {
        let now_ms = Instant::now().duration_since_epoch().as_millis() as u32;
        let last = LAST_REPORT_MS.load(Ordering::Relaxed);
        if now_ms.wrapping_sub(last) < PERIOD.as_millis() as u32 {
            return;
        }
        // Relaxed compare-exchange: two channels racing here would at worst
        // print the block twice, and losing the race must not print at all.
        if LAST_REPORT_MS
            .compare_exchange(last, now_ms, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        for ch in 0..TX_CHANNELS as u8 {
            let Some(state) = DRIVER.channel(ch) else {
                continue;
            };
            let half = state.half_words();
            if half == 0 {
                // Never configured: an absorbed slot, or a channel this board
                // does not offer. Nothing to say about it.
                continue;
            }
            let stats = DRIVER.stats(ch);
            // Refills an untruncated frame needs: one per half boundary it
            // crosses. `total_bits` is the current frame's pixel bits.
            let wanted_per_frame = state.total_bits().div_ceil(half) as u64;
            let (lag_int, lag_tenths) =
                mean_lag_tenths(stats.refill_lag_sum, stats.refill_lag_count);
            // Field order is the classic's, verbatim: the matrix scripts grep
            // by position and the two chips' captures must line up column for
            // column. `entry_max`/`entry_hist` stay last — appended, never
            // inserted — so older captures keep matching the prefix.
            esp_println::println!(
                "[WS281X] t_ms={} ch={} half={} frames={} complete={} trips={} skips={} \
                 errors={} refills={} wanted={} lag_avg={}.{} lag_max={} over_half={} \
                 hist={} entry_max={} entry_hist={}",
                now_ms,
                ch,
                half,
                stats.frames,
                stats.complete_frames(),
                stats.guard_trips,
                stats.guard_skips,
                stats.errors,
                stats.refill_lag_count.max(0) as u64,
                stats.frames as u64 * wanted_per_frame,
                lag_int,
                lag_tenths,
                stats.refill_lag_max,
                stats.lag_over_half(),
                HistFmt(stats.lag_hist),
                stats.entry_delay_max,
                HistFmt(stats.entry_delay_hist),
            );
        }
    }

    /// Mean refill lag as `(whole_words, tenths)` — integer arithmetic so no
    /// float formatter is linked into the image.
    fn mean_lag_tenths(sum: i32, count: i32) -> (i32, i32) {
        if count <= 0 {
            return (0, 0);
        }
        let tenths = (sum as i64 * 10) / count as i64;
        ((tenths / 10) as i32, (tenths % 10).unsigned_abs() as i32)
    }

    /// `a:b:c:…` rendering of a histogram, so one field carries all nine
    /// buckets without an allocation.
    struct HistFmt([u32; LAG_BUCKETS]);

    impl core::fmt::Display for HistFmt {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            for (i, v) in self.0.iter().enumerate() {
                if i > 0 {
                    f.write_str(":")?;
                }
                write!(f, "{v}")?;
            }
            Ok(())
        }
    }
}
