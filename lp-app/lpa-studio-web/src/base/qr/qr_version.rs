//! What each version (1–10) holds at error-correction level M.
//!
//! The numbers are the standard's capacity table for level M: how many
//! error-correction codewords each block carries, and how the data
//! codewords split into blocks. A version's total codeword count is not
//! stored — it falls out of the module layout ([`super::qr_code`] counts the
//! free modules), and a test checks the two agree.

/// The largest version this encoder produces.
pub const MAX_VERSION: u8 = 10;

/// One version's block structure at level M.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockLayout {
    /// Error-correction codewords per block (every block has the same).
    pub ec_per_block: usize,
    /// Blocks in the first group, and the data codewords each carries.
    pub short_blocks: usize,
    pub short_len: usize,
    /// Blocks in the second group, each one data codeword longer.
    pub long_blocks: usize,
}

impl BlockLayout {
    /// Every data codeword the version carries.
    pub fn data_codewords(&self) -> usize {
        self.short_blocks * self.short_len + self.long_blocks * (self.short_len + 1)
    }

    /// Every codeword, data and error correction.
    pub fn total_codewords(&self) -> usize {
        self.data_codewords() + (self.short_blocks + self.long_blocks) * self.ec_per_block
    }
}

/// Level M's layout for `version` (1–10).
pub fn block_layout(version: u8) -> BlockLayout {
    let (ec_per_block, short_blocks, short_len, long_blocks) = match version {
        1 => (10, 1, 16, 0),
        2 => (16, 1, 28, 0),
        3 => (26, 1, 44, 0),
        4 => (18, 2, 32, 0),
        5 => (24, 2, 43, 0),
        6 => (16, 4, 27, 0),
        7 => (18, 4, 31, 0),
        8 => (22, 2, 38, 2),
        9 => (22, 3, 36, 2),
        10 => (26, 4, 43, 1),
        _ => panic!("QR version {version} is outside 1..=10"),
    };
    BlockLayout {
        ec_per_block,
        short_blocks,
        short_len,
        long_blocks,
    }
}

/// The byte-mode character count field's width: 8 bits through version 9,
/// 16 from version 10.
pub fn count_bits(version: u8) -> usize {
    if version <= 9 { 8 } else { 16 }
}

/// Side length in modules.
pub fn side(version: u8) -> usize {
    4 * usize::from(version) + 17
}

/// Row/column coordinates of the alignment pattern centres (empty for
/// version 1). Every pairing is a centre, except the three that would sit
/// on a finder pattern.
pub fn alignment_centers(version: u8) -> &'static [usize] {
    match version {
        1 => &[],
        2 => &[6, 18],
        3 => &[6, 22],
        4 => &[6, 26],
        5 => &[6, 30],
        6 => &[6, 34],
        7 => &[6, 22, 38],
        8 => &[6, 24, 42],
        9 => &[6, 26, 46],
        10 => &[6, 28, 50],
        _ => panic!("QR version {version} is outside 1..=10"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The standard's level-M data capacity, in codewords, for 1–10.
    #[test]
    fn data_capacity_matches_the_standard() {
        let expected = [16, 28, 44, 64, 86, 108, 124, 154, 182, 216];
        for (version, want) in (1..=MAX_VERSION).zip(expected) {
            assert_eq!(block_layout(version).data_codewords(), want, "v{version}");
        }
    }

    #[test]
    fn the_last_alignment_centre_is_seven_from_the_edge() {
        for version in 2..=MAX_VERSION {
            let centers = alignment_centers(version);
            assert_eq!(*centers.last().unwrap(), side(version) - 7, "v{version}");
        }
    }
}
