//! Thread side of the level-4 refill: configure, start, poll, abort,
//! telemetry — the `shared_driver`-shaped surface the endpoint driver swaps
//! in under `hli_refill`.
//!
//! The division of labor: everything per-refill lives in the level-4 vector
//! (`super::vector`); everything else — pulse-code compilation, wire-order
//! permutation, prefilling both halves, starting and stopping the
//! transmitter — runs here in ordinary Rust at thread level, exactly where
//! the level-3 driver runs it. The prefill goes through the host-tested
//! reference model in `lp-ws281x-hli`, so the model (the assembly's spec) is
//! exercised on silicon every frame alongside the assembly itself.
//!
//! # Concurrency contract
//!
//! Single core; the handler never nests with itself. The thread side mutates
//! a channel only while `active == 0` (the vector ignores inactive channels
//! and still acknowledges their causes), and `start_frame_state` publishes
//! `active = 1` only after the channel state and both prefilled halves are in
//! place. The wire-order staging buffers are touched only between frames from
//! the single server/harness task — see [`HliAppDriver`]'s `Sync` note.

extern crate alloc;

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::AtomicU32;
use core::sync::atomic::Ordering::Relaxed;

use esp_hal::peripherals::RMT;
use esp_hal::rmt::Rmt;
use lp_ws281x::{ChannelTiming, ColorOrder, PulseCodes, StartError, TimingError};
use lp_ws281x_hli::{
    configure_channel, start_frame_state, HliChannel, HliConfigError, HliPort, HLI_CHANNELS,
};

use crate::output::rmt::v3_rmt::{
    int_err_bit, int_thr_bit, int_tx_end_bit, BLOCK_WORDS, RAM_BASE, SLOT_STRIDE, TX_BLOCKS,
    TX_CHANNELS, V3Rmt,
};
use crate::output::rmt::hli::vector::{route_rmt_to_level4, HLI_BANK};

// Same values as the level-3 path — the comparison must not vary them.
pub use crate::output::rmt::shared_driver::FRAME_TIMEOUT;

/// The register backend used for the thread-side start/stop operations
/// (`start_tx`, `stop_tx`) — the same proven `RmtHw` impl the level-3 driver
/// uses, so the start path differs between the two modes in *nothing*.
const V3: V3Rmt = V3Rmt::new(TX_BLOCKS);

/// One installed flag, mirroring `shared_driver::ISR_INSTALLED`.
static INSTALLED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Route the RMT interrupt to the level-4 vector, exactly once.
///
/// Takes the `Rmt` driver mutably like `shared_driver::install_isr` so the
/// call sites are interchangeable — but deliberately does **not** call
/// `set_interrupt_handler`: under this feature the peripheral's interrupt
/// never goes through esp-hal's level-3 dispatch at all.
// The harness uses `install_isr_raw`; this wrapper is the endpoint seam.
#[cfg_attr(
    all(feature = "hli_stress", not(feature = "server")),
    allow(dead_code, reason = "the harness calls install_isr_raw directly")
)]
pub fn install_isr(_rmt: &mut Rmt<'_, esp_hal::Blocking>) {
    install_isr_raw();
}

/// [`install_isr`] without the `Rmt` witness — for the stress harness, whose
/// `Rmt` value has had its channel creators moved out by the time it switches
/// paths mid-boot. The peripheral is necessarily initialized by then (the
/// level-3 phase ran on it).
pub fn install_isr_raw() {
    if INSTALLED
        .compare_exchange(false, true, Relaxed, Relaxed)
        .is_err()
    {
        return;
    }
    let regs = RMT::regs();
    HLI_BANK
        .int_st_addr
        .store(regs.int_st().as_ptr() as usize, Relaxed);
    HLI_BANK
        .int_clr_addr
        .store(regs.int_clr().as_ptr() as usize, Relaxed);
    core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
    route_rmt_to_level4();
}

/// Why a channel could not be configured for the level-4 path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HliConfigureError {
    /// Not an owning slot under the block plan (`0/2/4/6` on this build).
    BadSlot,
    /// The wire timing cannot be expressed at this clock.
    Timing(TimingError),
    /// The window geometry violates the level-4 contract.
    Contract(HliConfigError),
}

/// The raw-MMIO [`HliPort`]: what the reference model drives on real
/// hardware. The vector performs the identical accesses in assembly.
struct DevicePort {
    status: *const u32,
    tx_lim: *mut u32,
    ram: *mut u32,
    window_words: u32,
}

impl HliPort for DevicePort {
    #[inline]
    fn read_pos_abs(&mut self) -> u32 {
        // SAFETY: `status` is the channel's CHnSTATUS register, valid MMIO.
        let raw = unsafe { self.status.read_volatile() };
        // mem_raddr_ex: bits 12..=21 (esp32 PAC) — the same extraction the
        // vector performs with `extui`.
        (raw >> 12) & 0x3FF
    }

    #[inline]
    fn write_tx_lim(&mut self, words: u32) {
        // SAFETY: the channel's CH_TX_LIM register; the 9-bit field is the
        // register's only field, and `words` is at most the half size (≤ 64).
        unsafe { self.tx_lim.write_volatile(words & 0x1FF) };
    }

    #[inline]
    fn write_ram(&mut self, word: u32, value: u32) {
        if word >= self.window_words {
            return;
        }
        // SAFETY: bounded to the channel's own window inside the RMT RAM;
        // volatile because the transmitter reads this memory concurrently.
        unsafe { self.ram.add(word as usize).write_volatile(value) };
    }
}

/// The level-4 app driver: `shared_driver::DRIVER`'s drop-in for the
/// endpoint layer.
pub struct HliAppDriver {
    /// Wire-order staging per entry, reused across frames. Touched only from
    /// the single thread-side task, only while the entry is inactive.
    staging: [UnsafeCell<Vec<u8>>; HLI_CHANNELS],
    /// Color order per entry (as `ColorOrder::as_u8`), applied during
    /// staging so the vector walks plain bytes.
    order: [AtomicU32; HLI_CHANNELS],
}

// SAFETY: `staging` is only accessed from the single thread-side task (the
// server loop / harness main), never from the interrupt — the vector sees
// only the raw pointer published in `HLI_BANK`, and only while `active != 0`,
// during which the thread side does not touch the `Vec`. `order` is atomic.
unsafe impl Sync for HliAppDriver {}

/// The one app-side driver instance.
pub static DRIVER: HliAppDriver = HliAppDriver {
    staging: [const { UnsafeCell::new(Vec::new()) }; HLI_CHANNELS],
    order: [const { AtomicU32::new(ColorOrder::Grb as u32) }; HLI_CHANNELS],
};

impl HliAppDriver {
    /// The bank entry for RMT slot `slot`, if the slot is one this build's
    /// block plan lets transmit.
    fn entry(&self, slot: u8) -> Option<(usize, &'static HliChannel)> {
        let slot = slot as usize;
        if slot % SLOT_STRIDE != 0 || slot >= TX_CHANNELS {
            return None;
        }
        let index = slot / SLOT_STRIDE;
        HLI_BANK.channels.get(index).map(|ch| (index, ch))
    }

    /// Compile `timing` for the 80 MHz RMT clock and bind slot `slot` to the
    /// level-4 bank. Mirrors `Ws281xDriver::configure_default_clock`.
    pub fn configure_default_clock(
        &self,
        slot: u8,
        timing: &ChannelTiming,
    ) -> Result<(), HliConfigureError> {
        let (index, ch) = self.entry(slot).ok_or(HliConfigureError::BadSlot)?;
        let codes = PulseCodes::new(timing, PulseCodes::DEFAULT_CLOCK_HZ)
            .map_err(HliConfigureError::Timing)?;

        let regs = RMT::regs();
        let window_start = TX_BLOCKS.window_start(slot, BLOCK_WORDS);
        let window_words = TX_BLOCKS.window_words(slot, BLOCK_WORDS);
        // SAFETY: in-range word offset into the RMT RAM MMIO window.
        let ram_base = unsafe { (RAM_BASE as *mut u32).add(window_start) };

        configure_channel(
            ch,
            (int_thr_bit(slot), int_tx_end_bit(slot), int_err_bit(slot)),
            regs.chstatus(slot as usize).as_ptr() as usize,
            regs.ch_tx_lim(slot as usize).as_ptr() as usize,
            ram_base as usize,
            window_start as u32,
            window_words as u32,
            (codes.zero, codes.one, codes.latch),
        )
        .map_err(HliConfigureError::Contract)?;

        self.order[index].store(timing.color_order as u32, Relaxed);
        // The vector services causes under `all_mask`; accumulate this slot's.
        HLI_BANK.all_mask.fetch_or(
            int_thr_bit(slot) | int_tx_end_bit(slot) | int_err_bit(slot),
            Relaxed,
        );
        Ok(())
    }

    /// Transmit `frame` (RGB triplets) on `slot` and wait for completion,
    /// calling `spin` between polls — `Ws281xDriver::send_blocking`'s shape,
    /// including its use as the endpoint layer's hang detector (`spin` may
    /// call [`Self::abort`], which completes the channel).
    pub fn send_blocking(
        &self,
        slot: u8,
        frame: &[u8],
        mut spin: impl FnMut(),
    ) -> Result<(), StartError> {
        let (index, ch) = self.entry(slot).ok_or(StartError::ChannelOutOfRange)?;
        if ch.half_words.load(Relaxed) == 0 {
            return Err(StartError::NotConfigured);
        }
        if ch.active.load(Relaxed) != 0 {
            return Err(StartError::Busy);
        }

        // Stage the frame in wire order. SAFETY (`&mut` through the
        // `UnsafeCell`): single thread-side task, channel inactive — the
        // vector is not reading this buffer.
        let staging = unsafe { &mut *self.staging[index].get() };
        let order = ColorOrder::from_u8(self.order[index].load(Relaxed) as u8)
            .unwrap_or(ColorOrder::Grb);
        let pixels = frame.len() / 3;
        staging.clear();
        staging.reserve(pixels * 3);
        for pixel in 0..pixels {
            for slot_idx in 0..3 {
                staging.push(frame[pixel * 3 + order.source_index(slot_idx)]);
            }
        }

        let mut port = self.port(ch);
        // SAFETY: `staging` stays alive, in place and unmodified until this
        // function observes completion (or aborts) — it is owned by the
        // driver and only rewritten by the next `send_blocking` on this
        // entry, which cannot begin until this one returns.
        unsafe {
            start_frame_state(
                ch,
                &mut port,
                staging.as_ptr() as usize,
                staging.len() as u32,
            );
        }
        // Same start path as the level-3 driver: clear stale causes, reset
        // the divider, hand the RAM to the transmitter, go.
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        use lp_ws281x::RmtHw;
        V3.start_tx(slot);

        while ch.complete.load(Relaxed) == 0 {
            spin();
        }
        Ok(())
    }

    /// Stop `slot` now and mark its frame complete. Mirrors
    /// `Ws281xDriver::abort`; safe at any time.
    pub fn abort(&self, slot: u8) {
        let Some((_, ch)) = self.entry(slot) else {
            return;
        };
        // Deactivate first: from here the vector ignores the channel (still
        // acknowledging its causes), so the STOP-fill below cannot race a
        // concurrent refill.
        ch.active.store(0, Relaxed);
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        use lp_ws281x::RmtHw;
        V3.stop_tx(slot);
        ch.complete.store(1, Relaxed);
    }

    /// The raw-MMIO port for a configured entry.
    fn port(&self, ch: &HliChannel) -> DevicePort {
        DevicePort {
            status: ch.status_addr.load(Relaxed) as *const u32,
            tx_lim: ch.tx_lim_addr.load(Relaxed) as *mut u32,
            ram: ch.ram_base.load(Relaxed) as *mut u32,
            window_words: ch.ram_mask.load(Relaxed) + 1,
        }
    }
}

/// Emit the per-channel `[WS281X]` counters from the level-4 bank, at most
/// once per period — the same line format as `shared_driver::telemetry` (the
/// P4 capture scripts parse fields by position) with one appended field,
/// `src=hli4`, so a capture says which refill path produced it.
#[cfg(not(feature = "ws281x_telemetry"))]
#[inline(always)]
pub fn report_telemetry_if_due() {}

#[cfg(feature = "ws281x_telemetry")]
pub use telemetry::report_telemetry_if_due;

#[cfg(feature = "ws281x_telemetry")]
mod telemetry {
    use core::sync::atomic::AtomicU32;
    use core::sync::atomic::Ordering::Relaxed;

    use esp_hal::time::Instant;
    use lp_ws281x_hli::LAG_BUCKETS;

    use crate::output::rmt::hli::vector::HLI_BANK;
    use crate::output::rmt::v3_rmt::SLOT_STRIDE;

    /// Same period as the level-3 telemetry — the comparison must not vary it.
    pub const PERIOD_MS: u32 = 10_000;

    static LAST_REPORT_MS: AtomicU32 = AtomicU32::new(0);

    /// Print one line per configured entry if the period has elapsed.
    pub fn report_telemetry_if_due() {
        let now_ms = Instant::now().duration_since_epoch().as_millis() as u32;
        let last = LAST_REPORT_MS.load(Relaxed);
        if now_ms.wrapping_sub(last) < PERIOD_MS {
            return;
        }
        if LAST_REPORT_MS
            .compare_exchange(last, now_ms, Relaxed, Relaxed)
            .is_err()
        {
            return;
        }

        for (index, ch) in HLI_BANK.channels.iter().enumerate() {
            let half = ch.half_words.load(Relaxed);
            if half == 0 {
                continue;
            }
            let slot = (index * SLOT_STRIDE) as u8;
            let frames = ch.frames.load(Relaxed) as u64;
            let trips = ch.trips.load(Relaxed) as u64;
            let wanted_per_frame = u64::from(ch.total_bits.load(Relaxed).div_ceil(half));
            let lag_sum = ch.lag_sum.load(Relaxed) as u64;
            let lag_count = ch.lag_count.load(Relaxed) as u64;
            let lag_tenths = if lag_count == 0 {
                0
            } else {
                lag_sum * 10 / lag_count
            };
            esp_println::println!(
                "[WS281X] t_ms={} ch={} half={} frames={} complete={} trips={} skips={} \
                 errors={} refills={} wanted={} lag_avg={}.{} lag_max={} over_half={} \
                 hist={} entry_max={} entry_hist={} src=hli4",
                now_ms,
                slot,
                half,
                frames,
                frames - trips,
                trips,
                ch.skips.load(Relaxed),
                ch.errors.load(Relaxed),
                lag_count,
                frames * wanted_per_frame,
                lag_tenths / 10,
                lag_tenths % 10,
                ch.lag_max.load(Relaxed),
                ch.lag_hist[LAG_BUCKETS - 1].load(Relaxed),
                Hist(&ch.lag_hist),
                ch.entry_max.load(Relaxed),
                Hist(&ch.entry_hist),
            );
        }
    }

    /// `a:b:…` rendering of a nine-bucket histogram.
    struct Hist<'a>(&'a [AtomicU32; LAG_BUCKETS]);

    impl core::fmt::Display for Hist<'_> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            for (i, v) in self.0.iter().enumerate() {
                if i > 0 {
                    f.write_str(":")?;
                }
                write!(f, "{}", v.load(Relaxed))?;
            }
            Ok(())
        }
    }
}
