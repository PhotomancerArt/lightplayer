//! What each word of the region is meant to hold.
//!
//! A function of the word's absolute address and the boot's generation,
//! about half its bits 0 (the density `flash-tears` measured its tears
//! under), and different every generation, so no cell is programmed the
//! same way twice running.

/// The word at `addr` (4-aligned) after generation `generation` programs it.
pub fn intended_word(addr: u32, generation: u32) -> u32 {
    let w = splitmix(0x0EE7_B175_0000_0000 ^ (u64::from(generation) << 32) ^ u64::from(addr >> 2))
        as u32;
    // An all-ones word would read as erased: never meant.
    if w == u32::MAX { 0x7FFF_FFFF } else { w }
}

/// Fill `out` with the intended bytes from `addr` on (both 4-aligned).
pub fn fill(addr: u32, generation: u32, out: &mut [u8]) {
    debug_assert!(addr % 4 == 0 && out.len() % 4 == 0);
    for (i, chunk) in out.chunks_exact_mut(4).enumerate() {
        chunk.copy_from_slice(&intended_word(addr + 4 * i as u32, generation).to_le_bytes());
    }
}

/// SplitMix64's output function, as a stateless step.
fn splitmix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_half_the_bits_are_zero_and_generations_differ() {
        let zeros: u32 = (0..1024u32)
            .map(|i| intended_word(0x35_2000 + 4 * i, 7).count_zeros())
            .sum();
        assert!(
            zeros > 32 * 1024 * 45 / 100 && zeros < 32 * 1024 * 55 / 100,
            "{zeros}"
        );
        assert_ne!(intended_word(0x35_2000, 1), intended_word(0x35_2000, 2));
        let mut b = [0u8; 8];
        fill(0x35_2004, 3, &mut b);
        assert_eq!(b[..4], intended_word(0x35_2004, 3).to_le_bytes());
        assert_eq!(b[4..], intended_word(0x35_2008, 3).to_le_bytes());
    }
}
