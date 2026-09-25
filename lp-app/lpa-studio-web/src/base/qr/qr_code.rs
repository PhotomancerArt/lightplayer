//! One QR symbol: encode bytes, lay out the function patterns, place the
//! codewords, mask.
//!
//! The steps, in the standard's order:
//!
//! 1. **Bit stream.** Mode indicator `0100` (byte), the length (8 or 16
//!    bits, [`qr_version::count_bits`]), the bytes, a terminator of up to
//!    four zeros, zeros to the byte boundary, then the pad bytes `0xEC`,
//!    `0x11` alternately to the version's data capacity. The version is the
//!    smallest (1–10) that holds it.
//! 2. **Blocks.** The data codewords split into the version's blocks (short
//!    ones first); each block gets its Reed–Solomon codewords. The final
//!    sequence interleaves them: the i-th data codeword of every block,
//!    then the i-th error-correction codeword of every block.
//! 3. **Function patterns.** Three finders with their light separators,
//!    the two timing lines, the alignment patterns, the dark module, and
//!    the reserved format (and, from version 7, version) areas.
//! 4. **Placement.** Two-module-wide columns from the right edge, snaking
//!    up then down, skipping the vertical timing column; each column pair
//!    fills its right module before its left. Most-significant bit first.
//!    Modules left over (the remainder bits) stay light.
//! 5. **Mask and format.** Each of the eight masks flips the data modules;
//!    the format bits (level M, the mask) are written, and the lowest
//!    penalty wins.

use super::qr_mask;
use super::qr_version::{self, MAX_VERSION};
use super::reed_solomon;

/// Level M's two format-information bits.
const LEVEL_M_BITS: u32 = 0b00;

/// A finished symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrCode {
    /// 1–10.
    pub version: u8,
    /// The mask used (0–7).
    pub mask: u8,
    /// `modules[row][col]`, true = dark. Square, `4 × version + 17` wide.
    pub modules: Vec<Vec<bool>>,
}

impl QrCode {
    /// Encode `data` in byte mode at level M, choosing the mask by penalty.
    /// `None` when it does not fit version 10 (213 bytes).
    pub fn encode(data: &[u8]) -> Option<Self> {
        let (version, codewords) = codewords_for(data)?;
        (0..8)
            .map(|mask| Self::build(version, &codewords, mask))
            .min_by_key(|code| qr_mask::penalty(&code.modules))
    }

    /// Encode with a given mask (0–7) instead of the chosen one.
    pub fn encode_with_mask(data: &[u8], mask: u8) -> Option<Self> {
        let (version, codewords) = codewords_for(data)?;
        Some(Self::build(version, &codewords, mask))
    }

    /// Side length in modules.
    pub fn side(&self) -> usize {
        self.modules.len()
    }

    fn build(version: u8, codewords: &[u8], mask: u8) -> Self {
        let mut grid = Grid::with_function_patterns(version);
        grid.place(codewords);
        grid.apply_mask(mask);
        grid.draw_format(mask);
        Self {
            version,
            mask,
            modules: grid.dark,
        }
    }
}

/// The smallest version holding `data`, and its final codeword sequence
/// (data and error correction, interleaved).
fn codewords_for(data: &[u8]) -> Option<(u8, Vec<u8>)> {
    let version = (1..=MAX_VERSION).find(|&version| {
        let capacity = qr_version::block_layout(version).data_codewords() * 8;
        4 + qr_version::count_bits(version) + 8 * data.len() <= capacity
    })?;
    let layout = qr_version::block_layout(version);
    let data_codewords = data_stream(data, version, layout.data_codewords());
    Some((version, interleave(&data_codewords, layout)))
}

/// Step 1: the padded data codewords.
fn data_stream(data: &[u8], version: u8, capacity: usize) -> Vec<u8> {
    let mut bits = BitWriter::default();
    bits.push(0b0100, 4);
    bits.push(data.len() as u32, qr_version::count_bits(version));
    for &byte in data {
        bits.push(u32::from(byte), 8);
    }
    let capacity_bits = capacity * 8;
    let terminator = (capacity_bits - bits.len()).min(4);
    bits.push(0, terminator);
    let to_byte = (8 - bits.len() % 8) % 8;
    bits.push(0, to_byte);
    let mut bytes = bits.into_bytes();
    for pad in [0xEC, 0x11].into_iter().cycle() {
        if bytes.len() >= capacity {
            break;
        }
        bytes.push(pad);
    }
    bytes
}

/// Step 2: blocks, their error correction, interleaved.
fn interleave(data: &[u8], layout: qr_version::BlockLayout) -> Vec<u8> {
    let generator = reed_solomon::generator(layout.ec_per_block);
    let mut blocks: Vec<(&[u8], Vec<u8>)> = Vec::new();
    let mut rest = data;
    for index in 0..layout.short_blocks + layout.long_blocks {
        let len = layout.short_len + usize::from(index >= layout.short_blocks);
        let (block, tail) = rest.split_at(len);
        blocks.push((block, reed_solomon::remainder(block, &generator)));
        rest = tail;
    }
    let mut out = Vec::with_capacity(layout.total_codewords());
    for i in 0..=layout.short_len {
        for (block, _) in &blocks {
            if let Some(&byte) = block.get(i) {
                out.push(byte);
            }
        }
    }
    for i in 0..layout.ec_per_block {
        for (_, ec) in &blocks {
            out.push(ec[i]);
        }
    }
    out
}

/// The symbol under construction: module colours, and which modules belong
/// to function patterns (never data, never masked).
struct Grid {
    version: u8,
    dark: Vec<Vec<bool>>,
    reserved: Vec<Vec<bool>>,
}

impl Grid {
    /// Step 3.
    fn with_function_patterns(version: u8) -> Self {
        let side = qr_version::side(version);
        let mut grid = Self {
            version,
            dark: vec![vec![false; side]; side],
            reserved: vec![vec![false; side]; side],
        };
        // Timing lines first; the finders and alignments overwrite where
        // they cross.
        for i in 0..side {
            grid.set(6, i, i % 2 == 0);
            grid.set(i, 6, i % 2 == 0);
        }
        for (row, col) in [(0, 0), (0, side - 7), (side - 7, 0)] {
            grid.finder(row, col);
        }
        let centers = qr_version::alignment_centers(version);
        let last = centers.len().saturating_sub(1);
        for (a, &row) in centers.iter().enumerate() {
            for (b, &col) in centers.iter().enumerate() {
                let on_finder =
                    (a == 0 && b == 0) || (a == 0 && b == last) || (a == last && b == 0);
                if !on_finder {
                    grid.alignment(row, col);
                }
            }
        }
        // Reserve the format areas (written after masking) and set the
        // dark module.
        grid.draw_format(0);
        grid.set(side - 8, 8, true);
        if version >= 7 {
            grid.draw_version();
        }
        grid
    }

    fn set(&mut self, row: usize, col: usize, dark: bool) {
        self.dark[row][col] = dark;
        self.reserved[row][col] = true;
    }

    /// A 7×7 finder at (`top`, `left`) and the light separator ring round
    /// it, clipped at the symbol's edge.
    fn finder(&mut self, top: usize, left: usize) {
        let side = self.dark.len() as isize;
        for dr in -1..=7isize {
            for dc in -1..=7isize {
                let (row, col) = (top as isize + dr, left as isize + dc);
                if !(0..side).contains(&row) || !(0..side).contains(&col) {
                    continue;
                }
                // Distance from the centre, in rings: 0–1 dark core, 2
                // light, 3 dark border, 4 the separator.
                let ring = (dr - 3).abs().max((dc - 3).abs());
                self.set(row as usize, col as usize, ring != 2 && ring != 4);
            }
        }
    }

    /// A 5×5 alignment pattern centred at (`row`, `col`).
    fn alignment(&mut self, row: usize, col: usize) {
        for dr in -2..=2isize {
            for dc in -2..=2isize {
                let ring = dr.abs().max(dc.abs());
                self.set(
                    (row as isize + dr) as usize,
                    (col as isize + dc) as usize,
                    ring != 1,
                );
            }
        }
    }

    /// The 18-bit version information (versions 7+), in its two 6×3
    /// blocks: bottom-left and top-right.
    fn draw_version(&mut self) {
        let bits = with_bch(u32::from(self.version), 12, 0x1F25);
        let side = self.dark.len();
        for i in 0..18 {
            let dark = (bits >> i) & 1 != 0;
            let (near, far) = (i / 3, side - 11 + i % 3);
            self.set(far, near, dark);
            self.set(near, far, dark);
        }
    }

    /// The 15 format bits (level M, `mask`), in both copies.
    fn draw_format(&mut self, mask: u8) {
        let data = (LEVEL_M_BITS << 3) | u32::from(mask);
        let bits = with_bch(data, 10, 0x537) ^ 0x5412;
        let bit = |i: usize| (bits >> i) & 1 != 0;
        let side = self.dark.len();
        // First copy, round the top-left finder: bits 0–5 down column 8
        // from the top, 6–8 turning the corner (skipping the timing
        // module), 9–14 along row 8 leftwards.
        for i in 0..6 {
            self.set(i, 8, bit(i));
        }
        self.set(7, 8, bit(6));
        self.set(8, 8, bit(7));
        self.set(8, 7, bit(8));
        for i in 9..15 {
            self.set(8, 14 - i, bit(i));
        }
        // Second copy: bits 0–7 along row 8 from the right edge, 8–14 up
        // column 8 from near the bottom.
        for i in 0..8 {
            self.set(8, side - 1 - i, bit(i));
        }
        for i in 8..15 {
            self.set(side - 15 + i, 8, bit(i));
        }
    }

    /// Step 4.
    fn place(&mut self, codewords: &[u8]) {
        let side = self.dark.len();
        let total_bits = codewords.len() * 8;
        let mut next = 0usize;
        let mut right = side - 1;
        let mut upward = true;
        loop {
            for step in 0..side {
                let row = if upward { side - 1 - step } else { step };
                for col in [right, right - 1] {
                    if self.reserved[row][col] {
                        continue;
                    }
                    if next < total_bits {
                        self.dark[row][col] = (codewords[next / 8] >> (7 - next % 8)) & 1 != 0;
                        next += 1;
                    }
                }
            }
            upward = !upward;
            if right < 2 {
                break;
            }
            right -= 2;
            // The vertical timing column is skipped: the pair left of it
            // is columns 5 and 4.
            if right == 6 {
                right = 5;
            }
        }
        debug_assert_eq!(next, total_bits, "every codeword placed");
    }

    fn apply_mask(&mut self, mask: u8) {
        let side = self.dark.len();
        for row in 0..side {
            for col in 0..side {
                if !self.reserved[row][col] && qr_mask::flips(mask, row, col) {
                    self.dark[row][col] = !self.dark[row][col];
                }
            }
        }
    }
}

/// `data` (of `data_bits` bits) followed by its BCH check bits: the
/// remainder of `data × 2^degree` modulo `generator`.
fn with_bch(data: u32, degree: u32, generator: u32) -> u32 {
    let mut rest = data << degree;
    let top = 31 - generator.leading_zeros();
    while rest != 0 && 31 - rest.leading_zeros() >= top {
        rest ^= generator << (31 - rest.leading_zeros() - top);
    }
    (data << degree) | rest
}

/// Bits, most significant first, packed into bytes.
#[derive(Default)]
struct BitWriter {
    bits: Vec<bool>,
}

impl BitWriter {
    fn push(&mut self, value: u32, count: usize) {
        for i in (0..count).rev() {
            self.bits.push((value >> i) & 1 != 0);
        }
    }

    fn len(&self) -> usize {
        self.bits.len()
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bits
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .enumerate()
                    .fold(0u8, |byte, (i, &bit)| byte | (u8::from(bit) << (7 - i)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The module layout leaves exactly the version's codewords' worth of
    /// data modules (plus the remainder bits) — the table and the drawing
    /// agree.
    #[test]
    fn the_free_modules_hold_every_codeword() {
        for version in 1..=MAX_VERSION {
            let grid = Grid::with_function_patterns(version);
            let free = grid.reserved.iter().flatten().filter(|&&r| !r).count();
            let codewords = qr_version::block_layout(version).total_codewords();
            let remainder = if (2..=6).contains(&version) { 7 } else { 0 };
            assert_eq!(free, codewords * 8 + remainder, "v{version}");
        }
    }

    /// The format words for level M — the standard's table values.
    #[test]
    fn format_bits_match_the_standard_table() {
        let format = |mask: u32| with_bch((LEVEL_M_BITS << 3) | mask, 10, 0x537) ^ 0x5412;
        assert_eq!(format(0), 0b101_0100_0001_0010);
        assert_eq!(format(1), 0b101_0001_0010_0101);
        assert_eq!(format(5), 0b100_0000_1100_1110);
        assert_eq!(format(7), 0b100_1010_1010_0000);
        // Version 7's information word, from the standard's table.
        assert_eq!(with_bch(7, 12, 0x1F25), 0x07C94);
    }

    #[test]
    fn the_smallest_version_that_fits_is_chosen() {
        // Level M, byte mode: version 1 holds 14 bytes, version 2 holds 26.
        assert_eq!(QrCode::encode(&[b'a'; 14]).unwrap().version, 1);
        assert_eq!(QrCode::encode(&[b'a'; 15]).unwrap().version, 2);
        assert_eq!(QrCode::encode(&[b'a'; 213]).unwrap().version, 10);
        assert!(QrCode::encode(&[b'a'; 214]).is_none());
    }

    /// The chosen mask is the lowest-penalty one.
    #[test]
    fn the_chosen_mask_has_the_lowest_penalty() {
        let data = b"https://lightplayer.app/unlock#PLAYFUL%20choker&maple-otter-42";
        let chosen = QrCode::encode(data).unwrap();
        for mask in 0..8 {
            let other = QrCode::encode_with_mask(data, mask).unwrap();
            assert!(qr_mask::penalty(&chosen.modules) <= qr_mask::penalty(&other.modules));
        }
    }

    /// The oracle (plan D15): `qrcodegen`, a dev-dependency only, builds
    /// the same symbols — same version, same level, each of the eight
    /// masks forced — and every module matches, for every length 1–150.
    #[test]
    fn every_module_matches_the_oracle_for_every_mask() {
        use qrcodegen::{Mask, QrCode as Oracle, QrCodeEcc, QrSegment, Version};

        for len in 1..=150usize {
            // Varied bytes, including non-ASCII, so byte mode is exercised
            // with every value class.
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + len * 11) as u8).collect();
            for mask in 0..8u8 {
                let mine = QrCode::encode_with_mask(&data, mask).unwrap();
                let oracle = Oracle::encode_segments_advanced(
                    &[QrSegment::make_bytes(&data)],
                    QrCodeEcc::Medium,
                    Version::new(1),
                    Version::new(10),
                    Some(Mask::new(mask)),
                    false,
                )
                .unwrap();
                assert_eq!(i32::from(mine.version), i32::from(oracle.version().value()));
                assert_eq!(mine.side() as i32, oracle.size(), "len {len}");
                for row in 0..mine.side() {
                    for col in 0..mine.side() {
                        assert_eq!(
                            mine.modules[row][col],
                            oracle.get_module(col as i32, row as i32),
                            "len {len} mask {mask} at row {row} col {col}"
                        );
                    }
                }
            }
        }
    }
}
