//! A canonical Huffman code, built from a table of per-symbol code lengths
//! (RFC 1951 §3.2.2) and decoded through a lookup table on the next
//! [`FAST_BITS`] bits, falling back to one bit at a time (RFC 1951 §3.2.4's
//! own `fixed_bl_count` / `fixed_bl_symbol` sketch, generalized to any
//! length table and reversed as a loop instead of a table) for a code longer
//! than that, a code the table does not hold, and the last bits of a stream.
//!
//! The lookup table is filled from the codes §3.2.2's `next_code` rule
//! assigns: every code of `len <= FAST_BITS` bits owns the
//! `2^(FAST_BITS - len)` entries whose low `len` bits are its bits in stream
//! order (deflate packs a Huffman code most-significant bit first into a
//! least-significant-first stream, §3.1.1). An empty entry means "not decided
//! by the first `FAST_BITS` bits": the slow loop decides it, so a corrupt or
//! truncated stream fails exactly as it did before the table.
//!
//! `N` is the symbol-alphabet size: 288 for the literal/length table, 30 for
//! distances, 19 for the code-length table that describes a dynamic block's
//! other two tables.

use crate::Error;
use crate::bits::Bits;

/// Bits the lookup table decides a code from.
pub(crate) const FAST_BITS: u32 = 9;
const FAST_LEN: usize = 1 << FAST_BITS;
/// A table entry: the code's length in the top four bits, its symbol below.
const FAST_LEN_SHIFT: u32 = 12;
const FAST_SYM_MASK: u16 = (1 << FAST_LEN_SHIFT) - 1;

#[cfg_attr(test, derive(Debug))]
pub(crate) struct Huff<const N: usize> {
    /// Number of codes of each length, index 0 unused (deflate has no
    /// zero-length codes).
    count: [u16; 16],
    /// Symbols in code order within each length, laid out contiguously by
    /// increasing length (see [`Huff::new`]).
    symbol: [u16; N],
    /// The next [`FAST_BITS`] stream bits → `len << 12 | symbol`, `0` when
    /// those bits do not decide a code (see the module docs).
    fast: [u16; FAST_LEN],
}

impl<const N: usize> Huff<N> {
    /// Builds the canonical code for `lengths[sym] = ` that symbol's code
    /// length, 0 meaning "not in this code". Rejects an over-subscribed
    /// table (RFC 1951 is explicit that one is possible only in a corrupt
    /// stream); an incomplete table is accepted, since RFC 1951 §3.2.7
    /// permits a single valid distance code with no matches to decode.
    pub(crate) fn new(lengths: &[u8]) -> Result<Self, Error> {
        let mut h = Huff {
            count: [0; 16],
            symbol: [0; N],
            fast: [0; FAST_LEN],
        };
        for &l in lengths {
            h.count[usize::from(l)] += 1;
        }
        h.count[0] = 0;
        let mut left: i32 = 1;
        for len in 1..16 {
            left <<= 1;
            left -= i32::from(h.count[len]);
            if left < 0 {
                return Err(Error::Corrupt);
            }
        }
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + h.count[len];
        }
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                h.symbol[usize::from(offs[usize::from(l)])] = sym as u16;
                offs[usize::from(l)] += 1;
            }
        }
        h.fill_fast();
        Ok(h)
    }

    /// The lookup table: §3.2.2's codes, in `symbol`'s order (by length,
    /// then symbol), each `len <= FAST_BITS` code spread over every entry
    /// whose low `len` bits are its bits in stream order.
    fn fill_fast(&mut self) {
        let mut code: u32 = 0;
        let mut index = 0usize;
        for len in 1..=FAST_BITS {
            for _ in 0..self.count[len as usize] {
                let sym = self.symbol[index];
                index += 1;
                let entry = ((len as u16) << FAST_LEN_SHIFT) | sym;
                // MSB-first code → the order its bits arrive in.
                let first = (code.reverse_bits() >> (32 - len)) as usize;
                let mut at = first;
                while at < FAST_LEN {
                    self.fast[at] = entry;
                    at += 1 << len;
                }
                code += 1;
            }
            code <<= 1;
        }
    }

    /// Reads one symbol: from the lookup table when the next bits decide
    /// it, else one bit at a time ([`Huff::decode_slow`]).
    #[inline]
    pub(crate) fn decode(&self, bits: &mut Bits) -> Result<u16, Error> {
        let (next, have) = bits.peek(FAST_BITS);
        let entry = self.fast[next as usize];
        let len = u32::from(entry >> FAST_LEN_SHIFT);
        if len != 0 && len <= have {
            bits.consume(len);
            return Ok(entry & FAST_SYM_MASK);
        }
        self.decode_slow(bits)
    }

    /// Reads one symbol: one bit at a time, most-significant first, against
    /// the running count of how many codes of each length sort before the
    /// bits read so far.
    pub(crate) fn decode_slow(&self, bits: &mut Bits) -> Result<u16, Error> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.take(1)? as i32;
            let count = i32::from(self.count[len]);
            if code - count < first {
                return Ok(self.symbol[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(Error::Corrupt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_symbol_code_decodes_one_bit() {
        // One symbol at length 1 is incomplete (RFC 1951 allows it for the
        // single-distance-code case) but must still decode.
        let h: Huff<4> = Huff::new(&[0, 1, 0, 0]).unwrap();
        let mut bits = Bits::new(&[0b0000_0000]);
        assert_eq!(h.decode(&mut bits).unwrap(), 1);
    }

    #[test]
    fn a_small_canonical_code_round_trips_every_symbol() {
        // Lengths 2,1,3,3 for symbols 0..4: a valid, complete canonical code.
        // Canonical assignment (MSB-first): sym1=0, sym0=10, sym2=110, sym3=111.
        let h: Huff<4> = Huff::new(&[2, 1, 3, 3]).unwrap();

        let mut bits = Bits::new(&[0b0000_0001]); // "10" MSB-first -> bits 0,1
        assert_eq!(h.decode(&mut bits).unwrap(), 0);

        let mut bits = Bits::new(&[0b0000_0000]);
        assert_eq!(h.decode(&mut bits).unwrap(), 1);

        let mut bits = Bits::new(&[0b0000_0011]); // "110"
        assert_eq!(h.decode(&mut bits).unwrap(), 2);

        let mut bits = Bits::new(&[0b0000_0111]); // "111"
        assert_eq!(h.decode(&mut bits).unwrap(), 3);
    }

    #[test]
    fn the_table_and_the_bit_loop_agree_on_every_bit_pattern() {
        // Literal/length-shaped lengths with codes both shorter and longer
        // than FAST_BITS, plus an incomplete code (unused code space).
        let mut lens = [0u8; 288];
        for (i, l) in lens.iter_mut().enumerate() {
            *l = match i % 7 {
                0 => 7,
                1 => 8,
                2 => 9,
                3 => 10,
                4 => 12,
                5 => 0,
                _ => 11,
            };
        }
        let h: Huff<288> = Huff::new(&lens).unwrap();
        for pattern in 0u32..(1 << 16) {
            let bytes = pattern.to_le_bytes();
            for n in [1usize, 2] {
                let (mut a, mut b) = (Bits::new(&bytes[..n]), Bits::new(&bytes[..n]));
                let fast = h.decode(&mut a);
                let slow = h.decode_slow(&mut b);
                assert_eq!(fast, slow, "pattern {pattern:#06x}, {n} bytes");
                // Both consumed the same bits.
                assert_eq!(a.peek(16), b.peek(16), "pattern {pattern:#06x}, {n} bytes");
            }
        }
    }

    #[test]
    fn over_subscribed_lengths_are_corrupt() {
        // Three symbols at length 1 cannot fit (only two length-1 codes exist).
        assert_eq!(Huff::<4>::new(&[1, 1, 1, 0]).unwrap_err(), Error::Corrupt);
    }

    #[test]
    fn running_out_of_bits_mid_code_is_truncated() {
        let h: Huff<4> = Huff::new(&[2, 1, 3, 3]).unwrap();
        let mut bits = Bits::new(&[]);
        assert_eq!(h.decode(&mut bits).unwrap_err(), Error::Truncated);
    }
}
