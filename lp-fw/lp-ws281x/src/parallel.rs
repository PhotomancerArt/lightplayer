//! Lockstep WS281x on a clocked parallel bus: up to eight lanes from one byte
//! stream (the ESP32-S3's LCD_CAM in i8080 mode, or any 8-bit DMA'd bus).
//!
//! **Spike code** (`lp2025/2026-10-20-2335-s3-radio-and-ota/led-channels-spike`).
//!
//! A bus that shifts out one byte per clock drives eight pins at once, one
//! bit of the byte per pin. A WS281x bit becomes `slots_per_bit` bus clocks:
//! the first `high_always` slots are high on every lane that is still sending,
//! the slots up to `high_one` are high only on lanes whose current bit is 1,
//! the rest are low. With 3 slots of 400 ns that is the textbook 400/800 ns
//! high time in a 1.2 µs bit; the 1-slots are the lanes' bits transposed into
//! one byte, which is the only real work the encoder does.
//!
//! Every lane clocks together: a frame lasts as long as the longest lane, and
//! a shorter lane is padded by holding its pin low once its bytes run out
//! (its `active` bit drops), which the strip reads as the start of its latch.
//!
//! The encoded frame sits in a DMA buffer the bus reads by itself, so the
//! transmission has **no deadline**: nothing on the CPU is involved between
//! the start and the end of the frame.
//!
//! [`decode_lane`] is the other direction: it reads one lane back out of a
//! stream of bus samples (the CAM half of LCD_CAM, sampling the pins at its
//! own clock) and reports the bytes and the measured pulse widths, so a
//! silicon harness can check a frame with no oscilloscope.
//!
//! ## Provenance
//!
//! Original code, written from the WS2812 datasheet's timing table and the
//! ESP32-S3 TRM's LCD_CAM chapter. The 8×8 transpose is the standard
//! delta-swap construction (three masked swaps on a 64-bit word), written
//! here from first principles. No LED-driver library's source was consulted.

use crate::timing::ColorOrder;

/// Lanes on an 8-bit bus.
pub const LANES: usize = 8;

/// How one WS281x bit maps onto bus slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotPlan {
    /// Bus clocks per WS281x bit.
    pub slots_per_bit: u8,
    /// Slots high for a 0 (and for a 1).
    pub high_always: u8,
    /// Slots high for a 1 (`> high_always`, `< slots_per_bit`).
    pub high_one: u8,
}

impl SlotPlan {
    /// Three slots: 1 high for a 0, 2 for a 1. At a 2.5 MHz bus clock
    /// (400 ns slots) that is T0H 400, T1H 800, period 1200 ns.
    pub const THREE: SlotPlan = SlotPlan {
        slots_per_bit: 3,
        high_always: 1,
        high_one: 2,
    };

    /// Bytes one WS281x byte occupies on the bus.
    pub const fn bus_bytes_per_byte(self) -> usize {
        8 * self.slots_per_bit as usize
    }
}

/// Bus bytes for a frame whose longest lane has `max_lane_bytes` wire bytes,
/// plus `tail` low bytes (the latch, or part of it).
pub const fn encoded_len(plan: SlotPlan, max_lane_bytes: usize, tail: usize) -> usize {
    max_lane_bytes * plan.bus_bytes_per_byte() + tail
}

/// Encode up to eight lanes of RGB triplets into `out`, each pixel's bytes
/// sent in `order` (WS2812 parts want [`ColorOrder::Grb`]; [`ColorOrder::Rgb`]
/// sends the bytes as given), followed by `tail` zero bytes. Returns the
/// bytes written.
///
/// Lane `i` drives bus bit `i`. A lane's length is a whole number of pixels
/// (a trailing partial pixel is sent as given). Panics if `out` is too small
/// or more than eight lanes are given; the caller sizes `out` with
/// [`encoded_len`].
pub fn encode_frame(
    plan: SlotPlan,
    lanes: &[&[u8]],
    order: ColorOrder,
    tail: usize,
    out: &mut [u8],
) -> usize {
    assert!(lanes.len() <= LANES, "at most eight lanes");
    let max = lanes.iter().map(|lane| lane.len()).max().unwrap_or(0);
    let len = encoded_len(plan, max, tail);
    assert!(out.len() >= len, "output buffer too small");

    let per_byte = plan.bus_bytes_per_byte();
    if plan == SlotPlan::THREE {
        // The fast path: a fixed 24-byte chunk per wire byte, so the stores
        // need no bounds checks and the bit loop unrolls.
        for index in 0..max {
            let (packed, active) = gather(lanes, order, index);
            let columns = transpose8(packed).to_le_bytes();
            let chunk: &mut [u8; 24] = (&mut out[index * 24..index * 24 + 24])
                .try_into()
                .expect("24-byte chunk");
            for bit in 0..8 {
                chunk[bit * 3] = active;
                chunk[bit * 3 + 1] = columns[7 - bit];
                chunk[bit * 3 + 2] = 0;
            }
        }
    } else {
        let spb = plan.slots_per_bit as usize;
        for index in 0..max {
            let (packed, active) = gather(lanes, order, index);
            // After the transpose, byte `b` holds bit `b` of every lane.
            let columns = transpose8(packed).to_le_bytes();
            let chunk = &mut out[index * per_byte..(index + 1) * per_byte];
            for bit in 0..8 {
                // MSB first on the wire.
                let ones = columns[7 - bit];
                let slots = &mut chunk[bit * spb..(bit + 1) * spb];
                for (slot, out) in slots.iter_mut().enumerate() {
                    *out = if slot < plan.high_always as usize {
                        active
                    } else if slot < plan.high_one as usize {
                        ones
                    } else {
                        0
                    };
                }
            }
        }
    }
    out[max * per_byte..len].fill(0);
    len
}

/// Wire byte `index` of every lane packed into one word (lane `i` in byte
/// `i`), and the mask of lanes that still have a byte there.
#[inline(always)]
fn gather(lanes: &[&[u8]], order: ColorOrder, index: usize) -> (u64, u8) {
    let slot = index % 3;
    let source = index - slot + order.source_index(slot);
    let mut packed = 0u64;
    let mut active = 0u8;
    for (lane, bytes) in lanes.iter().enumerate() {
        if index < bytes.len() {
            // A trailing partial pixel has no permutation: send it as given.
            let byte = bytes.get(source).copied().unwrap_or(bytes[index]);
            packed |= (byte as u64) << (8 * lane);
            active |= 1 << lane;
        }
    }
    (packed, active)
}

/// Transpose an 8×8 bit matrix held as eight bytes: on return, bit `r` of
/// byte `c` is what bit `c` of byte `r` was.
pub const fn transpose8(mut x: u64) -> u64 {
    let t = (x ^ (x >> 7)) & 0x00AA_00AA_00AA_00AA;
    x ^= t ^ (t << 7);
    let t = (x ^ (x >> 14)) & 0x0000_CCCC_0000_CCCC;
    x ^= t ^ (t << 14);
    let t = (x ^ (x >> 28)) & 0x0000_0000_F0F0_F0F0;
    x ^= t ^ (t << 28);
    x
}

/// What [`decode_lane`] measured, in samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LaneDecode {
    /// Bits decoded.
    pub bits: usize,
    /// Sample index of the frame's first rising edge.
    pub first_edge: usize,
    /// High time of a 0 bit, min and max.
    pub t0h: (u32, u32),
    /// High time of a 1 bit, min and max.
    pub t1h: (u32, u32),
    /// Rising edge to rising edge, min and max (every bit but the last).
    pub period: (u32, u32),
    /// The low after the last bit, as far as the capture saw it.
    pub tail_low: u32,
}

/// Why a lane did not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The lane was already high at the first sample: the capture began
    /// inside a frame.
    StartedHigh,
    /// No rising edge at all.
    Silent,
    /// The line went high and stayed high to the end of the capture.
    StuckHigh,
    /// More bits than `out` holds.
    TooLong,
}

/// Read lane `lane` out of `samples` (one bus sample per byte, bit `lane`
/// the pin's level) into `out`, MSB first.
///
/// A high run longer than `threshold` samples is a 1. The frame ends at the
/// first low run of at least `idle` samples, or at the end of the capture.
/// `out` is zeroed first; a frame that is not a whole number of bytes leaves
/// its last byte partial (the caller compares `bits`).
pub fn decode_lane(
    samples: &[u8],
    lane: u8,
    threshold: u32,
    idle: u32,
    out: &mut [u8],
) -> Result<LaneDecode, DecodeError> {
    let mask = 1u8 << lane;
    let level = |i: usize| samples[i] & mask != 0;
    out.fill(0);
    if samples.is_empty() {
        return Err(DecodeError::Silent);
    }
    if level(0) {
        return Err(DecodeError::StartedHigh);
    }
    let mut i = 0;
    while i < samples.len() && !level(i) {
        i += 1;
    }
    if i == samples.len() {
        return Err(DecodeError::Silent);
    }

    let mut result = LaneDecode {
        first_edge: i,
        t0h: (u32::MAX, 0),
        t1h: (u32::MAX, 0),
        period: (u32::MAX, 0),
        ..LaneDecode::default()
    };
    // `i` is at a rising edge.
    loop {
        let rise = i;
        while i < samples.len() && level(i) {
            i += 1;
        }
        if i == samples.len() {
            return Err(DecodeError::StuckHigh);
        }
        let high = (i - rise) as u32;
        let fall = i;
        while i < samples.len() && !level(i) {
            i += 1;
        }
        let low = (i - fall) as u32;

        let byte = result.bits / 8;
        if byte >= out.len() {
            return Err(DecodeError::TooLong);
        }
        let one = high > threshold;
        if one {
            out[byte] |= 0x80 >> (result.bits % 8);
            result.t1h = (result.t1h.0.min(high), result.t1h.1.max(high));
        } else {
            result.t0h = (result.t0h.0.min(high), result.t0h.1.max(high));
        }
        result.bits += 1;

        if i == samples.len() || low >= idle {
            result.tail_low = low;
            break;
        }
        let period = (i - rise) as u32;
        result.period = (result.period.0.min(period), result.period.1.max(period));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    #[test]
    fn transpose8_matches_the_definition() {
        let mut seed = 0x1234_5678_9abc_def0u64;
        for _ in 0..1000 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let x = seed;
            let t = transpose8(x);
            for r in 0..8 {
                for c in 0..8 {
                    let src = (x >> (8 * r + c)) & 1;
                    let dst = (t >> (8 * c + r)) & 1;
                    assert_eq!(src, dst, "r={r} c={c}");
                }
            }
        }
    }

    #[test]
    fn three_slot_layout() {
        // One lane, one byte 0b1000_0001.
        let lanes: [&[u8]; 1] = [&[0x81]];
        let mut out = [0xEEu8; 24 + 2];
        let n = encode_frame(SlotPlan::THREE, &lanes, ColorOrder::Rgb, 2, &mut out);
        assert_eq!(n, 26);
        assert_eq!(&out[0..3], &[1, 1, 0]); // MSB = 1
        assert_eq!(&out[3..6], &[1, 0, 0]); // 0
        assert_eq!(&out[21..24], &[1, 1, 0]); // LSB = 1
        assert_eq!(&out[24..26], &[0, 0]);
    }

    #[test]
    fn four_slot_plan_uses_the_general_path() {
        let plan = SlotPlan {
            slots_per_bit: 4,
            high_always: 1,
            high_one: 3,
        };
        let lanes: [&[u8]; 2] = [&[0x80], &[0x00]];
        let mut out = [0xEEu8; 32];
        encode_frame(plan, &lanes, ColorOrder::Rgb, 0, &mut out);
        // Bit 7: lane 0 is 1 (three slots high), lane 1 is 0 (one slot).
        assert_eq!(&out[0..4], &[0b11, 0b01, 0b01, 0]);
        assert_eq!(&out[4..8], &[0b11, 0, 0, 0]);
    }

    #[test]
    fn grb_order_sends_green_first() {
        // One pixel R=0x01 G=0x80 B=0x00: GRB puts 0x80 first on the wire.
        let lanes: [&[u8]; 1] = [&[0x01, 0x80, 0x00]];
        let mut out = vec![0u8; encoded_len(SlotPlan::THREE, 3, 0)];
        encode_frame(SlotPlan::THREE, &lanes, ColorOrder::Grb, 0, &mut out);
        // First wire byte (G = 0x80): MSB is a 1, the rest 0.
        assert_eq!(&out[0..3], &[1, 1, 0]);
        assert_eq!(&out[3..6], &[1, 0, 0]);
        // Second wire byte (R = 0x01): only the LSB is a 1.
        assert_eq!(&out[24..27], &[1, 0, 0]);
        assert_eq!(&out[45..48], &[1, 1, 0]);
    }

    #[test]
    fn short_lanes_drop_out() {
        let lanes: [&[u8]; 3] = [&[0xFF, 0xFF], &[0xFF], &[]];
        let mut out = vec![0u8; encoded_len(SlotPlan::THREE, 2, 0)];
        encode_frame(SlotPlan::THREE, &lanes, ColorOrder::Rgb, 0, &mut out);
        assert_eq!(&out[0..3], &[0b011, 0b011, 0]);
        assert_eq!(&out[24..27], &[0b001, 0b001, 0]);
    }

    /// Resample a bus stream at `samples_per_slot` (fractional), as an
    /// asynchronous capture clock would, with `lead` low samples first.
    fn resample(bus: &[u8], samples_per_slot: f64, lead: usize, trail: usize) -> Vec<u8> {
        let total = (bus.len() as f64 * samples_per_slot) as usize;
        let mut out = vec![0u8; lead];
        for s in 0..total {
            out.push(bus[(s as f64 / samples_per_slot) as usize]);
        }
        out.extend(std::iter::repeat(0).take(trail));
        out
    }

    #[test]
    fn decode_reads_back_every_lane() {
        let lane_bytes: Vec<Vec<u8>> = (0..8)
            .map(|lane| {
                (0..30 + lane * 5)
                    .map(|i| (i * 37 + lane * 11) as u8)
                    .collect()
            })
            .collect();
        let lanes: Vec<&[u8]> = lane_bytes.iter().map(|lane| lane.as_slice()).collect();
        let max = lanes.iter().map(|lane| lane.len()).max().unwrap();
        let mut bus = vec![0u8; encoded_len(SlotPlan::THREE, max, 8)];
        encode_frame(SlotPlan::THREE, &lanes, ColorOrder::Rgb, 8, &mut bus);

        // 8 MHz sampling of 2.5 MHz slots: 3.2 samples a slot.
        let samples = resample(&bus, 3.2, 17, 400);
        for (lane, expected) in lane_bytes.iter().enumerate() {
            let mut got = vec![0u8; 80];
            let d = decode_lane(&samples, lane as u8, 5, 200, &mut got).unwrap();
            assert_eq!(d.bits, expected.len() * 8, "lane {lane}");
            assert_eq!(&got[..expected.len()], expected.as_slice(), "lane {lane}");
            assert!(d.t0h.0 >= 3 && d.t0h.1 <= 4, "{d:?}");
            assert!(d.t1h.0 >= 6 && d.t1h.1 <= 7, "{d:?}");
            assert!(d.period.0 >= 9 && d.period.1 <= 10, "{d:?}");
        }
    }

    #[test]
    fn decode_refuses_a_capture_that_starts_high() {
        assert_eq!(
            decode_lane(&[1, 1, 0], 0, 5, 10, &mut [0; 4]),
            Err(DecodeError::StartedHigh)
        );
        assert_eq!(
            decode_lane(&[0, 0, 0], 0, 5, 10, &mut [0; 4]),
            Err(DecodeError::Silent)
        );
    }
}
