//! The `rmt-chase` payload: a white dot walking a 256-LED strip, and one
//! record per frame saying what the guest *believes* it sent.
//!
//! The harness this replaces (`fw-esp32c6/src/tests/test_rmt.rs`) has driven
//! the RMT since long before there was a validation system: it built a frame,
//! handed it to `lp-ws281x`, waited, slept 10 ms, and said nothing at all
//! about any of it. That made it a fine smoke test on a desk with a strip
//! plugged in and useless as evidence — "the LEDs looked right" is not a
//! transcript.
//!
//! So the portable half moves here. What the payload adds is one line per
//! frame:
//!
//! ```text
//! [fw-check-json] {"kind":"rmt-frame","n":0,"leds":256,"lit":1,"crc":"0x2f8a1b3c"}
//! ```
//!
//! and that line is the whole point of the phase. The emulator decodes the
//! same frame off **the pad** (M5 P2's fabric and WS281x decoder) and computes
//! the same checksum from the waveform; the gate is that the two agree, frame
//! by frame. One number is what the driver thinks it wrote, the other is what
//! a logic analyser would have read, and until this payload existed there was
//! no way to compare them.
//!
//! Everything here is arithmetic over bytes: the pattern, the checksum, the
//! record's JSON, the done marker. `no_std`, `alloc`-free, host-tested. What
//! stays in `fw-esp32c6` is board init, `Rmt`, `LedChannel` and
//! `embassy_time` — the parts that need a chip.
//!
//! # The checksum is transcribed, in three places now
//!
//! [`fnv1a`] is FNV-1a 32-bit with the standard offset basis and prime, and it
//! is the *fourth* copy of those two constants in this repository:
//! `fw-esp32s3/src/output/rmt/frame_dump.rs`, its `fw-esp32v3` port,
//! `lp-app/lpa-server/tests/shader_oracle_frame.rs`, and this. They are
//! deliberately not shared: the S3 pair are firmware for other chips, the
//! oracle test is a product crate, and this crate must not depend on either
//! (nor may `lp-emu/`, which transcribes them a fifth time for the pin-side
//! gate — see `lp-emu/esp/lp-emu-esp32c6/tests/rmt_chase_replay.rs`). Four
//! transcriptions of two hex constants, each with a unit test that pins a
//! known vector, is cheaper than a dependency edge across three fences.
//!
//! # The colour order, and why this pattern does not care
//!
//! `LedChannel::start_transmission` swaps the caller's RGB into GRB itself,
//! and then `lp-ws281x` permutes again by [`ColorOrder::Grb`] at encode time —
//! so the bytes on the wire are the caller's RGB, unswapped. That double swap
//! is a **finding, not a fix** (DD34 d): correcting it here would change what
//! every existing harness puts on a strip, in a phase whose job is to measure.
//! This payload is immune to it on purpose — its only lit pixel is white
//! (`[10, 10, 10]`) and every other pixel is black, and both are invariant
//! under any permutation of the three channels. The chase therefore says the
//! same thing about the wire whichever way the question is settled later.

use core::fmt;

/// The line that says the payload finished.
pub const DONE_MARKER: &str = "[rmt-chase] === DONE ===";

/// LEDs on the harness strip, unchanged from the old `test_rmt`.
pub const LEDS: usize = 256;

/// Full passes of the dot down the strip.
///
/// Three, and the number is load-bearing: at 256 frames a chase and a
/// measured 18.10 ms a frame (7.98 ms of transmission plus the harness's
/// 10 ms sleep, M5 P2) that is 768 frames and ≈ 13.9 s — long enough to cross
/// the `ws281x_telemetry` module's 10-second reporting period, so a transcript
/// holds exactly one `[WS281X]` line. Two chases would hold none.
pub const CHASES: usize = 3;

/// Frames the payload sends before the done marker.
pub const FRAMES: usize = LEDS * CHASES;

/// The lit pixel. White, so the pattern is colour-order invariant.
pub const DOT: [u8; 3] = [10, 10, 10];

/// Bytes one frame of `leds` pixels occupies.
pub const fn frame_bytes(leds: usize) -> usize {
    leds * 3
}

/// Write frame `k` of the chase into `out`: pixel `k % leds` is [`DOT`],
/// every other pixel is black.
///
/// Returns the number of lit pixels — always 1 for a non-empty strip, and the
/// record carries it so a reader of the transcript can see the claim rather
/// than infer it.
pub fn chase_frame(k: usize, leds: usize, out: &mut [u8]) -> usize {
    let bytes = frame_bytes(leds).min(out.len());
    out[..bytes].fill(0);
    if leds == 0 {
        return 0;
    }
    let lit = k % leds;
    let at = lit * 3;
    if at + 3 > bytes {
        return 0;
    }
    out[at..at + 3].copy_from_slice(&DOT);
    1
}

/// FNV-1a, 32-bit. See the module docs for why it is transcribed.
pub fn fnv1a(data: &[u8]) -> u32 {
    const OFFSET: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let mut hash = OFFSET;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// One frame's record: what the guest handed the driver.
///
/// `crc` is [`fnv1a`] over the frame **as the caller built it** — the RGB
/// bytes, before `LedChannel`'s swap and before `lp-ws281x`'s permutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRecord {
    pub n: usize,
    pub leds: usize,
    pub lit: usize,
    pub crc: u32,
}

impl FrameRecord {
    /// The record for frame `k` of `frame`, which must already hold
    /// [`chase_frame`]'s output.
    pub fn of(k: usize, leds: usize, lit: usize, frame: &[u8]) -> Self {
        Self {
            n: k,
            leds,
            lit,
            crc: fnv1a(&frame[..frame_bytes(leds).min(frame.len())]),
        }
    }
}

impl fmt::Display for FrameRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            r#"{{"kind":"rmt-frame","n":{},"leds":{},"lit":{},"crc":"0x{:08x}"}}"#,
            self.n, self.leds, self.lit, self.crc
        )
    }
}

/// Write one record through any [`fmt::Write`] sink, prefix and newline
/// included.
///
/// Sink-agnostic for the same reason [`crate::write_header`] is: this crate
/// cannot prove a `log` sink is installed, and a C6 harness prints through
/// `esp_println::Printer` so that its records and its header share one path.
pub fn write_frame_record<W: fmt::Write>(w: &mut W, record: &FrameRecord) -> fmt::Result {
    writeln!(w, "{}{record}", crate::FW_CHECK_JSON_PREFIX)
}

/// Write the done marker.
pub fn write_done<W: fmt::Write>(w: &mut W) -> fmt::Result {
    writeln!(w, "{DONE_MARKER}")
}

/// Emit one record through `log`, for a harness that has a logger and would
/// rather use it. The C6 harness does not — see [`write_frame_record`].
pub fn emit_frame_record(record: &FrameRecord) {
    crate::emit_record_json(format_args!("{record}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;
    use std::string::String;

    #[test]
    fn the_dot_walks_and_wraps() {
        let mut frame = [0u8; frame_bytes(8)];
        for k in [0usize, 1, 7, 8, 9] {
            let lit = chase_frame(k, 8, &mut frame);
            assert_eq!(lit, 1);
            let at = (k % 8) * 3;
            assert_eq!(&frame[at..at + 3], &DOT);
            let dark = frame
                .iter()
                .enumerate()
                .filter(|(i, _)| !(at..at + 3).contains(i))
                .all(|(_, b)| *b == 0);
            assert!(dark, "frame {k} lit more than one pixel: {frame:?}");
        }
    }

    /// The pattern is the same bytes under any channel permutation, which is
    /// what makes the payload immune to the double colour swap (module docs).
    #[test]
    fn every_pixel_is_grey_so_the_frame_is_order_invariant() {
        let mut frame = [0u8; frame_bytes(16)];
        chase_frame(5, 16, &mut frame);
        for pixel in frame.chunks_exact(3) {
            assert!(
                pixel[0] == pixel[1] && pixel[1] == pixel[2],
                "pixel {pixel:?} is not grey"
            );
        }
    }

    /// The published FNV-1a 32-bit vectors. If this fails, four other copies
    /// of these constants disagree with this one.
    #[test]
    fn fnv1a_matches_the_published_vectors() {
        assert_eq!(fnv1a(b""), 0x811c_9dc5);
        assert_eq!(fnv1a(b"a"), 0xe40c_292c);
        assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968);
    }

    #[test]
    fn the_record_renders_the_line_the_host_parses() {
        let mut frame = [0u8; frame_bytes(4)];
        let lit = chase_frame(2, 4, &mut frame);
        let record = FrameRecord::of(2, 4, lit, &frame);
        let mut out = String::new();
        write_frame_record(&mut out, &record).unwrap();
        assert_eq!(
            out,
            std::format!(
                "[fw-check-json] {{\"kind\":\"rmt-frame\",\"n\":2,\"leds\":4,\"lit\":1,\
                 \"crc\":\"0x{:08x}\"}}\n",
                fnv1a(&frame)
            )
        );
    }

    /// Two different frames of the chase have different checksums, or the
    /// record would say nothing about which frame went out.
    #[test]
    fn each_frame_of_the_chase_has_its_own_checksum() {
        let mut a = [0u8; frame_bytes(32)];
        let mut b = [0u8; frame_bytes(32)];
        chase_frame(0, 32, &mut a);
        chase_frame(1, 32, &mut b);
        assert_ne!(fnv1a(&a), fnv1a(&b));
        // …and the chase wraps back onto itself exactly.
        let mut wrapped = [0u8; frame_bytes(32)];
        chase_frame(32, 32, &mut wrapped);
        assert_eq!(a, wrapped);
    }

    #[test]
    fn the_done_marker_is_the_line_the_runner_stops_on() {
        let mut out = String::new();
        write_done(&mut out).unwrap();
        assert_eq!(out, "[rmt-chase] === DONE ===\n");
        assert_eq!(FRAMES, 768);
    }
}
