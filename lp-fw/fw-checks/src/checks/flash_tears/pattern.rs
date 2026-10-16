//! The bytes a region sector holds after a cycle completes.
//!
//! Each sector has one random base pattern (seeded by the sector index, about
//! half its bits 0). Generation `g = cycle / REGION_SECTORS` writes the base
//! when `g` is even and its bitwise complement when `g` is odd, so the pattern
//! a cycle programs is always the complement of the one it erased: the old
//! zeros and the new zeros never share a bit, and a torn sector's stable zeros
//! say which operation left them.

use super::{REGION_SECTORS, SECTOR_SIZE};

/// Fill `out` with the pattern `cycle` programs into `sector`.
pub fn fill_pattern(sector: u32, cycle: u32, out: &mut [u8]) {
    debug_assert_eq!(out.len(), SECTOR_SIZE);
    let invert = (cycle / REGION_SECTORS) % 2 == 1;
    let mut state = splitmix(0xF1A5_7EA2_0000_0000 ^ u64::from(sector));
    for chunk in out.chunks_mut(8) {
        state = splitmix(state);
        let word = if invert { !state } else { state };
        let bytes = word.to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
}

/// SplitMix64's output function, used as a stateless step (the unaligned
/// program plan draws its lengths with it too).
pub(super) fn splitmix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(sector: u32, cycle: u32) -> [u8; SECTOR_SIZE] {
        let mut b = [0u8; SECTOR_SIZE];
        fill_pattern(sector, cycle, &mut b);
        b
    }

    #[test]
    fn consecutive_generations_are_complements() {
        let a = pat(3, 3);
        let b = pat(3, 3 + REGION_SECTORS);
        let c = pat(3, 3 + 2 * REGION_SECTORS);
        for i in 0..SECTOR_SIZE {
            assert_eq!(a[i], !b[i]);
        }
        assert_eq!(a, c);
    }

    #[test]
    fn about_half_the_bits_are_zero_and_sectors_differ() {
        let a = pat(0, 0);
        let zeros: u32 = a.iter().map(|b| b.count_zeros()).sum();
        let bits = (SECTOR_SIZE * 8) as u32;
        assert!(
            zeros > bits * 45 / 100 && zeros < bits * 55 / 100,
            "{zeros}"
        );
        assert_ne!(pat(0, 0), pat(1, 1));
        // The pattern is a function of the sector and the generation only.
        assert_eq!(pat(5, 5), pat(5, 5));
    }
}
