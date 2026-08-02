//! Host suite: pins the reference model's behavior, with `lp-ws281x`'s
//! driver-plus-mock as the oracle where a comparison is meaningful.
//!
//! The simulated transmitter here ([`MiniTx`]) models the **classic** chip's
//! threshold semantics — `CH_TX_LIM` as a periodic count of words sent —
//! where `lp_ws281x::MockRmt` models the S3's position semantics. Under
//! prompt service the two produce identical event streams, which is what
//! makes the differential test valid.

extern crate std;

use std::vec::Vec;

use core::sync::atomic::Ordering::Relaxed;

use lp_ws281x::{lag_bucket, ChannelTiming, ColorOrder, MockRmt, PulseCodes, Pump, Ws281xDriver};

use crate::{
    configure_channel, service_flags, service_threshold, start_frame_state, HliChannel, HliPort,
};

/// A one-channel simulated transmitter with classic `tx_lim` count semantics
/// and STOP-on-zero end.
struct MiniTx {
    ram: Vec<u32>,
    /// Window-relative read pointer.
    pos: usize,
    /// Pretend absolute offset of the window in RMT RAM, to exercise the
    /// model's `window_start` subtraction.
    window_start: u32,
    tx_lim: u32,
    since_arm: u32,
    running: bool,
    emitted: Vec<u32>,
    pending_thr: bool,
    pending_end: bool,
}

impl MiniTx {
    fn new(ram_words: usize, window_start: u32) -> Self {
        Self {
            ram: std::vec![0; ram_words],
            pos: 0,
            window_start,
            tx_lim: 0,
            since_arm: 0,
            running: false,
            emitted: Vec::new(),
            pending_thr: false,
            pending_end: false,
        }
    }

    fn start(&mut self) {
        self.pos = 0;
        self.since_arm = 0;
        self.running = true;
    }

    /// Advance the wire clock by `words`, mirroring `MockRmt::advance`:
    /// a STOP word ends the transmission without being emitted; a threshold
    /// event fires every `tx_lim` words of transmission — periodic from
    /// `start`, *not* restarted by an equal-value `CH_TX_LIM` write. The
    /// shipped L3 driver rewrites the same value on every refill and runs
    /// trip-free at double-digit service latencies on silicon, which rules
    /// out restart-on-write drift; this mock encodes that observed behavior.
    fn advance(&mut self, words: usize) {
        for _ in 0..words {
            if !self.running {
                return;
            }
            let value = self.ram[self.pos];
            if value == 0 {
                self.running = false;
                self.pending_end = true;
                return;
            }
            self.emitted.push(value);
            self.pos = (self.pos + 1) % self.ram.len();
            self.since_arm += 1;
            if self.tx_lim > 0 && self.since_arm >= self.tx_lim {
                self.pending_thr = true;
                self.since_arm = 0;
            }
        }
    }

    fn has_pending(&self) -> bool {
        self.pending_thr || self.pending_end
    }
}

impl HliPort for MiniTx {
    fn read_pos_abs(&mut self) -> u32 {
        self.window_start + self.pos as u32
    }

    fn write_tx_lim(&mut self, words: u32) {
        self.tx_lim = words;
    }

    fn write_ram(&mut self, word: u32, value: u32) {
        let idx = word as usize;
        assert!(idx < self.ram.len(), "model wrote outside the window");
        self.ram[idx] = value;
    }
}

/// GRB permutation of an RGB frame — what the firmware's thread side does
/// once per frame so the handler can walk bytes in wire order.
fn to_wire_order(rgb: &[u8], order: ColorOrder) -> Vec<u8> {
    let mut out = std::vec![0u8; (rgb.len() / 3) * 3];
    for pixel in 0..rgb.len() / 3 {
        for slot in 0..3 {
            out[pixel * 3 + slot] = rgb[pixel * 3 + order.source_index(slot)];
        }
    }
    out
}

/// A configured 16-word (8-word half) channel over a [`MiniTx`], plus the
/// wire-order frame storage the raw `frame_ptr` points into.
struct Rig {
    ch: HliChannel,
    tx: MiniTx,
    wire: Vec<u8>,
}

impl Rig {
    fn new(ram_words: usize, window_start: u32) -> Self {
        let ch = HliChannel::new();
        let codes = PulseCodes::new(&ChannelTiming::WS2812, PulseCodes::DEFAULT_CLOCK_HZ).unwrap();
        configure_channel(
            &ch,
            (1 << 24, 1 << 0, 1 << 2),
            0, // status_addr / tx_lim_addr unused by MiniTx
            0,
            0,
            window_start,
            ram_words as u32,
            (codes.zero, codes.one, codes.latch),
        )
        .unwrap();
        Self {
            ch,
            tx: MiniTx::new(ram_words, window_start),
            wire: Vec::new(),
        }
    }

    fn start(&mut self, rgb: &[u8]) {
        self.wire = to_wire_order(rgb, ColorOrder::Grb);
        // SAFETY: `self.wire` lives as long as the rig and is not mutated
        // until the next `start`; the frame completes within each test before
        // that.
        unsafe {
            start_frame_state(
                &self.ch,
                &mut self.tx,
                self.wire.as_ptr() as usize,
                self.wire.len() as u32,
            );
        }
        self.tx.start();
    }

    /// Advance one word at a time with one word of service latency —
    /// `Pump::default()`'s schedule — until the transmitter stops.
    fn run_to_end(&mut self) {
        let mut guard = 0;
        while self.tx.running || self.tx.has_pending() {
            self.tx.advance(1);
            if self.tx.has_pending() {
                self.tx.advance(1);
                let thr = core::mem::take(&mut self.tx.pending_thr);
                let end = core::mem::take(&mut self.tx.pending_end);
                // SAFETY: frame storage held by the rig.
                unsafe { service_flags(&self.ch, &mut self.tx, thr, end, false) };
            }
            guard += 1;
            assert!(guard < 1 << 16, "simulated frame never completed");
        }
    }
}

/// The shift-based histogram bucket is exactly `lp_ws281x::lag_bucket` for
/// every power-of-two half the classic can configure.
#[test]
fn bucket_matches_lp_ws281x() {
    for half in [8u32, 16, 32, 64, 128] {
        let ch = HliChannel::new();
        configure_channel(&ch, (0, 0, 0), 0, 0, 0, 0, half * 2, (1, 2, 3)).unwrap();
        for delay in 0..half * 2 {
            let ours = if delay >= half {
                crate::LAG_BUCKETS - 1
            } else {
                (delay >> ch.bucket_shift.load(Relaxed)) as usize
            };
            assert_eq!(
                ours,
                lag_bucket(delay as usize, half as usize),
                "half={half} delay={delay}"
            );
        }
    }
}

/// Geometry the contract refuses.
#[test]
fn configure_rejects_bad_geometry() {
    use crate::HliConfigError;
    let ch = HliChannel::new();
    assert_eq!(
        configure_channel(&ch, (0, 0, 0), 0, 0, 0, 0, 48, (1, 2, 3)),
        Err(HliConfigError::RamNotPowerOfTwo)
    );
    assert_eq!(
        configure_channel(&ch, (0, 0, 0), 0, 0, 0, 0, 8, (1, 2, 3)),
        Err(HliConfigError::RamTooSmall)
    );
    assert!(configure_channel(&ch, (0, 0, 0), 0, 0, 0, 0, 16, (1, 2, 3)).is_ok());
}

/// The model transmits byte-identical wire streams to `lp-ws281x`'s driver
/// (the shipping L3 implementation) across frame sizes that exercise every
/// tail shape: latch mid-half, latch exactly on a half boundary, single-half
/// frames, and multi-wrap frames.
#[test]
fn differential_wire_stream_vs_lp_ws281x() {
    // 16 words = 8-word halves. 3 bytes/pixel · 8 bits = 24 words per pixel:
    // pixel counts 1..6 sweep the latch across every position in a half.
    for pixels in 1..=6usize {
        let rgb: Vec<u8> = (0..pixels * 3).map(|i| (i * 37 + 11) as u8).collect();

        // Oracle: the shipping driver core over the position-semantics mock.
        let driver: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, 16));
        driver
            .configure_default_clock(0, &ChannelTiming::WS2812)
            .unwrap();
        driver
            .send_blocking(0, &rgb, || {
                let mock = driver.hw();
                mock.advance_all(1);
                if mock.has_pending() {
                    mock.advance_all(1);
                    driver.on_interrupt();
                }
            })
            .unwrap();
        let oracle = driver.hw().emitted(0);
        let stats = driver.stats(0);
        assert_eq!(stats.guard_trips, 0, "oracle run truncated at {pixels}px");

        // Model, same schedule.
        let mut rig = Rig::new(16, 96);
        rig.start(&rgb);
        rig.run_to_end();

        assert_eq!(
            rig.tx.emitted, oracle,
            "wire stream diverged at {pixels} pixels"
        );
        assert_eq!(rig.ch.frames.load(Relaxed), 1);
        assert_eq!(rig.ch.trips.load(Relaxed), 0, "model truncated {pixels}px");
        assert_eq!(rig.ch.complete.load(Relaxed), 1);
        assert_eq!(rig.ch.active.load(Relaxed), 0);
    }
}

/// A lost refill interrupt truncates the frame via the guard word instead of
/// replaying stale data forever, and the truncation is counted.
#[test]
fn lost_refill_trips_guard() {
    let mut rig = Rig::new(16, 0);
    // 4 pixels = 96 data words: needs many refills on 8-word halves.
    let rgb: Vec<u8> = (0..12).map(|i| 0x10 + i as u8).collect();
    rig.start(&rgb);

    let mut thresholds = 0;
    let mut guard = 0;
    while rig.tx.running || rig.tx.has_pending() {
        rig.tx.advance(1);
        if rig.tx.has_pending() {
            rig.tx.advance(1);
            let thr = core::mem::take(&mut rig.tx.pending_thr);
            let end = core::mem::take(&mut rig.tx.pending_end);
            let mut deliver_thr = thr;
            if thr {
                thresholds += 1;
                if thresholds == 3 {
                    // Swallow the third refill, as masking would.
                    deliver_thr = false;
                }
            }
            // SAFETY: frame storage held by the rig.
            unsafe { service_flags(&rig.ch, &mut rig.tx, deliver_thr, end, false) };
        }
        guard += 1;
        assert!(guard < 1 << 16, "truncation run never ended");
    }

    assert_eq!(rig.ch.frames.load(Relaxed), 1);
    assert_eq!(rig.ch.trips.load(Relaxed), 1, "guard trip not counted");
    assert!(
        rig.ch.bit_cursor.load(Relaxed) < rig.ch.total_bits.load(Relaxed),
        "cursor should stop short on a truncated frame"
    );
}

/// Entry delay is measured against the armed boundary: a service delayed by
/// `d` words reads back `d` (plus the schedule's built-in 1-word latency).
#[test]
fn entry_delay_measured_against_boundary() {
    let mut rig = Rig::new(16, 32);
    let rgb: Vec<u8> = (0..12).map(|i| 0x80 | i as u8).collect();
    rig.start(&rgb);

    // First event fires after 8 words; let 3 more pass before servicing.
    rig.tx.advance(8);
    assert!(rig.tx.pending_thr);
    rig.tx.advance(3);
    rig.tx.pending_thr = false;
    // SAFETY: frame storage held by the rig.
    unsafe { service_threshold(&rig.ch, &mut rig.tx) };

    assert_eq!(rig.ch.entry_max.load(Relaxed), 3);
    assert_eq!(rig.ch.lag_count.load(Relaxed), 1);
    // Bucket 3·8/8 = 3 on an 8-word half.
    assert_eq!(rig.ch.entry_hist[3].load(Relaxed), 1);
}

/// A service so late the reader wrapped past the boundary still reports a
/// positive delay (the outer modulus in the model).
#[test]
fn entry_delay_wraps_positive() {
    let mut rig = Rig::new(16, 0);
    let rgb: Vec<u8> = (0..12).map(|_| 0xFF).collect();
    rig.start(&rgb);

    // Event at 8; delay service by 9 words: pos = 1 (wrapped), boundary = 8,
    // delay = (1 - 8) mod 16 = 9.
    rig.tx.advance(8);
    rig.tx.pending_thr = false;
    rig.tx.advance(9);
    rig.tx.pending_thr = false;
    // SAFETY: frame storage held by the rig.
    unsafe { service_threshold(&rig.ch, &mut rig.tx) };

    assert_eq!(rig.ch.entry_max.load(Relaxed), 9);
    // At or beyond the half: the overflow bucket.
    assert_eq!(rig.ch.entry_hist[crate::LAG_BUCKETS - 1].load(Relaxed), 1);
}

/// An inactive channel is ignored by every service path (causes are still
/// acknowledged by the vector; the model just never touches state).
#[test]
fn inactive_channel_untouched() {
    let mut rig = Rig::new(16, 0);
    // No start: active = 0.
    // SAFETY: no frame is read because the channel is inactive.
    unsafe { service_flags(&rig.ch, &mut rig.tx, true, true, false) };
    assert_eq!(rig.ch.frames.load(Relaxed), 0);
    assert_eq!(rig.ch.lag_count.load(Relaxed), 0);
    assert_eq!(rig.ch.complete.load(Relaxed), 0);
}

/// The guard is skipped (and counted) when the reader still sits on the guard
/// slot — the boundary case that would otherwise end a healthy frame.
#[test]
fn guard_skip_when_reader_on_slot() {
    let mut rig = Rig::new(16, 0);
    let rgb: Vec<u8> = (0..12).map(|_| 0xA5).collect();
    rig.start(&rgb);

    // Service the first event with zero latency: pos == 8 == guard slot.
    rig.tx.advance(8);
    rig.tx.pending_thr = false;
    // SAFETY: frame storage held by the rig.
    unsafe { service_threshold(&rig.ch, &mut rig.tx) };

    assert_eq!(rig.ch.skips.load(Relaxed), 1);
    assert_eq!(rig.ch.entry_max.load(Relaxed), 0);
}

/// `Pump`-schedule sanity against the oracle for a frame whose data ends
/// exactly on a half boundary (the latch-defers-to-next-half case).
#[test]
fn latch_on_boundary_matches_oracle() {
    // 2 pixels = 48 words = exactly 6 halves of 8: data never ends mid-half.
    let rgb: Vec<u8> = std::vec![0x01, 0x02, 0x03, 0xFE, 0xFD, 0xFC];

    let driver: Ws281xDriver<MockRmt, 1> = Ws281xDriver::new(MockRmt::new(1, 16));
    driver
        .configure_default_clock(0, &ChannelTiming::WS2812)
        .unwrap();
    // SAFETY: `rgb` outlives the pump run below, which completes the frame.
    unsafe { driver.start_frame(0, &rgb).unwrap() };
    Pump::default().run(&driver);
    let oracle = driver.hw().emitted(0);

    let mut rig = Rig::new(16, 0);
    rig.start(&rgb);
    rig.run_to_end();

    assert_eq!(rig.tx.emitted, oracle);
    assert_eq!(rig.ch.trips.load(Relaxed), 0);
}
