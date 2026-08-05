//! The one [`lp_ws281x::Ws281xDriver`] instance on this chip, the RMT
//! interrupt that feeds it, and the optional telemetry tap.
//!
//! There is exactly one RMT peripheral, one RMT interrupt line, and therefore
//! one driver: the transmitting channels are *channels of it*, not several
//! drivers. Every per-channel decision (timing, frame in flight, statistics)
//! already lives inside [`lp_ws281x::ChannelState`], so nothing here needs a
//! second layer of per-channel state — which is why this module holds a
//! `static` and the endpoint-facing driver holds none.
//!
//! `Ws281xDriver::new` is `const` and every field of `ChannelState` is
//! an atomic, so this needs neither `static mut` nor a `StaticCell`: the
//! handler and thread context share a `&'static`. The memory-block plan the
//! backend reads is published at driver init through
//! [`super::v3_rmt::TX_PLAN`], before the interrupt is installed and before
//! any channel is configured.
//!
//! Note the driver is sized to [`TX_CHANNELS`] = 8, the chip's RMT slot count,
//! not to the outputs the plan offers. The absorbed slots simply never get
//! configured; giving them a `ChannelState` each costs a few hundred bytes of
//! `.bss` and keeps a channel number meaning the same thing everywhere.

use core::sync::atomic::{AtomicBool, Ordering};

use esp_hal::interrupt::{InterruptHandler, Priority};
use esp_hal::peripherals::{CPU_CTRL, Interrupt};
use esp_hal::rmt::Rmt;
use esp_hal::system::{Cpu, CpuControl, Stack};
use esp_hal::time::{Duration, Instant, Rate};
use lp_ws281x::Ws281xDriver;

use super::v3_rmt::{TX_CHANNELS, V3Rmt};

/// RMT source clock. The classic ESP32's RMT runs off APB, and esp-hal's
/// classic `validate_clock` accepts **only** the source frequency itself
/// (`frequency != source.freq()` is an error), so this must be exactly 80 MHz;
/// the per-channel divider of 1 then makes one tick 12.5 ns, which is what
/// [`lp_ws281x::PulseCodes::DEFAULT_CLOCK_HZ`] assumes.
pub const RMT_CLOCK: Rate = Rate::from_mhz(80);

/// A frame that has not completed within this long has hung; abort it and
/// report rather than spinning forever. The longest frame the output provider
/// can ask for (`WS281X_MAX_LEDS_PER_CHANNEL` = 1024 LEDs) is ~31 ms on the
/// wire, so a healthy frame always finishes inside this deadline — which
/// matters more now that the wait is deferred to the next frame's write: the
/// deadline still runs from `start`, and by wait time most of it has already
/// elapsed in wall-clock terms.
pub const FRAME_TIMEOUT: Duration = Duration::from_millis(50);

/// The driver, shared between thread context and the interrupt handler.
pub static DRIVER: Ws281xDriver<V3Rmt, TX_CHANNELS> = Ws281xDriver::new(V3Rmt::new());

/// Set once the RMT interrupt handler has been bound.
///
/// fw-esp32c6 re-registers its handler on every channel construction and says
/// in a comment that it should not. Here it does not: with several endpoints
/// opening independently, rebinding is not a rare accident but the normal
/// case, and a handler swapped while a frame is in flight loses that frame's
/// refills.
static ISR_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Set (from core 1, `Release`) once the RMT handler is bound into the APP
/// core's interrupt matrix. This is THE dual-core flag: the admission cap and
/// the provider's flush behaviour key off it, and `false` means the firmware
/// runs exactly the single-core (M4) semantics — barrier flush, cap 2, ISR on
/// the PRO core.
static ISR_ON_APP_CORE: AtomicBool = AtomicBool::new(false);

/// Is the RMT ISR serviced by the dedicated APP core this boot?
///
/// Constant after boot: it is set once, before the RMT driver exists, and
/// never cleared.
pub fn isr_on_app_core() -> bool {
    ISR_ON_APP_CORE.load(Ordering::Acquire)
}

/// How long [`start_app_core_isr`] waits for core 1 to report its bind. The
/// core has ~nothing to do before setting the flag; this expiring means the
/// core never really started, and the caller falls back to single-core.
const APP_CORE_BIND_TIMEOUT: Duration = Duration::from_millis(10);

/// Everything core 1 ever runs: bind the RMT handler, report, service
/// interrupts forever.
///
/// This executes ON the APP core — which is the entire point:
/// `interrupt::bind_handler` maps the peripheral source into the **calling**
/// core's interrupt matrix, and there is no remote-core form of the mapping
/// call in esp-hal. From here on the APP core services every RMT interrupt
/// while the PRO core renders; nothing on this core ever masks interrupts,
/// takes a critical section, allocates, or prints — the silence is the
/// product.
///
/// ⚠️ This function must NEVER return: esp-hal parks a returned core-1 entry
/// with a **hardware stall** (`internal_park_core`), and a stalled core
/// services no interrupts — the wires would go dark with no error anywhere.
/// The idle loop is `waiti 0`: wait for an interrupt at any level, take it
/// (the RMT handler runs), resume waiting.
fn app_core_main() {
    esp_hal::interrupt::bind_handler(
        Interrupt::RMT,
        InterruptHandler::new(rmt_isr, Priority::max()),
    );
    ISR_ON_APP_CORE.store(true, Ordering::Release);
    idle_forever();
}

/// The APP core's forever-idle loop, in IRAM.
///
/// IRAM placement matters here just as it does for the service path
/// (lp-ws281x's `isr-in-ram`): flash reads and writes on the PRO core open
/// cache-disabled windows, and a core fetching flash-resident code inside one
/// stalls (measured: a service ~110 words late during project-load reads) or
/// faults. The loop and the ISR path being IRAM-resident is what makes the
/// APP core independent of the PRO core's flash traffic; flash *writes* still
/// hardware-stall the core outright — see [`with_app_core_stalled`].
#[esp_hal::ram]
fn idle_forever() -> ! {
    loop {
        // SAFETY (asm): `waiti 0` only waits for an interrupt with the
        // threshold at level 0; no registers or memory are touched.
        unsafe { core::arch::asm!("waiti 0") };
    }
}

/// Run `f` with the APP core hardware-stalled. **Required around every flash
/// write/erase** (littlefs's storage adapter wraps its write paths in this).
///
/// Why: programming SPI flash disables the flash cache for the duration
/// (esp-storage's ROM-function window), and the classic ESP32 gives that
/// window no protection from the *other* core — a second core touching flash
/// mid-window reads garbage or faults, and the write itself can fail. That is
/// exactly how the first upload attempted under the dual-core deployment
/// crashed the board (2026-08-05: littlefs writes returned `I/O error` on a
/// healthy filesystem; the render-loaded variant crashed hard enough to wedge
/// the CH340K). The stall is the same mechanism esp-hal parks cores with — a
/// hardware clock-gate, resumed exactly where it stopped.
///
/// Register values mirror esp-hal 1.1.1 `soc/esp32/cpu_control.rs::
/// internal_park_core` (MIT/Apache-2.0): `SW_CPU_STALL.sw_stall_appcpu_c1 =
/// 0x21` + `OPTIONS0.sw_stall_appcpu_c0 = 0x02` stalls; zeros resume.
///
/// Consequences while stalled: RMT refills pend unserviced, so a frame in
/// flight during a flash write ends on its guard word — one torn frame per
/// write burst, the exact degradation the guard exists to provide. No
/// deadlock: the write path runs on the PRO core, which is the caller.
///
/// A no-op in the single-core fallback (nothing to stall; writes were always
/// safe there).
pub fn with_app_core_stalled<R>(f: impl FnOnce() -> R) -> R {
    use esp_hal::peripherals::LPWR;
    if !isr_on_app_core() {
        return f();
    }
    LPWR::regs()
        .sw_cpu_stall()
        .modify(|_, w| unsafe { w.sw_stall_appcpu_c1().bits(0x21) });
    LPWR::regs()
        .options0()
        .modify(|_, w| unsafe { w.sw_stall_appcpu_c0().bits(0x02) });
    let result = f();
    LPWR::regs()
        .sw_cpu_stall()
        .modify(|_, w| unsafe { w.sw_stall_appcpu_c1().bits(0) });
    LPWR::regs()
        .options0()
        .modify(|_, w| unsafe { w.sw_stall_appcpu_c0().bits(0) });
    result
}

/// Start the APP core with [`app_core_main`] and wait (briefly) for its bind.
///
/// Returns whether the dual-core deployment is live; on `false` the caller
/// keeps full single-core (M4) behaviour — [`install_isr`] then binds on the
/// PRO core as before, so a board whose second core will not start still
/// drives its wires. Call exactly once at boot, before the RMT driver is
/// constructed.
pub fn start_app_core_isr(cpu_ctrl: CPU_CTRL<'static>) -> bool {
    /// Core 1's stack: ISR frames plus the `waiti` loop, nothing else, so
    /// 4 KiB is generous. A `static` in .bss — this DRAM is the standing
    /// price of the dedicated core (the m6 compact-mappings relief is what
    /// pays for it at dome scale; see the plan).
    static mut APP_CORE_STACK: Stack<4096> = Stack::new();

    let mut cpu_control = CpuControl::new(cpu_ctrl);
    // SAFETY: the one and only reference ever taken to the stack static —
    // this function runs once at boot, and the core it hands the stack to
    // runs forever.
    let stack = unsafe { &mut *core::ptr::addr_of_mut!(APP_CORE_STACK) };
    match cpu_control.start_app_core(stack, app_core_main) {
        Ok(guard) => {
            // Dropping the guard would hardware-stall the core mid-ISR
            // forever after; the core is deliberately permanent.
            core::mem::forget(guard);
            let started = Instant::now();
            while !isr_on_app_core() && started.elapsed() < APP_CORE_BIND_TIMEOUT {}
            isr_on_app_core()
        }
        Err(error) => {
            esp_println::println!("[INIT] APP core start failed ({error:?})");
            false
        }
    }
}

/// The RMT interrupt entry point: a trampoline and nothing else.
///
/// Placed in IRAM with `#[ram]` — a flash-cache miss here is exactly the
/// latency the guard word exists to survive, so it should not be
/// self-inflicted. No logging, no allocation: the core documents
/// [`Ws281xDriver::on_interrupt`] as the whole of the handler's work. That
/// matters more on this chip than on any other in the family: the classic's
/// delivered interrupt rate saturates around 48 k/s (findings.md §12), and
/// that ceiling is what decides how many outputs run clean.
///
/// One entry services every channel. With 64-word halves the four transmitters
/// cross their half boundaries within microseconds of one another, so
/// coincident causes are the rule rather than the exception — and dispatching
/// them in one pass is `on_interrupt`'s job, not this trampoline's.
#[esp_hal::ram]
extern "C" fn rmt_isr() {
    DRIVER.on_interrupt();
}

/// Route the RMT interrupt to [`rmt_isr`], exactly once.
///
/// Two shapes, decided by [`isr_on_app_core`]:
///
/// * **Dual-core (the default deployment).** Core 1 already bound the handler
///   into its own matrix in [`app_core_main`]; the only thing left is
///   hygiene — make sure the PRO core's matrix does not also claim the
///   source. ⚠️ `rmt.set_interrupt_handler` must NOT be called in this shape,
///   here or anywhere else: its first act is disabling the RMT mapping on
///   every *other* core, which would silently unmap core 1 and kill every
///   refill. That foot-gun is the reason this function no longer uses it in
///   the dual-core path.
/// * **Single-core fallback** (core 1 failed to start): the pre-dual-core
///   bind on the current (PRO) core, unchanged M4 behaviour.
pub fn install_isr(rmt: &mut Rmt<'_, esp_hal::Blocking>) {
    if ISR_INSTALLED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    if isr_on_app_core() {
        esp_hal::interrupt::disable(Cpu::ProCpu, Interrupt::RMT);
        let _ = rmt;
    } else {
        rmt.set_interrupt_handler(InterruptHandler::new(rmt_isr, Priority::max()));
    }
}

/// Emit the per-channel WS281x counters, at most once per
/// [`telemetry::PERIOD`], from the frame-write path.
///
/// A no-op — and not even a timer read — unless the `ws281x_telemetry` feature
/// is on. See [`telemetry`] for what it prints and why the numbers are the
/// ones P3's capacity sweep needs.
#[cfg(not(feature = "ws281x_telemetry"))]
#[inline(always)]
pub fn report_telemetry_if_due() {}

#[cfg(feature = "ws281x_telemetry")]
pub use telemetry::report_telemetry_if_due;

/// Runtime telemetry over the serial link, off by default.
///
/// The counters `lp-ws281x` keeps (`guard_trips`, `guard_skips`, `errors`, the
/// refill-lag sum/count/max and the 9-bucket lag histogram) are the only
/// evidence there is about whether the classic's interrupt budget is actually
/// carrying four channels. The S3 firmware reads them **only** from its
/// `test_loopback` harness, so there was no app-path pattern to mirror; this
/// is that pattern, kept deliberately small:
///
/// * one `[WS281X]` line per configured channel, at most every
///   [`PERIOD`] — a period long enough that the print cost cannot itself
///   perturb the thing being measured;
/// * integer formatting only (mean lag is reported in tenths of a word), so
///   no float-formatting machinery is linked;
/// * emitted from the *frame-write* path, never from the ISR;
/// * compiled out entirely when the feature is off, so the shipping image
///   pays nothing — not even the `Instant::now()`.
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
    use crate::output::rmt::v3_rmt::TX_CHANNELS;

    /// How often the report line is emitted. Long enough not to perturb the
    /// measurement, short enough that a capacity sweep gets several samples
    /// per cell.
    pub const PERIOD: Duration = Duration::from_secs(10);

    /// Millisecond timestamp of the last report.
    ///
    /// **32-bit, not 64**: Xtensa LX6 has no 64-bit atomics, and this is
    /// shared between however many output handles are writing frames. Wrapping
    /// arithmetic below keeps it correct across the ~49.7-day rollover; the
    /// initial `0` simply means the first report lands one [`PERIOD`] into
    /// uptime.
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
            // `entry_max`/`entry_hist` are appended, never inserted: P4's
            // matrix scripts grep the earlier fields by position, and the
            // pre-existing prefix must keep matching old captures so runs
            // before and after this change stay comparable. Entry delay vs
            // refill lag split one deadline into "getting in" vs "getting
            // out" — see lp-ws281x's driver docs.
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

    /// `a:b:c:…` rendering of the lag histogram, so one field carries all nine
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
