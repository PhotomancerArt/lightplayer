//! Per-fixture channel transform lookup table.
//!
//! Collapses the per-channel post-loop transform
//! `Q32 → ×brightness → to_u16_saturating → (optional gamma) → u16`
//! into a single 4096-entry lookup keyed by the top 12 bits of the
//! saturated accumulator. Rebuilt by `FixtureRuntime` whenever
//! `brightness` or `gamma_correction` changes.

use lps_q32::q32::{Q32, ToQ32};

use super::gamma::apply_gamma;

const BIN_COUNT: usize = 4096;

/// 12-bit-input lookup table for the per-channel post-loop transform.
///
/// Memory cost: 4096 * 2 bytes = 8 KB per fixture. Sheddable by
/// `FixtureRuntime::shed_optional_buffers`.
pub struct ChannelLut {
    out_u16: [u16; BIN_COUNT],
}

impl ChannelLut {
    /// Build a fresh LUT for the given brightness/gamma combination.
    ///
    /// Each bin's u16 output is computed by `channel_transform_reference`,
    /// so the LUT is bit-exact with the reference by construction.
    pub fn build(brightness: u8, gamma: bool) -> Self {
        let mut out_u16 = [0u16; BIN_COUNT];
        for bin in 0..BIN_COUNT {
            let q = bin_to_q32(bin);
            out_u16[bin] = channel_transform_reference(q, brightness, gamma);
        }
        Self { out_u16 }
    }

    /// Look up the post-loop transform for a Q32 channel value.
    ///
    /// Saturates inputs at or above `Q32::ONE` to the same bin as
    /// `Q32::ONE - 1` (mirroring `to_u16_saturating`'s saturation).
    #[inline]
    pub fn lookup(&self, ch_q32: Q32) -> u16 {
        // ch_q32.0 may be negative or >= ONE; saturate to [0, ONE - 1].
        let raw = ch_q32.0;
        let sat: u32 = if raw < 0 {
            0
        } else {
            (raw as u32).min(Q32::ONE.0 as u32 - 1)
        };
        let idx = (sat >> 4) as usize; // 0..=4095
        self.out_u16[idx]
    }
}

/// Map a 12-bit bin index to the Q32 value at that bin's lower edge.
#[inline]
fn bin_to_q32(bin: usize) -> Q32 {
    // bin is 0..4096, so (bin << 4) is 0..65536. Clamp to ONE - 1 so
    // bin=4095 → Q32(65520), and the reference function never sees a
    // value at or above ONE here. Saturation is handled in `lookup`.
    let raw = ((bin as i32) << 4).min(Q32::ONE.0 - 1);
    Q32(raw)
}

/// Slow-path reference: the EXACT transform that the LUT collapses.
/// Used by `ChannelLut::build` (single source of truth) and by the
/// exhaustive sweep test.
fn channel_transform_reference(ch_q32: Q32, brightness: u8, gamma: bool) -> u16 {
    let brightness_q = brightness.to_q32() / 255.to_q32();
    let r_q = ch_q32 * brightness_q;
    let mut r = r_q.to_u16_saturating();
    if gamma {
        r = apply_gamma((r >> 8) as u8).to_q32().to_u16_saturating();
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_matches_reference_exhaustive() {
        for &brightness in &[0u8, 1, 8, 32, 64, 127, 200, 255] {
            for &gamma in &[false, true] {
                let lut = ChannelLut::build(brightness, gamma);
                for bin in 0..BIN_COUNT {
                    let q = bin_to_q32(bin);
                    let expected = channel_transform_reference(q, brightness, gamma);
                    assert_eq!(
                        lut.out_u16[bin], expected,
                        "bin={bin} brightness={brightness} gamma={gamma}"
                    );
                }
            }
        }
    }

    #[test]
    fn lookup_saturates_above_one() {
        let lut = ChannelLut::build(255, false);
        let last_bin = lut.out_u16[BIN_COUNT - 1];
        // Inputs at or above ONE collapse to the same bin as ONE - 1.
        assert_eq!(lut.lookup(Q32::ONE), last_bin);
        assert_eq!(lut.lookup(Q32(Q32::ONE.0 + 1)), last_bin);
        assert_eq!(lut.lookup(Q32(i32::MAX)), last_bin);
    }

    #[test]
    fn lookup_saturates_below_zero() {
        let lut = ChannelLut::build(255, false);
        assert_eq!(lut.lookup(Q32(-1)), lut.out_u16[0]);
        assert_eq!(lut.lookup(Q32(i32::MIN)), lut.out_u16[0]);
    }

    #[test]
    fn brightness_zero_yields_all_zeros() {
        for &gamma in &[false, true] {
            let lut = ChannelLut::build(0, gamma);
            for (bin, &v) in lut.out_u16.iter().enumerate() {
                assert_eq!(v, 0, "non-zero output at bin={bin} gamma={gamma}");
            }
        }
    }

    #[test]
    fn lookup_matches_reference_for_arbitrary_inputs() {
        let lut = ChannelLut::build(64, true);
        for &raw in &[0i32, 1, 1024, 16_384, 32_768, 49_152, 65_535, 65_519] {
            let q = Q32(raw);
            let from_lut = lut.lookup(q);
            let from_ref = channel_transform_reference(
                Q32(raw.min(Q32::ONE.0 - 1)),
                64,
                true,
            );
            assert_eq!(from_lut, from_ref, "raw={raw}");
        }
    }
}
