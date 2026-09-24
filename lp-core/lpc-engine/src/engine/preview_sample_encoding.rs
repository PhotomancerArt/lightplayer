//! Transport precision for the samples a preview probe ships.
//!
//! The engine renders and publishes control samples at 16 bits (`unorm16`,
//! little-endian, LINEAR). A client drawing them on a screen needs 8, so the
//! control-product and output-frame probes let it ask for an 8-bit format and
//! halve the pixel bytes on the wire:
//!
//! - [`WireChannelSampleFormat::Srgb8`], Studio's preview default: the
//!   correctly rounded sRGB display code, `lpc_wire::linear16_to_srgb8` (its
//!   module docs state the rule; integer-only, 766 bytes of tables). The
//!   codes are spent where a screen tells levels apart, so a dim picture
//!   keeps its darks.
//! - [`WireChannelSampleFormat::U8`]: linear, rounded to nearest (below).
//!
//! # The linear `U8` rounding rule
//!
//! `u8 = round(u16 / 257)`, computed exactly in integers as
//! `(v · 255 + 32767) / 65535`. 257 is the ratio between the two full scales
//! (65535 = 255 · 257), so both ends are fixed points — 0 → 0 and
//! 65535 → 255 — and every 8-bit level `k` is the nearest one to the 16-bit
//! values around `k · 257`. There are no ties to break: `v / 257` is never
//! exactly half-way for an integer `v`.
//!
//! The samples stay what they were (an output frame's are post-finalize);
//! 8-bit is only how precisely they travel. A consumer widening `U8` back to
//! 16 bits multiplies by 257; one decoding `Srgb8` uses
//! `lpc_wire::srgb8_to_linear16`.

use alloc::vec::Vec;

use lpc_wire::{WireChannelSampleFormat, linear16_to_srgb8};

/// Round one 16-bit sample to the nearest 8-bit level (see the module docs).
#[must_use]
pub(crate) fn unorm16_to_unorm8(value: u16) -> u8 {
    ((u32::from(value) * 255 + 32_767) / 65_535) as u8
}

/// Encode rendered 16-bit samples in the requested transport format.
#[must_use]
pub(crate) fn encode_unorm16_samples(samples: &[u16], format: WireChannelSampleFormat) -> Vec<u8> {
    match format {
        WireChannelSampleFormat::U16 => {
            let mut bytes = Vec::with_capacity(samples.len() * 2);
            for sample in samples {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            bytes
        }
        WireChannelSampleFormat::U8 => samples.iter().map(|&v| unorm16_to_unorm8(v)).collect(),
        WireChannelSampleFormat::Srgb8 => samples.iter().map(|&v| linear16_to_srgb8(v)).collect(),
    }
}

/// Encode an already-serialized little-endian 16-bit buffer (a published
/// output frame) in the requested transport format. `U16` is verbatim; a
/// trailing odd byte, which no published buffer carries, is dropped by the
/// 8-bit formats.
#[must_use]
pub(crate) fn encode_unorm16_le_bytes(bytes: &[u8], format: WireChannelSampleFormat) -> Vec<u8> {
    let narrow = |encode: fn(u16) -> u8| -> Vec<u8> {
        bytes
            .chunks_exact(2)
            .map(|pair| encode(u16::from_le_bytes([pair[0], pair[1]])))
            .collect()
    };
    match format {
        WireChannelSampleFormat::U16 => bytes.to_vec(),
        WireChannelSampleFormat::U8 => narrow(unorm16_to_unorm8),
        WireChannelSampleFormat::Srgb8 => narrow(linear16_to_srgb8),
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    /// The edges the rule is pinned by: both full scales are fixed points,
    /// and 255 as a 16-bit value (just under one 8-bit step, 257) rounds UP
    /// to the first level rather than truncating to black.
    #[test]
    fn rounding_edges_hold() {
        assert_eq!(unorm16_to_unorm8(0), 0);
        assert_eq!(unorm16_to_unorm8(255), 1);
        assert_eq!(unorm16_to_unorm8(65_535), 255);
    }

    /// Round-to-nearest around one step: 128 / 257 is below half a level,
    /// 129 / 257 above it; and every level `k · 257` maps back to `k`.
    #[test]
    fn rounding_is_to_nearest_and_levels_round_trip() {
        assert_eq!(unorm16_to_unorm8(128), 0);
        assert_eq!(unorm16_to_unorm8(129), 1);
        assert_eq!(unorm16_to_unorm8(65_535 - 128), 255);
        assert_eq!(unorm16_to_unorm8(65_535 - 129), 254);
        for level in 0..=255_u16 {
            assert_eq!(unorm16_to_unorm8(level * 257), level as u8);
        }
    }

    /// The rule agrees with a float `round(v / 257)` for every input.
    #[test]
    fn integer_rule_matches_float_rounding_for_every_value() {
        for value in 0..=u16::MAX {
            let expected = (f64::from(value) / 257.0).round() as u8;
            assert_eq!(unorm16_to_unorm8(value), expected, "value {value}");
        }
    }

    #[test]
    fn encoders_agree_between_samples_and_published_bytes() {
        let samples = [0_u16, 255, 32_896, 65_535];
        let le = encode_unorm16_samples(&samples, WireChannelSampleFormat::U16);
        assert_eq!(le, vec![0, 0, 255, 0, 128, 128, 255, 255]);
        assert_eq!(
            encode_unorm16_le_bytes(&le, WireChannelSampleFormat::U16),
            le,
            "U16 is verbatim"
        );
        let u8s = encode_unorm16_samples(&samples, WireChannelSampleFormat::U8);
        assert_eq!(u8s, vec![0, 1, 128, 255]);
        assert_eq!(
            encode_unorm16_le_bytes(&le, WireChannelSampleFormat::U8),
            u8s
        );
        let srgb = encode_unorm16_samples(&samples, WireChannelSampleFormat::Srgb8);
        // 255/65535 linear is sRGB code 13; half scale is code 188.
        assert_eq!(srgb, vec![0, 13, 188, 255]);
        assert_eq!(
            encode_unorm16_le_bytes(&le, WireChannelSampleFormat::Srgb8),
            srgb
        );
    }
}
