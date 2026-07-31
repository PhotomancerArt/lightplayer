//! Helpers shared by the integration tests.

use lp_ws281x::{ChannelTiming, PulseCodes};

/// The exact word stream a correct transmission of `frame` must produce:
/// every bit, MSB first, in the configured byte order, then the latch.
///
/// The terminating STOP word is deliberately absent — the transmitter stops
/// *on* it and never puts it on the wire.
pub fn expected_words(frame: &[u8], timing: &ChannelTiming) -> Vec<u32> {
    let codes = PulseCodes::at_default_clock(timing).expect("timing must be encodable");
    let mut out = Vec::new();
    for pixel in frame.chunks_exact(3) {
        for slot in 0..3 {
            let byte = pixel[timing.color_order.source_index(slot)];
            for bit in 0..8 {
                out.push(codes.bit(byte & (0x80 >> bit) != 0));
            }
        }
    }
    out.push(codes.latch);
    out
}

/// A deterministic frame of `pixels` pixels with no repeating byte pattern, so
/// a duplicated or dropped half shows up as a mismatch rather than aliasing
/// into a plausible stream.
pub fn ramp_frame(pixels: usize) -> Vec<u8> {
    (0..pixels * 3)
        .map(|i| (i.wrapping_mul(37).wrapping_add(11) % 251) as u8)
        .collect()
}
