//! Gamma correction for the 16-bit fixture pipeline
//!
//! Corrects for the non-linear brightness response of LEDs so brightness
//! appears linear to the human eye. The pipeline is unorm16 end to end
//! (shader sample → gamma → brightness → power limit → `DisplayPipeline`),
//! so gamma must be u16 → u16: an 8-bit table here quantizes the whole
//! pipeline down to 8 bits and the wire-side temporal dithering cannot
//! recover what gamma already destroyed (see
//! docs/defects/2026-08-01-gamma-8bit-choke.md).
//!
//! The curve is x^2.8, matching the exponent of the legacy 8-bit Adafruit
//! table this replaced. It is evaluated by linear interpolation over a
//! 513-entry lookup table (512 segments of 128 input counts), the same
//! shape as the white-point LUT in `lpc_shared::display_pipeline::lut`.
//! Entries carry 2 fractional bits so table rounding stays below the
//! interpolation error; the final entry sits one segment step past the
//! input domain (like the white-point LUT's 257th entry) and holds the
//! extrapolated curve value there, which is what lets interpolation land
//! exactly on 65535 at the top without an edge clamp. Max absolute error
//! versus the analytic curve is under one output count across the full
//! domain (asserted by test).

/// Number of GAMMA16 entries (index 0..=512; segment width 128 counts).
pub const GAMMA16_LEN: usize = 513;

/// Gamma lookup table, 16.2 fixed point:
/// `GAMMA16[i] = round(4 · 65535 · ((128·i) / 65535)^2.8)`.
///
/// Generated table — do not hand-edit; `table_matches_generator` compares
/// every entry against the formula.
pub const GAMMA16: [u32; GAMMA16_LEN] = [
    0, 0, 0, 0, 0, 1, 1, 2, 2, 3, 4, 6, 7, 9, 11, 13, 16, 19, 22, 26, 30, 34, 39, 44, 50, 56, 62,
    69, 77, 85, 93, 102, 111, 121, 132, 143, 155, 167, 180, 194, 208, 223, 239, 255, 272, 289, 308,
    327, 347, 367, 389, 411, 434, 458, 482, 508, 534, 561, 589, 618, 648, 678, 710, 743, 776, 811,
    846, 882, 920, 958, 997, 1038, 1079, 1122, 1165, 1210, 1256, 1302, 1350, 1399, 1450, 1501,
    1553, 1607, 1662, 1718, 1775, 1833, 1893, 1954, 2016, 2079, 2144, 2210, 2277, 2345, 2415, 2486,
    2559, 2633, 2708, 2784, 2862, 2941, 3022, 3104, 3188, 3272, 3359, 3447, 3536, 3627, 3719, 3813,
    3908, 4005, 4103, 4203, 4304, 4407, 4511, 4617, 4725, 4834, 4945, 5058, 5172, 5287, 5405, 5524,
    5645, 5767, 5891, 6017, 6145, 6274, 6405, 6538, 6672, 6808, 6946, 7086, 7228, 7371, 7516, 7663,
    7812, 7963, 8116, 8270, 8427, 8585, 8745, 8907, 9071, 9237, 9405, 9574, 9746, 9920, 10096,
    10273, 10453, 10635, 10818, 11004, 11192, 11382, 11573, 11767, 11963, 12161, 12362, 12564,
    12768, 12975, 13184, 13394, 13607, 13822, 14040, 14259, 14481, 14705, 14931, 15159, 15390,
    15623, 15858, 16095, 16335, 16576, 16821, 17067, 17316, 17567, 17820, 18076, 18334, 18595,
    18857, 19123, 19390, 19660, 19932, 20207, 20484, 20764, 21046, 21331, 21618, 21907, 22199,
    22494, 22791, 23090, 23392, 23696, 24003, 24313, 24625, 24940, 25257, 25577, 25899, 26224,
    26552, 26882, 27215, 27551, 27889, 28230, 28573, 28920, 29268, 29620, 29974, 30331, 30691,
    31053, 31419, 31787, 32157, 32531, 32907, 33286, 33668, 34052, 34440, 34830, 35223, 35619,
    36018, 36419, 36824, 37231, 37642, 38055, 38471, 38890, 39312, 39736, 40164, 40595, 41029,
    41465, 41905, 42347, 42793, 43242, 43693, 44148, 44606, 45066, 45530, 45997, 46467, 46940,
    47416, 47895, 48377, 48862, 49351, 49842, 50337, 50835, 51336, 51840, 52347, 52858, 53372,
    53889, 54409, 54932, 55459, 55988, 56521, 57058, 57597, 58140, 58686, 59236, 59788, 60344,
    60903, 61466, 62032, 62601, 63174, 63750, 64329, 64912, 65498, 66088, 66681, 67277, 67877,
    68480, 69086, 69696, 70310, 70927, 71547, 72171, 72798, 73429, 74064, 74702, 75343, 75988,
    76636, 77288, 77944, 78603, 79266, 79932, 80602, 81276, 81953, 82633, 83318, 84006, 84697,
    85392, 86091, 86794, 87500, 88210, 88924, 89641, 90362, 91087, 91815, 92548, 93284, 94023,
    94767, 95514, 96265, 97020, 97778, 98541, 99307, 100077, 100851, 101629, 102410, 103196,
    103985, 104778, 105575, 106376, 107181, 107989, 108802, 109618, 110439, 111263, 112092, 112924,
    113760, 114600, 115445, 116293, 117145, 118001, 118861, 119726, 120594, 121466, 122342, 123223,
    124107, 124996, 125888, 126785, 127686, 128591, 129500, 130413, 131330, 132251, 133177, 134107,
    135040, 135978, 136921, 137867, 138818, 139772, 140731, 141695, 142662, 143634, 144610, 145590,
    146574, 147563, 148556, 149553, 150555, 151560, 152571, 153585, 154604, 155627, 156654, 157686,
    158723, 159763, 160808, 161857, 162911, 163969, 165032, 166099, 167170, 168246, 169326, 170411,
    171500, 172593, 173691, 174794, 175901, 177013, 178129, 179249, 180374, 181504, 182638, 183777,
    184920, 186068, 187220, 188377, 189539, 190705, 191875, 193051, 194231, 195415, 196605, 197798,
    198997, 200200, 201408, 202620, 203838, 205060, 206286, 207517, 208753, 209994, 211240, 212490,
    213745, 215004, 216269, 217538, 218812, 220091, 221375, 222663, 223956, 225254, 226557, 227865,
    229177, 230495, 231817, 233144, 234476, 235813, 237155, 238501, 239853, 241209, 242571, 243937,
    245308, 246684, 248065, 249452, 250843, 252239, 253640, 255046, 256457, 257873, 259294, 260720,
    262151,
];

/// Apply gamma correction to a single unorm16 channel value.
///
/// Linear interpolation through [`GAMMA16`]: index = value >> 7,
/// alpha = value & 0x7F, blending `GAMMA16[index]` and `GAMMA16[index + 1]`.
/// The `>> 9` folds away the interpolation weight (7 bits) and the table's
/// fractional bits (2); the `+ 0x100` rounds to nearest.
///
/// Monotone non-decreasing, with exact endpoints:
/// `apply_gamma16(0) == 0` and `apply_gamma16(65535) == 65535`.
#[inline]
pub fn apply_gamma16(value: u16) -> u16 {
    let v = value as u32;
    let index = (v >> 7) as usize;
    let alpha = v & 0x7F;
    let blended = GAMMA16[index] * (0x80 - alpha) + GAMMA16[index + 1] * alpha;
    ((blended + 0x100) >> 9).min(0xFFFF) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The formula behind the checked-in literal. Entry 512 is one segment
    /// step past the input domain, so it exceeds 4 · 65535 by design.
    fn generator(i: usize) -> u32 {
        let x = (128 * i) as f64 / 65535.0;
        libm::round(4.0 * 65535.0 * libm::pow(x, 2.8)) as u32
    }

    #[test]
    fn table_matches_generator() {
        for (i, &entry) in GAMMA16.iter().enumerate() {
            assert_eq!(entry, generator(i), "GAMMA16[{i}] drifted from formula");
        }
    }

    #[test]
    fn matches_analytic_curve_within_one_count() {
        let mut max_err = 0.0f64;
        for v in 0..=u16::MAX {
            let out = apply_gamma16(v) as f64;
            let oracle = 65535.0 * libm::pow(v as f64 / 65535.0, 2.8);
            let err = libm::fabs(out - oracle);
            if err > max_err {
                max_err = err;
            }
        }
        // Measured 0.75; interpolation curvature alone bounds at ~0.16,
        // the rest is table + output rounding.
        assert!(max_err <= 1.0, "max error {max_err} exceeds one count");
    }

    #[test]
    fn monotone_with_exact_endpoints() {
        assert_eq!(apply_gamma16(0), 0);
        assert_eq!(apply_gamma16(u16::MAX), u16::MAX);
        let mut prev = 0u16;
        for v in 0..=u16::MAX {
            let out = apply_gamma16(v);
            assert!(out >= prev, "output decreased at input {v}");
            prev = out;
        }
    }

    /// Regression for the 8-bit choke this table replaced: at fixture
    /// brightness 38/255 (a real desk project), post-brightness values span
    /// [0, ~9766] and the old u8 table collapsed every pixel to {0, 257} —
    /// binary output. The 16-bit curve keeps hundreds of distinct levels.
    #[test]
    fn dim_brightness_keeps_many_levels() {
        let brightness_q16 = (38u32 << 16) / 255;
        let mut distinct = [false; 65536];
        let mut count = 0usize;
        for v in 0..=u16::MAX as u32 {
            let scaled = ((v * brightness_q16) >> 16) as u16;
            let out = apply_gamma16(scaled) as usize;
            if !distinct[out] {
                distinct[out] = true;
                count += 1;
            }
        }
        // Measured 318 distinct levels; the old path produced exactly 2.
        assert!(count >= 250, "only {count} distinct post-gamma levels");
    }
}
