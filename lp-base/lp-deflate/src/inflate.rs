//! The decoder entry point: RFC 1951's block loop (§3.2.3) over the three
//! block kinds (stored, §3.2.4; fixed Huffman, §3.2.6; dynamic Huffman,
//! §3.2.7), dispatching into [`codes`] for the two Huffman kinds and
//! [`dynamic_tables`] for building a dynamic block's own tables first.

use crate::Error;
use crate::bits::Bits;
use crate::huffman::Huff;
use crate::tables::{DIST_BASE, DIST_EXTRA, LEN_BASE, LEN_EXTRA, fixed_tables};

/// Decodes the raw deflate stream `src` into `buf[start..]`.
///
/// `buf[..start]` is a **preset dictionary**: bytes already in place that a
/// back-reference may point into, exactly as if they had been decoded in an
/// earlier call into the same buffer (the OTA use this crate was built for:
/// each 4 KiB chunk is compressed against the 32 KiB already written before
/// it). Pass `start = 0` for a stream with no dictionary.
///
/// Returns the number of bytes written at or after `start` on success.
/// Never panics: a malformed `src`, an output that would overflow `buf`, or
/// a distance reaching before `buf`'s start all come back as an [`Error`].
pub fn inflate(src: &[u8], buf: &mut [u8], start: usize) -> Result<usize, Error> {
    let mut bits = Bits::new(src);
    let mut out = start;
    loop {
        let last = bits.take(1)?;
        match bits.take(2)? {
            0 => out = stored_block(&mut bits, buf, out)?,
            1 => {
                let (lit, dist) = fixed_tables();
                out = codes(&mut bits, buf, out, &lit, &dist)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut bits)?;
                out = codes(&mut bits, buf, out, &lit, &dist)?;
            }
            _ => return Err(Error::Corrupt),
        }
        if last == 1 {
            return Ok(out - start);
        }
    }
}

/// A type-0 block (RFC 1951 §3.2.4): byte-aligned, a 16-bit length, its
/// one's complement, then that many literal bytes.
fn stored_block(bits: &mut Bits, buf: &mut [u8], mut out: usize) -> Result<usize, Error> {
    bits.align();
    let len = bits.take(16)? as usize;
    let nlen = bits.take(16)? as usize;
    if len != !nlen & 0xffff {
        return Err(Error::Corrupt);
    }
    for _ in 0..len {
        *buf.get_mut(out).ok_or(Error::NoRoom)? = bits.take(8)? as u8;
        out += 1;
    }
    Ok(out)
}

/// A type-2 block's own literal/length and distance tables (RFC 1951
/// §3.2.7): a third, small Huffman code describes the two real tables'
/// lengths, run-length encoded (repeat the previous length, or a run of
/// zeros).
fn dynamic_tables(bits: &mut Bits) -> Result<(Huff<288>, Huff<30>), Error> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let nlen = bits.take(5)? as usize + 257;
    let ndist = bits.take(5)? as usize + 1;
    let ncode = bits.take(4)? as usize + 4;
    if nlen > 286 || ndist > 30 {
        return Err(Error::Corrupt);
    }
    let mut cl = [0u8; 19];
    for &i in &ORDER[..ncode] {
        cl[i] = bits.take(3)? as u8;
    }
    let clh: Huff<19> = Huff::new(&cl)?;
    let mut lens = [0u8; 316];
    let mut i = 0;
    while i < nlen + ndist {
        let sym = clh.decode(bits)?;
        if sym < 16 {
            lens[i] = sym as u8;
            i += 1;
            continue;
        }
        let (val, rep) = match sym {
            16 => {
                if i == 0 {
                    return Err(Error::Corrupt);
                }
                (lens[i - 1], 3 + bits.take(2)?)
            }
            17 => (0, 3 + bits.take(3)?),
            _ => (0, 11 + bits.take(7)?),
        };
        if i + rep as usize > nlen + ndist {
            return Err(Error::Corrupt);
        }
        for _ in 0..rep {
            lens[i] = val;
            i += 1;
        }
    }
    if lens[256] == 0 {
        // No end-of-block code: every block must terminate, so a table
        // missing one is corrupt rather than merely incomplete.
        return Err(Error::Corrupt);
    }
    Ok((
        Huff::new(&lens[..nlen])?,
        Huff::new(&lens[nlen..nlen + ndist])?,
    ))
}

/// The literal/length/distance loop both Huffman block kinds share (RFC 1951
/// §3.2.3): literals copy straight to output, length/distance pairs copy
/// `len` bytes from `d` back, and symbol 256 ends the block.
fn codes(
    bits: &mut Bits,
    buf: &mut [u8],
    mut out: usize,
    lit: &Huff<288>,
    dist: &Huff<30>,
) -> Result<usize, Error> {
    loop {
        let sym = lit.decode(bits)?;
        if sym < 256 {
            *buf.get_mut(out).ok_or(Error::NoRoom)? = sym as u8;
            out += 1;
            continue;
        }
        if sym == 256 {
            return Ok(out);
        }
        let s = usize::from(sym - 257);
        if s >= 29 {
            return Err(Error::Corrupt);
        }
        let len = usize::from(LEN_BASE[s]) + bits.take(u32::from(LEN_EXTRA[s]))? as usize;
        let ds = usize::from(dist.decode(bits)?);
        if ds >= 30 {
            return Err(Error::Corrupt);
        }
        let d = usize::from(DIST_BASE[ds]) + bits.take(u32::from(DIST_EXTRA[ds]))? as usize;
        if d > out {
            return Err(Error::FarBack);
        }
        if out + len > buf.len() {
            return Err(Error::NoRoom);
        }
        for k in 0..len {
            buf[out + k] = buf[out + k - d];
        }
        out += len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single stored block, final: RFC 1951 §3.2.4's simplest case,
    /// built by hand bit by bit.
    #[test]
    fn stored_block_round_trips() {
        // BFINAL=1, BTYPE=00, then align, LEN=3, NLEN=~3, "abc".
        let src = [0b0000_0001, 3, 0, !3u8, 0xff, b'a', b'b', b'c'];
        let mut buf = [0u8; 8];
        let n = inflate(&src, &mut buf, 0).unwrap();
        assert_eq!(&buf[..n], b"abc");
    }

    #[test]
    fn fixed_block_decodes_a_literal_run_and_a_backreference() {
        // "aaaa": one literal 'a' (RFC 1951's fixed table: 0-143 at 8 bits,
        // code = 0x30 + value), then a length-3/distance-1 match that
        // copies it three more times, then end-of-block (symbol 256, 7
        // bits, value 0).
        let mut w = BitWriter::default();
        w.put(1, 1); // BFINAL
        w.put(0b01, 2); // BTYPE=01 (fixed)
        w.code(0x30 + u32::from(b'a'), 8); // literal 'a'
        w.code(1, 7); // length code 257 (base 3, 0 extra bits)
        w.code(0, 5); // distance code 0 (base 1, 0 extra bits)
        w.code(0, 7); // end-of-block
        let src = w.finish();
        let mut buf = [0u8; 8];
        let n = inflate(&src, &mut buf, 0).unwrap();
        assert_eq!(&buf[..n], b"aaaa");
    }

    #[test]
    fn bad_block_type_is_corrupt() {
        // BFINAL=1, BTYPE=11 (reserved).
        let src = [0b0000_0111];
        let mut buf = [0u8; 8];
        assert_eq!(inflate(&src, &mut buf, 0).unwrap_err(), Error::Corrupt);
    }

    #[test]
    fn stored_block_with_mismatched_complement_is_corrupt() {
        let src = [0b0000_0001, 3, 0, 0, 0, b'a', b'b', b'c'];
        let mut buf = [0u8; 8];
        assert_eq!(inflate(&src, &mut buf, 0).unwrap_err(), Error::Corrupt);
    }

    #[test]
    fn stored_block_overflowing_the_buffer_is_no_room() {
        let src = [0b0000_0001, 3, 0, !3u8, 0xff, b'a', b'b', b'c'];
        let mut buf = [0u8; 2];
        assert_eq!(inflate(&src, &mut buf, 0).unwrap_err(), Error::NoRoom);
    }

    #[test]
    fn truncated_stored_block_is_truncated() {
        let src = [0b0000_0001, 3, 0, !3u8, 0xff, b'a'];
        let mut buf = [0u8; 8];
        assert_eq!(inflate(&src, &mut buf, 0).unwrap_err(), Error::Truncated);
    }

    #[test]
    fn empty_input_is_truncated() {
        let mut buf = [0u8; 8];
        assert_eq!(inflate(&[], &mut buf, 0).unwrap_err(), Error::Truncated);
    }

    #[test]
    fn a_distance_reaching_before_the_buffer_start_is_far_back() {
        // Fixed block: literal-free, a single length/distance pair whose
        // distance (1) is further back than anything written (out == start
        // == 0): BFINAL=1, BTYPE=01 (fixed), length code 257 (symbol 257,
        // 7 bits, RFC 1951's fixed table assigns it the 7-bit value 1),
        // distance code 0 (5 bits, value 0).
        let mut w = BitWriter::default();
        w.put(1, 1); // BFINAL
        w.put(0b01, 2); // BTYPE=01 (fixed), LSB-first
        w.code(1, 7); // length code 257
        w.code(0, 5); // distance code 0
        let src = w.finish();
        let mut buf = [0u8; 8];
        assert_eq!(inflate(&src, &mut buf, 0).unwrap_err(), Error::FarBack);
    }

    /// A minimal LSB-first bit writer, test-only: packs bits exactly as
    /// `Bits` unpacks them, into a fixed-size array (no alloc), so
    /// hand-built streams can be expressed as a sequence of (value, width)
    /// pairs instead of raw bytes.
    #[derive(Default)]
    struct BitWriter {
        bytes: [u8; 8],
        len: usize,
        acc: u32,
        n: u32,
    }

    impl BitWriter {
        fn put(&mut self, v: u32, k: u32) {
            self.acc |= v << self.n;
            self.n += k;
            while self.n >= 8 {
                self.bytes[self.len] = self.acc as u8;
                self.len += 1;
                self.acc >>= 8;
                self.n -= 8;
            }
        }

        /// A Huffman code's `k`-bit value, written most-significant-bit
        /// first (RFC 1951 §3.2.2), i.e. bit-reversed before going into the
        /// LSB-first stream.
        fn code(&mut self, code: u32, k: u32) {
            self.put(code.reverse_bits() >> (32 - k), k);
        }

        fn finish(mut self) -> [u8; 8] {
            if self.n > 0 {
                self.put(0, 8 - self.n);
            }
            self.bytes
        }
    }
}
