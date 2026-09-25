//! The wire's sRGB-encoded 8-bit sample: linear unorm16 in, display code out.
//!
//! [`WireChannelSampleFormat::Srgb8`](super::WireChannelSampleFormat::Srgb8)
//! and [`WireTextureFormat::Srgb8`](super::WireTextureFormat::Srgb8) carry
//! samples through the sRGB transfer function, so the 256 codes are spent
//! where the eye tells levels apart — in the darks. Linear 8-bit spends 13 of
//! them below the first sRGB step above black; this encoding spends one.
//!
//! # The rounding rule
//!
//! For a linear sample `v` (unorm16), the code is the correctly rounded
//! display value:
//!
//! ```text
//! x = v / 65535
//! s = 12.92 · x                     if x ≤ 0.0031308
//!     1.055 · x^(1/2.4) − 0.055     otherwise
//! code = floor(255 · s + 0.5)
//! ```
//!
//! evaluated exactly (the nearest any of the 65,536 inputs comes to a
//! half-way point is 1.4e-6 of a code, so there are no ties to break). 0 → 0
//! and 65535 → 255. The encoder uses no floating point: the code is the
//! number of step boundaries at or below `v`
//! (`SRGB8_STEP_THRESHOLDS`), found by starting from the code at `v`'s
//! 256-wide bucket (`SRGB8_AT_COARSE_LINEAR`) and stepping up at most 13
//! times. 766 bytes of tables, on a chip with no FPU. The host test checks
//! it against the float transfer for every input.
//!
//! Decoding ([`srgb8_to_linear16`]) returns the linear value nearest the
//! exact inverse of `code / 255`, which re-encodes to the same code.

/// Encode one linear unorm16 sample as its correctly rounded sRGB8 code
/// (see the module docs).
#[must_use]
pub fn linear16_to_srgb8(value: u16) -> u8 {
    let mut code = SRGB8_AT_COARSE_LINEAR[usize::from(value >> 8)];
    while let Some(&threshold) = SRGB8_STEP_THRESHOLDS.get(usize::from(code))
        && threshold <= value
    {
        code += 1;
    }
    code
}

/// Decode one sRGB8 code to linear unorm16: the value nearest the exact
/// inverse transfer, so `linear16_to_srgb8(srgb8_to_linear16(c)) == c`.
#[must_use]
pub fn srgb8_to_linear16(code: u8) -> u16 {
    SRGB8_TO_LINEAR16[usize::from(code)]
}

/// `SRGB8_STEP_THRESHOLDS[k]` is the smallest linear unorm16 value whose
/// code is `k + 1`: the 255 step boundaries of the sRGB8 staircase.
const SRGB8_STEP_THRESHOLDS: [u16; 255] = [
    10, 30, 50, 70, 90, 110, 130, 150, 170, 189, 209, 230, 253, 276, 301, 327, 354, 382, 412, 443,
    475, 509, 544, 580, 618, 657, 698, 740, 783, 828, 875, 923, 972, 1023, 1075, 1129, 1185, 1242,
    1300, 1360, 1422, 1486, 1551, 1617, 1685, 1755, 1827, 1900, 1975, 2052, 2130, 2210, 2292, 2376,
    2461, 2548, 2637, 2727, 2820, 2914, 3010, 3108, 3208, 3309, 3412, 3518, 3625, 3734, 3844, 3957,
    4072, 4188, 4307, 4427, 4550, 4674, 4800, 4928, 5059, 5191, 5325, 5461, 5599, 5740, 5882, 6026,
    6173, 6321, 6471, 6624, 6778, 6935, 7094, 7255, 7418, 7583, 7750, 7919, 8091, 8265, 8440, 8618,
    8798, 8981, 9165, 9352, 9541, 9732, 9925, 10121, 10318, 10518, 10720, 10925, 11132, 11341,
    11552, 11765, 11981, 12199, 12420, 12643, 12868, 13095, 13325, 13557, 13791, 14028, 14267,
    14508, 14752, 14998, 15247, 15498, 15751, 16007, 16265, 16525, 16788, 17054, 17321, 17592,
    17864, 18139, 18417, 18697, 18980, 19264, 19552, 19842, 20134, 20429, 20727, 21027, 21329,
    21634, 21942, 22252, 22564, 22880, 23197, 23518, 23840, 24166, 24494, 24824, 25158, 25493,
    25832, 26173, 26516, 26862, 27211, 27563, 27917, 28273, 28633, 28995, 29359, 29727, 30097,
    30469, 30845, 31223, 31603, 31987, 32373, 32762, 33153, 33547, 33944, 34344, 34747, 35152,
    35560, 35970, 36384, 36800, 37219, 37640, 38065, 38492, 38922, 39355, 39790, 40229, 40670,
    41114, 41561, 42011, 42463, 42918, 43377, 43838, 44301, 44768, 45238, 45710, 46185, 46663,
    47144, 47628, 48115, 48605, 49097, 49593, 50091, 50592, 51096, 51604, 52114, 52627, 53142,
    53661, 54183, 54708, 55235, 55766, 56300, 56836, 57376, 57918, 58464, 59012, 59564, 60118,
    60675, 61236, 61799, 62366, 62935, 63508, 64083, 64662, 65244,
];

/// The code at the start of each 256-wide linear bucket (`v >> 8`): where
/// [`linear16_to_srgb8`] starts counting boundaries.
const SRGB8_AT_COARSE_LINEAR: [u8; 256] = [
    0, 13, 22, 28, 34, 38, 42, 46, 49, 53, 56, 58, 61, 64, 66, 68, 71, 73, 75, 77, 79, 81, 83, 85,
    86, 88, 90, 91, 93, 95, 96, 98, 99, 101, 102, 103, 105, 106, 107, 109, 110, 111, 113, 114, 115,
    116, 118, 119, 120, 121, 122, 123, 124, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136,
    137, 138, 139, 140, 141, 142, 143, 144, 145, 145, 146, 147, 148, 149, 150, 151, 152, 153, 153,
    154, 155, 156, 157, 158, 158, 159, 160, 161, 162, 162, 163, 164, 165, 166, 166, 167, 168, 169,
    169, 170, 171, 172, 172, 173, 174, 174, 175, 176, 177, 177, 178, 179, 179, 180, 181, 181, 182,
    183, 184, 184, 185, 186, 186, 187, 188, 188, 189, 189, 190, 191, 191, 192, 193, 193, 194, 195,
    195, 196, 196, 197, 198, 198, 199, 199, 200, 201, 201, 202, 202, 203, 204, 204, 205, 205, 206,
    207, 207, 208, 208, 209, 209, 210, 211, 211, 212, 212, 213, 213, 214, 214, 215, 216, 216, 217,
    217, 218, 218, 219, 219, 220, 220, 221, 221, 222, 223, 223, 224, 224, 225, 225, 226, 226, 227,
    227, 228, 228, 229, 229, 230, 230, 231, 231, 232, 232, 233, 233, 234, 234, 235, 235, 236, 236,
    237, 237, 238, 238, 239, 239, 239, 240, 240, 241, 241, 242, 242, 243, 243, 244, 244, 245, 245,
    246, 246, 246, 247, 247, 248, 248, 249, 249, 250, 250, 251, 251, 251, 252, 252, 253, 253, 254,
    254, 255,
];

/// Linear unorm16 for each code: `round(65535 · inverse_transfer(code / 255))`.
const SRGB8_TO_LINEAR16: [u16; 256] = [
    0, 20, 40, 60, 80, 99, 119, 139, 159, 179, 199, 219, 241, 264, 288, 313, 340, 367, 396, 427,
    458, 491, 526, 562, 599, 637, 677, 718, 761, 805, 851, 898, 947, 997, 1048, 1101, 1156, 1212,
    1270, 1330, 1391, 1453, 1517, 1583, 1651, 1720, 1790, 1863, 1937, 2013, 2090, 2170, 2250, 2333,
    2418, 2504, 2592, 2681, 2773, 2866, 2961, 3058, 3157, 3258, 3360, 3464, 3570, 3678, 3788, 3900,
    4014, 4129, 4247, 4366, 4488, 4611, 4736, 4864, 4993, 5124, 5257, 5392, 5530, 5669, 5810, 5953,
    6099, 6246, 6395, 6547, 6700, 6856, 7014, 7174, 7335, 7500, 7666, 7834, 8004, 8177, 8352, 8528,
    8708, 8889, 9072, 9258, 9445, 9635, 9828, 10022, 10219, 10417, 10619, 10822, 11028, 11235,
    11446, 11658, 11873, 12090, 12309, 12530, 12754, 12980, 13209, 13440, 13673, 13909, 14146,
    14387, 14629, 14874, 15122, 15371, 15623, 15878, 16135, 16394, 16656, 16920, 17187, 17456,
    17727, 18001, 18277, 18556, 18837, 19121, 19407, 19696, 19987, 20281, 20577, 20876, 21177,
    21481, 21787, 22096, 22407, 22721, 23038, 23357, 23678, 24002, 24329, 24658, 24990, 25325,
    25662, 26001, 26344, 26688, 27036, 27386, 27739, 28094, 28452, 28813, 29176, 29542, 29911,
    30282, 30656, 31033, 31412, 31794, 32179, 32567, 32957, 33350, 33745, 34143, 34544, 34948,
    35355, 35764, 36176, 36591, 37008, 37429, 37852, 38278, 38706, 39138, 39572, 40009, 40449,
    40891, 41337, 41785, 42236, 42690, 43147, 43606, 44069, 44534, 45002, 45473, 45947, 46423,
    46903, 47385, 47871, 48359, 48850, 49344, 49841, 50341, 50844, 51349, 51858, 52369, 52884,
    53401, 53921, 54445, 54971, 55500, 56032, 56567, 57105, 57646, 58190, 58737, 59287, 59840,
    60396, 60955, 61517, 62082, 62650, 63221, 63795, 64372, 64952, 65535,
];

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    /// Every input agrees with the float transfer (f64), which is the rule
    /// the module docs state.
    #[test]
    fn encoder_matches_the_float_transfer_for_every_input() {
        for value in 0..=u16::MAX {
            assert_eq!(
                linear16_to_srgb8(value),
                reference_encode(value),
                "value {value}"
            );
        }
    }

    #[test]
    fn full_scales_are_fixed_points() {
        assert_eq!(linear16_to_srgb8(0), 0);
        assert_eq!(linear16_to_srgb8(u16::MAX), 255);
        assert_eq!(srgb8_to_linear16(0), 0);
        assert_eq!(srgb8_to_linear16(255), u16::MAX);
    }

    /// Each step boundary, and one either side of it.
    #[test]
    fn every_step_boundary_is_exact() {
        for (index, &threshold) in SRGB8_STEP_THRESHOLDS.iter().enumerate() {
            let code = index as u8 + 1;
            assert_eq!(linear16_to_srgb8(threshold), code, "at {threshold}");
            assert_eq!(
                linear16_to_srgb8(threshold - 1),
                code - 1,
                "below {threshold}"
            );
            if let Some(above) = threshold.checked_add(1) {
                assert!(linear16_to_srgb8(above) >= code, "above {threshold}");
            }
        }
        // The darks the format exists for: the first steps above black.
        assert_eq!(&SRGB8_STEP_THRESHOLDS[..4], &[10, 30, 50, 70]);
    }

    #[test]
    fn encoder_is_monotonic_and_hits_every_code() {
        let mut previous = 0_u8;
        let mut seen = [false; 256];
        for value in 0..=u16::MAX {
            let code = linear16_to_srgb8(value);
            assert!(code >= previous, "not monotonic at {value}");
            assert!(code - previous <= 1, "skipped a code at {value}");
            previous = code;
            seen[usize::from(code)] = true;
        }
        assert!(seen.iter().all(|&hit| hit));
    }

    #[test]
    fn decode_round_trips_every_code_and_is_the_nearest_inverse() {
        for code in 0..=u8::MAX {
            let linear = srgb8_to_linear16(code);
            assert_eq!(linear16_to_srgb8(linear), code, "code {code}");
            let s = f64::from(code) / 255.0;
            let exact = if s <= 0.040_45 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            };
            assert_eq!(linear, (exact * 65_535.0).round() as u16, "code {code}");
        }
    }

    fn reference_encode(value: u16) -> u8 {
        let x = f64::from(value) / 65_535.0;
        let s = if x <= 0.003_130_8 {
            12.92 * x
        } else {
            1.055 * x.powf(1.0 / 2.4) - 0.055
        };
        (255.0 * s + 0.5).floor() as u8
    }
}
