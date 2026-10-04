//! A canonical Huffman code, built from a table of per-symbol code lengths
//! (RFC 1951 §3.2.2) and decoded one bit at a time (RFC 1951 §3.2.4's own
//! `fixed_bl_count` / `fixed_bl_symbol` sketch, generalized to any length
//! table and reversed as a loop instead of a table).
//!
//! `N` is the symbol-alphabet size: 288 for the literal/length table, 30 for
//! distances, 19 for the code-length table that describes a dynamic block's
//! other two tables.

use crate::Error;
use crate::bits::Bits;

#[cfg_attr(test, derive(Debug))]
pub(crate) struct Huff<const N: usize> {
    /// Number of codes of each length, index 0 unused (deflate has no
    /// zero-length codes).
    count: [u16; 16],
    /// Symbols in code order within each length, laid out contiguously by
    /// increasing length (see [`Huff::new`]).
    symbol: [u16; N],
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
        Ok(h)
    }

    /// Reads one symbol: one bit at a time, most-significant first, against
    /// the running count of how many codes of each length sort before the
    /// bits read so far.
    pub(crate) fn decode(&self, bits: &mut Bits) -> Result<u16, Error> {
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
