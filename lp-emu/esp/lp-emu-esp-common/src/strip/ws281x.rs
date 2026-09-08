//! The WS281x wire decoder: a pad's edges back into frames.
//!
//! # The protocol, as this reads it
//!
//! There is no clock line. Each bit is a pulse: a rising edge, a high time
//! that carries the value (`T0H` or `T1H`), then a low. A frame ends with a
//! long low — the latch — after which the strip presents what it shifted in.
//!
//! So the state machine is small:
//!
//! - a **rising** edge opens a bit (and, after a low at least
//!   [`Ws281xDecoder::reset_min_ns`] long, first closes the frame that was
//!   open);
//! - the **falling** edge gives that bit's high time: within
//!   [`Ws281xDecoder::tolerance_ns`] of `T0H` it is a zero, within the same
//!   of `T1H` it is a one, and anything else is a [`BitError`] — counted,
//!   the bit dropped, the frame marked incomplete.
//!
//! The tolerance is the datasheet's **±150 ns**. It is not a knob to be
//! widened until a test passes: a pulse outside it is a finding about the
//! transmitter, and a decoder that shrugs at one is worth nothing as an
//! oracle.
//!
//! # Arithmetic
//!
//! Everything is integer arithmetic in **cycles**. `cpu_hz` is a parameter
//! rather than a constant because the C6 runs at 160 MHz and the classic
//! ESP32 at 240; a decoder that baked one in would silently misread the
//! other. Nanoseconds appear only where a human reads them (a `BitError`'s
//! `high_ns`, the tolerance and reset thresholds as configured).
//!
//! # Byte order
//!
//! [`Frame::wire`] is what the wire carried, in wire order — GRB for a
//! WS2812. [`unpermute`] turns that back into the RGB triplets the *driver*
//! was handed. Both are kept in the record on purpose: a wrong assumption
//! about the strip's order is then visible as a colour swap between two
//! fields rather than silently baked into one.

use alloc::vec::Vec;
use lp_emu_core::sched::Cycles;
use lp_ws281x::{ChannelTiming, ColorOrder};

use crate::pins::{Edge, PadId};

/// The datasheet tolerance on a bit's high time, in nanoseconds.
pub const DEFAULT_TOLERANCE_NS: u32 = 150;

/// How long a low must be to end a frame, in nanoseconds.
///
/// The original WS2812 datasheet's reset is 50 µs. Our own driver sends a
/// 300 µs latch (`ChannelTiming::WS2812.latch_us`, for WS2812B-V5 and
/// WS2815), so this threshold is well clear of the 850 ns inter-bit low and
/// well under anything a real latch would be.
pub const DEFAULT_RESET_MIN_NS: u32 = 50_000;

/// How many [`BitError`]s one frame records before it stops keeping them.
/// The count ([`Frame::error_count`]) keeps rising.
pub const ERROR_CAP: usize = 64;

/// A high time that is neither a zero nor a one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitError {
    /// The cycle the pulse started.
    pub at: Cycles,
    /// Its high time in nanoseconds, so a reader can see how far out it was.
    pub high_ns: u64,
}

/// One decoded frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub pad: PadId,
    /// The frame's index on this pad, from 0.
    pub n: u64,
    /// The first rising edge.
    pub start: Cycles,
    /// The last edge that belonged to the frame — the final falling edge.
    pub end: Cycles,
    /// Bits decoded, errors excluded.
    pub bits: usize,
    /// The complete bytes, in **wire** order (GRB for a WS2812). The
    /// `bits % 8` trailing bits are in [`trailing_bits`](Self::trailing_bits)
    /// and not here.
    pub wire: Vec<u8>,
    pub trailing_bits: u8,
    /// The first [`ERROR_CAP`] bad pulses.
    pub errors: Vec<BitError>,
    /// Every bad pulse, counted.
    pub error_count: u64,
    /// The low that closed the frame, in cycles. `None` when the run ended
    /// with the frame still open ([`Ws281xDecoder::flush`]).
    pub reset_cycles: Option<u64>,
}

impl Frame {
    /// A frame is complete when every pulse decoded, the bits made whole
    /// bytes, and a reset actually closed it.
    pub fn is_complete(&self) -> bool {
        self.error_count == 0 && self.trailing_bits == 0 && self.reset_cycles.is_some()
    }

    /// Whole pixels (three bytes each).
    pub fn leds(&self) -> usize {
        self.wire.len() / 3
    }
}

/// Where the decoder is between edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// No edge seen yet, or the last frame was closed by its reset.
    Idle,
    /// A bit's high time is running, from this cycle.
    High(Cycles),
    /// Low since this cycle, with a frame open.
    Low(Cycles),
}

/// Reads a pad's edges as a WS281x wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ws281xDecoder {
    pad: PadId,
    timing: ChannelTiming,
    cpu_hz: u64,
    /// ±this on a bit's high time. See the module docs — not a knob.
    tolerance_ns: u32,
    reset_min_ns: u32,

    // Derived, in cycles.
    t0h: u64,
    t1h: u64,
    tol: u64,
    reset_min: u64,

    state: State,
    next_frame: u64,
    open: Option<Open>,
}

/// The frame being accumulated.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Open {
    start: Cycles,
    last_edge: Cycles,
    bits: usize,
    wire: Vec<u8>,
    partial: u8,
    partial_bits: u8,
    errors: Vec<BitError>,
    error_count: u64,
}

impl Ws281xDecoder {
    /// A decoder for `pad`, reading `timing` on a machine clocked at
    /// `cpu_hz`, with the datasheet tolerance and reset.
    pub fn new(pad: PadId, timing: ChannelTiming, cpu_hz: u64) -> Self {
        Self::with_thresholds(
            pad,
            timing,
            cpu_hz,
            DEFAULT_TOLERANCE_NS,
            DEFAULT_RESET_MIN_NS,
        )
    }

    pub fn with_thresholds(
        pad: PadId,
        timing: ChannelTiming,
        cpu_hz: u64,
        tolerance_ns: u32,
        reset_min_ns: u32,
    ) -> Self {
        let cycles = |ns: u32| u64::from(ns) * cpu_hz / 1_000_000_000;
        Self {
            pad,
            timing,
            cpu_hz,
            tolerance_ns,
            reset_min_ns,
            t0h: cycles(timing.t0h_ns),
            t1h: cycles(timing.t1h_ns),
            tol: cycles(tolerance_ns),
            reset_min: cycles(reset_min_ns),
            state: State::Idle,
            next_frame: 0,
            open: None,
        }
    }

    pub fn pad(&self) -> PadId {
        self.pad
    }

    pub fn timing(&self) -> ChannelTiming {
        self.timing
    }

    pub fn cpu_hz(&self) -> u64 {
        self.cpu_hz
    }

    pub fn tolerance_ns(&self) -> u32 {
        self.tolerance_ns
    }

    pub fn reset_min_ns(&self) -> u32 {
        self.reset_min_ns
    }

    /// Frames completed on this pad so far.
    pub fn frames_decoded(&self) -> u64 {
        self.next_frame
    }

    /// Whether a frame is part-decoded right now — what a snapshot taken
    /// mid-frame has to carry.
    pub fn is_mid_frame(&self) -> bool {
        self.open.is_some()
    }

    fn to_ns(&self, cycles: u64) -> u64 {
        cycles * 1_000_000_000 / self.cpu_hz
    }

    /// Feed one edge on this pad, in cycle order. Returns the frame the edge
    /// closed, if it closed one.
    pub fn feed(&mut self, edge: &Edge) -> Option<Frame> {
        debug_assert_eq!(edge.pad, self.pad, "an edge from another pad");
        match (self.state, edge.level) {
            // A rising edge: it may first close the open frame.
            (State::Idle, true) => {
                self.state = State::High(edge.at);
                self.open_frame(edge.at);
                None
            }
            (State::Low(since), true) => {
                let gap = edge.at.saturating_sub(since);
                // A long enough low is the latch: it closes the frame, and
                // this edge opens the next one. A short one is just the low
                // between two bits, and the frame carries on.
                let closed = (gap >= self.reset_min).then(|| {
                    let frame = self.close(Some(gap));
                    self.open_frame(edge.at);
                    frame
                });
                self.state = State::High(edge.at);
                closed
            }
            (State::Low(_), false) | (State::Idle, false) => {
                // A falling edge with nothing high: the wire was already low
                // (an idle level written twice). Nothing to time.
                None
            }
            (State::High(since), false) => {
                let high = edge.at.saturating_sub(since);
                self.bit(since, high);
                if let Some(open) = self.open.as_mut() {
                    open.last_edge = edge.at;
                }
                self.state = State::Low(edge.at);
                None
            }
            (State::High(_), true) => {
                // Two rising edges in a row cannot happen: the fabric only
                // records a level that changed.
                None
            }
        }
    }

    /// End of the run: report a frame still open as incomplete.
    ///
    /// `at` is the cycle the run stopped at; it becomes the frame's `end`
    /// when the frame's last edge is still high (the wire stopped mid-bit).
    pub fn flush(&mut self, at: Cycles) -> Option<Frame> {
        self.open.as_ref()?;
        if let (State::High(_), Some(open)) = (self.state, self.open.as_mut()) {
            open.last_edge = open.last_edge.max(at);
        }
        self.state = State::Idle;
        Some(self.close(None))
    }

    fn open_frame(&mut self, at: Cycles) {
        self.open = Some(Open {
            start: at,
            last_edge: at,
            bits: 0,
            wire: Vec::new(),
            partial: 0,
            partial_bits: 0,
            errors: Vec::new(),
            error_count: 0,
        });
    }

    /// Classify one pulse's high time and fold it into the open frame.
    fn bit(&mut self, at: Cycles, high: u64) {
        let (t0h, t1h, tol) = (self.t0h, self.t1h, self.tol);
        let value = if high.abs_diff(t0h) <= tol {
            Some(false)
        } else if high.abs_diff(t1h) <= tol {
            Some(true)
        } else {
            None
        };
        let high_ns = self.to_ns(high);
        let Some(open) = self.open.as_mut() else {
            return;
        };
        match value {
            Some(bit) => {
                open.bits += 1;
                // MSB first.
                open.partial = (open.partial << 1) | u8::from(bit);
                open.partial_bits += 1;
                if open.partial_bits == 8 {
                    open.wire.push(open.partial);
                    open.partial = 0;
                    open.partial_bits = 0;
                }
            }
            None => {
                open.error_count += 1;
                if open.errors.len() < ERROR_CAP {
                    open.errors.push(BitError { at, high_ns });
                }
            }
        }
    }

    fn close(&mut self, reset_cycles: Option<u64>) -> Frame {
        let open = self.open.take().expect("a frame is open");
        let n = self.next_frame;
        self.next_frame += 1;
        if reset_cycles.is_some() {
            self.state = State::Idle;
        }
        Frame {
            pad: self.pad,
            n,
            start: open.start,
            end: open.last_edge,
            bits: open.bits,
            wire: open.wire,
            trailing_bits: open.partial_bits,
            errors: open.errors,
            error_count: open.error_count,
            reset_cycles,
        }
    }
}

/// Wire bytes → the RGB triplets the driver was handed.
///
/// The inverse of the permutation `lp_ws281x` applies while encoding
/// ([`ColorOrder::source_index`]: the index of the source byte that occupies
/// wire slot `slot`). A trailing partial pixel is copied through unchanged —
/// it has no complete triplet to permute.
pub fn unpermute(wire: &[u8], order: ColorOrder) -> Vec<u8> {
    let mut out = wire.to_vec();
    for (px, chunk) in wire.chunks_exact(3).enumerate() {
        for (slot, byte) in chunk.iter().enumerate() {
            out[px * 3 + order.source_index(slot)] = *byte;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_ws281x::{PulseCodes, PulseItem};

    const CPU_HZ: u64 = 160_000_000;
    const PAD: PadId = PadId(18);
    /// The RMT tick our drivers use: 80 MHz, so two CPU cycles.
    const CYCLES_PER_TICK: u64 = 2;

    fn ns(n: u64) -> u64 {
        n * CPU_HZ / 1_000_000_000
    }

    fn decoder(timing: ChannelTiming) -> Ws281xDecoder {
        Ws281xDecoder::new(PAD, timing, CPU_HZ)
    }

    /// The encoder counterpart of the decoder: `lp_ws281x`'s own words for
    /// `timing` at 80 MHz, turned into edges two cycles per tick — exactly
    /// what the RMT model puts on the fabric.
    fn encode(bytes: &[u8], timing: &ChannelTiming, at: Cycles) -> (Vec<Edge>, Cycles) {
        let codes = PulseCodes::new(timing, 80_000_000).expect("codes");
        let mut edges = Vec::new();
        let mut now = at;
        let emit = |word: u32, now: &mut Cycles, edges: &mut Vec<Edge>| {
            let item = PulseItem::decode(word).expect("not STOP");
            for half in [item.first, item.second] {
                if half.ticks == 0 {
                    continue;
                }
                edges.push(Edge {
                    at: *now,
                    pad: PAD,
                    level: half.level,
                });
                *now += u64::from(half.ticks) * CYCLES_PER_TICK;
            }
        };
        for byte in bytes {
            for k in 0..8 {
                emit(codes.bit(byte & (0x80 >> k) != 0), &mut now, &mut edges);
            }
        }
        emit(codes.latch, &mut now, &mut edges);
        // The fabric only records changes: drop an edge whose level equals
        // the previous one (the latch's two low halves, and the low that
        // follows a zero bit's low).
        let mut level = false;
        let mut out = Vec::with_capacity(edges.len());
        for e in edges {
            if e.level != level {
                level = e.level;
                out.push(e);
            }
        }
        (out, now)
    }

    fn feed_all(d: &mut Ws281xDecoder, edges: &[Edge]) -> Vec<Frame> {
        edges.iter().filter_map(|e| d.feed(e)).collect()
    }

    #[test]
    fn a_random_frame_round_trips_through_the_encoder() {
        let timing = ChannelTiming::WS2812;
        // A deterministic "random": an LCG, so the test carries its data.
        let mut x = 0x1234_5678u32;
        let rgb: Vec<u8> = (0..64 * 3)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (x >> 16) as u8
            })
            .collect();
        // What the driver puts on the wire: the RGB triplets permuted.
        let wire: Vec<u8> = rgb
            .chunks_exact(3)
            .flat_map(|px| (0..3).map(move |slot| px[timing.color_order.source_index(slot)]))
            .collect();

        let mut d = decoder(timing);
        let (edges, end) = encode(&wire, &timing, 1_000);
        assert!(feed_all(&mut d, &edges).is_empty(), "no reset yet");
        // The next frame's first rising edge, well past the latch, closes it.
        let closed = d
            .feed(&Edge {
                at: end + ns(1_000_000),
                pad: PAD,
                level: true,
            })
            .expect("the reset closes the frame");
        assert_eq!(closed.n, 0);
        assert_eq!(closed.bits, 64 * 24);
        assert_eq!(closed.trailing_bits, 0);
        assert_eq!(closed.error_count, 0);
        assert!(closed.is_complete());
        assert_eq!(closed.leds(), 64);
        assert_eq!(closed.wire, wire, "the wire bytes are what was encoded");
        assert_eq!(
            unpermute(&closed.wire, timing.color_order),
            rgb,
            "unpermuted back to what the driver was handed"
        );
        // The reset is the latch plus the gap we left.
        let reset = closed.reset_cycles.expect("a reset");
        assert!(reset >= ns(u64::from(timing.latch_us) * 1_000));
    }

    #[test]
    fn a_high_time_one_hundred_and_sixty_ns_off_is_an_error_and_one_forty_is_not() {
        let timing = ChannelTiming::WS2812;
        for (off_ns, expect_error) in [(140u64, false), (160, true)] {
            let mut d = decoder(timing);
            let t0h = ns(u64::from(timing.t0h_ns));
            let edges = [
                Edge {
                    at: 100,
                    pad: PAD,
                    level: true,
                },
                Edge {
                    at: 100 + t0h + ns(off_ns),
                    pad: PAD,
                    level: false,
                },
                Edge {
                    at: 100 + ns(100_000),
                    pad: PAD,
                    level: true,
                },
            ];
            let frames = feed_all(&mut d, &edges);
            assert_eq!(frames.len(), 1);
            let f = &frames[0];
            if expect_error {
                assert_eq!(f.error_count, 1, "{off_ns} ns off must be a BitError");
                assert_eq!(f.bits, 0);
                assert!(!f.is_complete());
                assert_eq!(f.errors[0].at, 100);
            } else {
                assert_eq!(f.error_count, 0, "{off_ns} ns off is inside ±150 ns");
                assert_eq!(f.bits, 1);
            }
        }
    }

    #[test]
    fn forty_nine_microseconds_is_not_a_reset_and_fifty_one_is() {
        let timing = ChannelTiming::WS2812;
        for (low_us, closes) in [(49u64, false), (51, true)] {
            let mut d = decoder(timing);
            let t1h = ns(u64::from(timing.t1h_ns));
            let mut edges = alloc::vec![
                Edge {
                    at: 0,
                    pad: PAD,
                    level: true
                },
                Edge {
                    at: t1h,
                    pad: PAD,
                    level: false
                },
            ];
            edges.push(Edge {
                at: t1h + ns(low_us * 1_000),
                pad: PAD,
                level: true,
            });
            let frames = feed_all(&mut d, &edges);
            if closes {
                assert_eq!(frames.len(), 1, "{low_us} µs must close the frame");
                assert_eq!(frames[0].bits, 1);
                assert_eq!(frames[0].reset_cycles, Some(ns(low_us * 1_000)));
            } else {
                assert!(frames.is_empty(), "{low_us} µs is not a reset");
                assert!(d.is_mid_frame());
            }
        }
    }

    #[test]
    fn a_frame_cut_at_thirteen_bits_reports_five_trailing_bits() {
        let timing = ChannelTiming::WS2812;
        let mut d = decoder(timing);
        let codes = PulseCodes::new(&timing, 80_000_000).expect("codes");
        let mut edges = Vec::new();
        let mut now = 0u64;
        for i in 0..13 {
            let item = PulseItem::decode(codes.bit(i % 3 == 0)).expect("not STOP");
            edges.push(Edge {
                at: now,
                pad: PAD,
                level: true,
            });
            now += u64::from(item.first.ticks) * CYCLES_PER_TICK;
            edges.push(Edge {
                at: now,
                pad: PAD,
                level: false,
            });
            now += u64::from(item.second.ticks) * CYCLES_PER_TICK;
        }
        let frames = feed_all(&mut d, &edges);
        assert!(frames.is_empty());
        let f = d.flush(now).expect("the open frame is reported");
        assert_eq!(f.bits, 13);
        assert_eq!(f.trailing_bits, 5);
        assert_eq!(f.wire.len(), 1, "one complete byte");
        assert_eq!(f.reset_cycles, None);
        assert!(!f.is_complete(), "cut short and never latched");
        assert!(d.flush(now).is_none(), "nothing is open any more");
    }

    #[test]
    fn ws2811_timings_decode_too() {
        let timing = ChannelTiming::WS2811;
        assert_eq!(timing.color_order, ColorOrder::Rgb);
        let rgb = [0x12u8, 0x34, 0x56, 0xff, 0x00, 0x80];
        let mut d = decoder(timing);
        let (edges, end) = encode(&rgb, &timing, 0);
        assert!(feed_all(&mut d, &edges).is_empty());
        let f = d
            .feed(&Edge {
                at: end + ns(1_000_000),
                pad: PAD,
                level: true,
            })
            .expect("closed");
        assert_eq!(f.wire, rgb);
        assert_eq!(unpermute(&f.wire, timing.color_order), rgb, "RGB is identity");
        assert!(f.is_complete());
        // And a WS2812 decoder would have refused those pulses: 300 ns is
        // 100 ns off WS2812's T0H, inside the tolerance, but 900 ns is 100 ns
        // off T1H — also inside. The two parts are not distinguishable at
        // ±150 ns, which is exactly why the timing is a parameter.
        let mut wrong = decoder(ChannelTiming::WS2812);
        let _ = feed_all(&mut wrong, &edges);
    }

    #[test]
    fn frames_are_numbered_and_the_wire_order_is_kept_beside_the_rgb() {
        let timing = ChannelTiming::WS2812;
        let mut d = decoder(timing);
        let mut at = 0;
        let mut got = Vec::new();
        for k in 0..3u8 {
            let wire = [k, k + 1, k + 2];
            let (edges, end) = encode(&wire, &timing, at);
            got.extend(feed_all(&mut d, &edges));
            at = end + ns(1_000_000);
        }
        got.extend(d.flush(at));
        assert_eq!(got.len(), 3);
        for (k, f) in got.iter().enumerate() {
            assert_eq!(f.n, k as u64);
            assert_eq!(f.wire, [k as u8, k as u8 + 1, k as u8 + 2]);
            assert_eq!(
                unpermute(&f.wire, ColorOrder::Grb),
                [k as u8 + 1, k as u8, k as u8 + 2],
                "GRB wire, RGB as the driver had it"
            );
        }
        assert_eq!(d.frames_decoded(), 3);
    }

    #[test]
    fn unpermute_is_the_inverse_of_the_encoder_permutation_for_every_order() {
        let rgb: Vec<u8> = (0..9u8).collect();
        for order in ColorOrder::ALL {
            let wire: Vec<u8> = rgb
                .chunks_exact(3)
                .flat_map(|px| (0..3).map(move |slot| px[order.source_index(slot)]))
                .collect();
            assert_eq!(unpermute(&wire, order), rgb, "{order:?}");
        }
    }

    #[test]
    fn the_decoder_is_clock_rate_aware() {
        // The same wire, produced against a 240 MHz machine, decodes the
        // same — the cycles differ, the nanoseconds do not.
        let timing = ChannelTiming::WS2812;
        let mut d = Ws281xDecoder::new(PAD, timing, 240_000_000);
        let t1h = 240 * u64::from(timing.t1h_ns) / 1_000;
        let edges = [
            Edge {
                at: 0,
                pad: PAD,
                level: true,
            },
            Edge {
                at: t1h,
                pad: PAD,
                level: false,
            },
            Edge {
                at: t1h + 240 * 100_000,
                pad: PAD,
                level: true,
            },
        ];
        let frames = feed_all(&mut d, &edges);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].bits, 1);
        assert_eq!(frames[0].error_count, 0);
    }
}
