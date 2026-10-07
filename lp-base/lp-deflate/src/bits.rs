//! An LSB-first bit reader over a byte slice — RFC 1951 §3.1.1: "packing the
//! bits starting with the least-significant bit of the byte."
//!
//! Deflate's three field kinds all read through [`Bits::take`]: fixed-width
//! header fields (block type, stored length), Huffman code bits (read one at
//! a time by [`crate::huffman::Huff::decode`], which reverses them back to
//! MSB-first per RFC 1951 §3.1.1), and extra length/distance bits (read
//! verbatim, already LSB-first per §3.2.5).

use crate::Error;

pub(crate) struct Bits<'a> {
    pub(crate) src: &'a [u8],
    pub(crate) pos: usize,
    acc: u32,
    n: u32,
}

impl<'a> Bits<'a> {
    pub(crate) fn new(src: &'a [u8]) -> Self {
        Bits {
            src,
            pos: 0,
            acc: 0,
            n: 0,
        }
    }

    fn need(&mut self, k: u32) -> Result<(), Error> {
        while self.n < k {
            let b = *self.src.get(self.pos).ok_or(Error::Truncated)?;
            self.pos += 1;
            self.acc |= u32::from(b) << self.n;
            self.n += 8;
        }
        Ok(())
    }

    /// The next `k` bits (0..=16), least-significant first.
    pub(crate) fn take(&mut self, k: u32) -> Result<u32, Error> {
        if k == 0 {
            return Ok(0);
        }
        self.need(k)?;
        let v = self.acc & ((1 << k) - 1);
        self.acc >>= k;
        self.n -= k;
        Ok(v)
    }

    /// The next `k` bits (0..=16), least-significant first, zero-filled past
    /// the end of the input, and how many of them are real (`k`, or fewer at
    /// the end). Consumes nothing: [`Bits::consume`] does.
    #[inline]
    pub(crate) fn peek(&mut self, k: u32) -> (u32, u32) {
        while self.n < k {
            let Some(&b) = self.src.get(self.pos) else {
                break;
            };
            self.pos += 1;
            self.acc |= u32::from(b) << self.n;
            self.n += 8;
        }
        (self.acc & ((1 << k) - 1), self.n.min(k))
    }

    /// Drops `k` bits a [`Bits::peek`] showed were there.
    #[inline]
    pub(crate) fn consume(&mut self, k: u32) {
        self.acc >>= k;
        self.n -= k;
    }

    /// Discards the bits remaining in the current byte, so the next `take`
    /// starts at a byte boundary (RFC 1951 §3.2.3, before a stored block).
    pub(crate) fn align(&mut self) {
        let drop = self.n % 8;
        self.acc >>= drop;
        self.n -= drop;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_bits_lsb_first_across_byte_boundaries() {
        // 0b1011_0101, 0b0000_0001 — low 4 bits of byte 0, then enough of the
        // rest to cross into byte 1.
        let mut bits = Bits::new(&[0b1011_0101, 0b0000_0001]);
        assert_eq!(bits.take(4).unwrap(), 0b0101);
        // Remaining 4 bits of byte 0 (1011) plus the low 2 bits of byte 1
        // (01), byte 1's bits placed above byte 0's in the accumulator:
        // 01_1011 = 27.
        assert_eq!(bits.take(6).unwrap(), 0b01_1011);
        assert_eq!(bits.take(6).unwrap(), 0);
    }

    #[test]
    fn take_zero_bits_is_free() {
        let mut bits = Bits::new(&[]);
        assert_eq!(bits.take(0).unwrap(), 0);
    }

    #[test]
    fn running_out_of_input_is_truncated() {
        let mut bits = Bits::new(&[0xff]);
        assert_eq!(bits.take(8).unwrap(), 0xff);
        assert_eq!(bits.take(1), Err(Error::Truncated));
    }

    #[test]
    fn align_drops_to_the_next_byte_boundary() {
        let mut bits = Bits::new(&[0b0000_0111, 0xaa]);
        assert_eq!(bits.take(3).unwrap(), 0b111);
        bits.align();
        assert_eq!(bits.take(8).unwrap(), 0xaa);
    }

    #[test]
    fn align_on_a_boundary_is_a_no_op() {
        let mut bits = Bits::new(&[0xaa, 0xbb]);
        assert_eq!(bits.take(8).unwrap(), 0xaa);
        bits.align();
        assert_eq!(bits.take(8).unwrap(), 0xbb);
    }
}
