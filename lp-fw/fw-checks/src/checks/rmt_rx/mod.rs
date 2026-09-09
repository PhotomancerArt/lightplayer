//! The `rmt-rx` payload: a frame put on a pad and read back off another one,
//! and the two checksums compared.
//!
//! Every other RMT payload in this crate says what the guest *sent*.
//! [`rmt_chase`](crate::checks::rmt_chase) is the one that does it best: one
//! `rmt-frame` record per frame, carrying the FNV-1a of the bytes the driver
//! was handed. What nothing said until now is what came **back**, because
//! until M2 P3 the emulator's RMT had transmitters only and the receivers
//! were accept-and-remember.
//!
//! So this payload runs both halves at once. Each frame is
//!
//! 1. encoded into WS2812 pulse codes ([`encode_frame`]) and transmitted on
//!    RMT channel 0, out of GPIO18 — the pad the product's strip uses;
//! 2. received on RMT channel 2, in from GPIO19, into that channel's RAM
//!    window;
//! 3. decoded back into bytes ([`decode_frame`]) and checksummed with the
//!    **same** [`fnv1a`](crate::checks::rmt_chase::fnv1a) the transmitting
//!    side uses.
//!
//! and the transcript carries both numbers:
//!
//! ```text
//! [fw-check-json] {"kind":"rmt-frame","n":0,"leds":64,"lit":1,"crc":"0x0d1a6f6b"}
//! [fw-check-json] {"kind":"rmt-rx","n":0,"words":1537,"crc":"0x0d1a6f6b"}
//! ```
//!
//! **The gate is that those two checksums are equal, frame for frame.** One
//! is what the guest built; the other is what a receiver measured off a wire.
//! Nothing in the middle is shared: the encoder writes durations, the pad
//! carries levels, and the decoder reads durations back.
//!
//! # The wire is a wire
//!
//! On an emulated configuration the two pads are tied with `--wire 18:19`,
//! which is a jumper in the signal fabric and nothing else — the transmitter
//! drives gpio18, the fabric resolves the tied group, and the receiver
//! samples gpio19 through `GPIO.func_in_sel_cfg[71]`. On silicon it is an
//! actual jumper between the two header pins, which is the one step of this
//! payload that needs hands; the desk batch's optional item has the
//! procedure. Neither side simulates the other: the emulated run's claim is
//! that *its* receiver read what *its* transmitter sent.
//!
//! # Why this payload encodes its own frames
//!
//! The chase drives the product's `LedChannel`, which publishes a
//! one-channel block plan and takes **all four** RMT RAM blocks for its
//! transmitter — including block 2, which is the receiver's window. A
//! loopback needs the shipped two-channel shape instead, so this payload
//! talks to esp-hal's `Channel<Tx>` directly with codes it built here, and
//! [`encode_frame`] is host-tested against [`decode_frame`] so that neither
//! can drift without the other noticing. What it keeps from the chase is the
//! part that matters for the comparison: the pattern
//! ([`chase_frame`](crate::checks::rmt_chase::chase_frame)), the checksum,
//! and the `rmt-frame` record's exact spelling.
//!
//! # The numbers, and where each comes from
//!
//! [`T0H`], [`T0L`], [`T1H`], [`T1L`] and [`LATCH`] are `lp_ws281x`'s
//! `ChannelTiming::WS2812` — 400/850 ns and 800/450 ns with a 300 µs latch —
//! converted to ticks of the [`CLOCK_HZ`] channel clock the C6 backend runs
//! the RMT at. [`HIGH_THRESHOLD`] is the midpoint of the two high times,
//! which is the decision a receiver has to make and the only one.
//! [`IDLE_THRES`] is the largest value `ch_rx_conf0.idle_thres` holds, chosen
//! because it must exceed the latch: a threshold below it would end the
//! reception in the middle of the inter-frame gap rather than after it.

use core::fmt;

use crate::checks::rmt_chase::fnv1a;

/// The line that says the payload finished.
pub const DONE_MARKER: &str = "[rmt-rx] === DONE ===";

/// The channel clock both sides run at (`c6_rmt::shared_driver::RMT_CLOCK`),
/// so a duration in ticks is a duration in 12.5 ns steps.
pub const CLOCK_HZ: u32 = 80_000_000;

/// Nanoseconds as ticks of [`CLOCK_HZ`], floored — the same conversion
/// `lp_ws281x` does when it builds a pulse code.
pub const fn ticks(ns: u32) -> u32 {
    (ns * (CLOCK_HZ / 1_000_000)) / 1_000
}

/// `ChannelTiming::WS2812`: a zero is 400 ns high then 850 ns low.
pub const T0H: u32 = ticks(400);
pub const T0L: u32 = ticks(850);
/// …and a one is 800 ns high then 450 ns low.
pub const T1H: u32 = ticks(800);
pub const T1L: u32 = ticks(450);
/// The inter-frame latch, 300 µs low.
pub const LATCH: u32 = ticks(300_000);

/// Where a receiver draws the line between a zero and a one: halfway between
/// the two high times, which is the only decision the decode makes.
pub const HIGH_THRESHOLD: u32 = (T0H + T1H) / 2;

/// The receiver's idle threshold, in ticks — the largest the register holds
/// (`ch_rx_conf0.idle_thres` is 15 bits, and esp-hal publishes the same
/// number as `MAX_RX_IDLE_THRESHOLD`).
///
/// It must be **longer than [`LATCH`]**, or the reception would end inside
/// the latch instead of after the frame; 32,767 ticks is 409 µs against the
/// latch's 300 µs.
pub const IDLE_THRES: u16 = 0x7fff;

/// LEDs in this payload's frame.
///
/// Smaller than the chase's 256 on purpose: every bit is one RMT word both
/// ways, so a 64-LED frame is 1,536 words through a 48-word window — 64
/// wraps of the receiver's RAM per frame, which is the machinery this
/// payload exists to exercise — while a 256-LED one would be four times the
/// guest time for the same answer, and M5's differential compares the words
/// one by one.
pub const LEDS: usize = 64;

/// Frames the payload sends before the done marker.
pub const FRAMES: usize = 32;

/// Bits on the wire for one frame of [`LEDS`] pixels.
pub const fn frame_bits(leds: usize) -> usize {
    leds * 24
}

/// Pulse codes one frame needs: one per bit, plus the latch word that ends
/// the transmission.
pub const fn frame_codes(leds: usize) -> usize {
    frame_bits(leds) + 1
}

/// One 32-bit RMT word from two (level, duration) pairs.
///
/// The encoding both engines use, from the PAC's bit map: bits 0:14 the first
/// duration, bit 15 its level, bits 16:30 the second duration, bit 31 its
/// level.
pub const fn pulse_code(l1: bool, d1: u32, l2: bool, d2: u32) -> u32 {
    (d1 & 0x7fff) | ((l1 as u32) << 15) | ((d2 & 0x7fff) << 16) | ((l2 as u32) << 31)
}

/// The first pair of a word: `(level, duration)`.
pub const fn first_half(word: u32) -> (bool, u32) {
    (word & (1 << 15) != 0, word & 0x7fff)
}

/// The second pair of a word.
pub const fn second_half(word: u32) -> (bool, u32) {
    (word & (1 << 31) != 0, (word >> 16) & 0x7fff)
}

/// Encode `bytes` into WS2812 pulse codes: one word per bit, most
/// significant bit first, then one latch word that also ends the
/// transmission.
///
/// Returns the number of codes written, or `None` when `out` is too short.
/// The latch word's second duration is zero, which is what esp-hal's
/// `PulseCode::is_end_marker` and the transmitter's own end rule both read as
/// "this word is the last one".
pub fn encode_frame(bytes: &[u8], out: &mut [u32]) -> Option<usize> {
    let needed = bytes.len() * 8 + 1;
    if out.len() < needed {
        return None;
    }
    let mut at = 0;
    for byte in bytes {
        for bit in (0..8).rev() {
            out[at] = if byte & (1 << bit) != 0 {
                pulse_code(true, T1H, false, T1L)
            } else {
                pulse_code(true, T0H, false, T0L)
            };
            at += 1;
        }
    }
    out[at] = pulse_code(false, LATCH, false, 0);
    Some(at + 1)
}

/// Why a received frame could not be read back as bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Fewer words came back than the frame has bits.
    Short { words: usize, wanted: usize },
    /// A word's first half was not a high pulse — the reader is out of phase
    /// with the wire, which is the failure a receiver that dropped or
    /// invented an edge produces.
    NotHighFirst { word: usize },
    /// A word's first half had no duration at all: an end marker where a bit
    /// was expected.
    EndMarker { word: usize },
    /// `out` is too short for the bits asked for.
    OutputTooShort,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Short { words, wanted } => {
                write!(f, "short: {words} words, wanted {wanted}")
            }
            DecodeError::NotHighFirst { word } => write!(f, "word {word} does not start high"),
            DecodeError::EndMarker { word } => write!(f, "end marker at word {word}"),
            DecodeError::OutputTooShort => write!(f, "output too short"),
        }
    }
}

/// Decode `words` — as a receiver wrote them — back into `out`, most
/// significant bit first.
///
/// One word is one bit: the **first** half is the bit's high pulse and it is
/// a one when that pulse is at least [`HIGH_THRESHOLD`] ticks long. The
/// second half is the low that follows, and the decode deliberately does not
/// look at it: on the frame's last bit the low run is not the bit's low at
/// all but the latch and the trailing idle, run together into one measured
/// run by a receiver that only sees level changes. A decoder that compared
/// the two halves would read that last bit wrong every time and pass every
/// test written on a frame's *interior*.
pub fn decode_frame(words: &[u32], out: &mut [u8]) -> Result<usize, DecodeError> {
    let bits = out.len() * 8;
    if words.len() < bits {
        return Err(DecodeError::Short {
            words: words.len(),
            wanted: bits,
        });
    }
    if bits == 0 {
        return Err(DecodeError::OutputTooShort);
    }
    for byte in out.iter_mut() {
        *byte = 0;
    }
    for (index, word) in words[..bits].iter().enumerate() {
        let (level, duration) = first_half(*word);
        if duration == 0 {
            return Err(DecodeError::EndMarker { word: index });
        }
        if !level {
            return Err(DecodeError::NotHighFirst { word: index });
        }
        if duration >= HIGH_THRESHOLD {
            out[index / 8] |= 1 << (7 - (index % 8));
        }
    }
    Ok(out.len())
}

/// One frame's record: what the **receiver** read off the pad.
///
/// `crc` is [`fnv1a`] over the decoded bytes, so it is directly comparable
/// with the `rmt-frame` record's checksum of the bytes the frame was built
/// from. `words` is how many RMT words the driver read out of the channel's
/// RAM, which is one per bit plus whatever the receiver wrote to close the
/// reception — reported, never compared against a constant, because it is
/// the one field a different receiver could legitimately disagree on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxRecord {
    pub n: usize,
    pub words: usize,
    pub crc: u32,
}

impl RxRecord {
    /// The record for frame `k`, from the words the receiver produced and
    /// the bytes they decoded to.
    pub fn of(k: usize, words: usize, decoded: &[u8]) -> Self {
        Self {
            n: k,
            words,
            crc: fnv1a(decoded),
        }
    }
}

impl fmt::Display for RxRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            r#"{{"kind":"rmt-rx","n":{},"words":{},"crc":"0x{:08x}"}}"#,
            self.n, self.words, self.crc
        )
    }
}

/// Write one record through any [`fmt::Write`] sink, prefix and newline
/// included.
pub fn write_rx_record<W: fmt::Write>(w: &mut W, record: &RxRecord) -> fmt::Result {
    writeln!(w, "{}{record}", crate::FW_CHECK_JSON_PREFIX)
}

/// The setup line, so a reader of the transcript knows which pads carried the
/// frame without going to the registry for it.
pub fn write_setup<W: fmt::Write>(w: &mut W, tx_gpio: u8, rx_gpio: u8) -> fmt::Result {
    writeln!(
        w,
        "[rmt-rx] tx=gpio{tx_gpio} rx=gpio{rx_gpio} leds={LEDS} frames={FRAMES} \
         idle_thres={IDLE_THRES} filter=off"
    )
}

/// Write the done marker.
pub fn write_done<W: fmt::Write>(w: &mut W) -> fmt::Result {
    writeln!(w, "{DONE_MARKER}")
}

/// A frame that could not be decoded still gets a line, so a run that fails
/// is a transcript rather than a silence.
pub fn write_decode_error<W: fmt::Write>(
    w: &mut W,
    n: usize,
    words: usize,
    error: DecodeError,
) -> fmt::Result {
    writeln!(
        w,
        "[rmt-rx] frame {n} ({words} words) did not decode: {error}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::rmt_chase::{chase_frame, frame_bytes};

    extern crate std;
    use std::string::String;
    use std::vec;

    /// The WS2812 numbers, spelled out, because everything else here is
    /// arithmetic over them.
    #[test]
    fn the_timings_are_the_ws2812_timings_at_eighty_megahertz() {
        assert_eq!((T0H, T0L), (32, 68));
        assert_eq!((T1H, T1L), (64, 36));
        assert_eq!(LATCH, 24_000);
        assert_eq!(HIGH_THRESHOLD, 48);
        assert!(
            u32::from(IDLE_THRES) > LATCH,
            "the reception must outlast the latch"
        );
    }

    /// The whole payload in one line: bytes out, bytes back, and the same
    /// checksum at both ends.
    #[test]
    fn a_frame_encodes_and_decodes_to_itself() {
        let mut frame = vec![0u8; frame_bytes(LEDS)];
        for k in [0usize, 1, 31] {
            chase_frame(k, LEDS, &mut frame);
            let mut codes = vec![0u32; frame_codes(LEDS)];
            let written = encode_frame(&frame, &mut codes).expect("room");
            assert_eq!(written, frame_bits(LEDS) + 1);
            let mut back = vec![0u8; frame_bytes(LEDS)];
            assert_eq!(decode_frame(&codes, &mut back).unwrap(), back.len());
            assert_eq!(back, frame);
            assert_eq!(fnv1a(&back), fnv1a(&frame));
        }
    }

    /// The last bit is the one a lazy decoder gets wrong: on the wire its low
    /// run is the latch and the idle, not 450 ns, so a decoder that compared
    /// the two halves of a word would read a one as a zero.
    #[test]
    fn the_last_bit_decodes_from_its_high_time_alone() {
        // One byte, 0x01: the final bit is a one, and the receiver measured
        // its low run as the whole trailing idle.
        let mut codes = [0u32; 8];
        for (i, code) in codes.iter_mut().enumerate() {
            *code = if i == 7 {
                pulse_code(true, T1H, false, IDLE_THRES as u32)
            } else {
                pulse_code(true, T0H, false, T0L)
            };
        }
        let mut out = [0u8; 1];
        decode_frame(&codes, &mut out).unwrap();
        assert_eq!(out[0], 0x01);
        // …and the same word read by "whichever half is longer" would be 0.
        let (_, high) = first_half(codes[7]);
        let (_, low) = second_half(codes[7]);
        assert!(low > high, "which is exactly the trap");
    }

    #[test]
    fn a_word_that_does_not_start_high_is_an_error_rather_than_a_wrong_byte() {
        let mut codes = [pulse_code(true, T0H, false, T0L); 8];
        codes[3] = pulse_code(false, T0L, true, T0H);
        let mut out = [0u8; 1];
        assert_eq!(
            decode_frame(&codes, &mut out),
            Err(DecodeError::NotHighFirst { word: 3 })
        );
        codes[3] = 0;
        assert_eq!(
            decode_frame(&codes, &mut out),
            Err(DecodeError::EndMarker { word: 3 })
        );
    }

    #[test]
    fn a_short_reception_is_an_error_and_names_both_counts() {
        let codes = [pulse_code(true, T0H, false, T0L); 4];
        let mut out = [0u8; 1];
        assert_eq!(
            decode_frame(&codes, &mut out),
            Err(DecodeError::Short {
                words: 4,
                wanted: 8
            })
        );
    }

    /// The encode is the transmitter's own word format: the emulator's TX
    /// engine takes these apart with the same bit positions.
    #[test]
    fn a_pulse_code_is_two_level_and_duration_pairs() {
        let code = pulse_code(true, 32, false, 68);
        assert_eq!(code, 0x0044_8020);
        assert_eq!(first_half(code), (true, 32));
        assert_eq!(second_half(code), (false, 68));
        // The end marker: a zero duration in either half.
        let latch = pulse_code(false, LATCH, false, 0);
        assert_eq!(second_half(latch), (false, 0));
    }

    #[test]
    fn the_record_renders_the_line_the_host_parses() {
        let record = RxRecord::of(7, 1537, b"abc");
        let mut out = String::new();
        write_rx_record(&mut out, &record).unwrap();
        assert_eq!(
            out,
            std::format!(
                "[fw-check-json] {{\"kind\":\"rmt-rx\",\"n\":7,\"words\":1537,\
                 \"crc\":\"0x{:08x}\"}}\n",
                fnv1a(b"abc")
            )
        );
    }

    #[test]
    fn the_done_marker_is_the_line_the_runner_stops_on() {
        let mut out = String::new();
        write_done(&mut out).unwrap();
        assert_eq!(out, "[rmt-rx] === DONE ===\n");
    }

    /// Two frames of the chase decode to different checksums, or the record
    /// would say nothing about which frame came back.
    #[test]
    fn each_frame_of_the_run_has_its_own_checksum() {
        let mut a = vec![0u8; frame_bytes(LEDS)];
        let mut b = vec![0u8; frame_bytes(LEDS)];
        chase_frame(0, LEDS, &mut a);
        chase_frame(1, LEDS, &mut b);
        let mut codes = vec![0u32; frame_codes(LEDS)];
        let mut back_a = vec![0u8; frame_bytes(LEDS)];
        let mut back_b = vec![0u8; frame_bytes(LEDS)];
        encode_frame(&a, &mut codes).unwrap();
        decode_frame(&codes, &mut back_a).unwrap();
        encode_frame(&b, &mut codes).unwrap();
        decode_frame(&codes, &mut back_b).unwrap();
        assert_ne!(
            RxRecord::of(0, 0, &back_a).crc,
            RxRecord::of(1, 0, &back_b).crc
        );
        // …and the run is long enough that the dot never wraps onto itself.
        assert!(FRAMES <= LEDS);
    }
}
