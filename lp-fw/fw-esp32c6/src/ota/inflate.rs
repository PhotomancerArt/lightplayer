//! Raw deflate (RFC 1951) decoding for the update path: an update's chunks
//! arrive compressed (`Z`), each against the 32 KiB of the image the board
//! wrote just before it ([`super::update_window`]).
//!
//! Spike: written from RFC 1951 (tested against miniz_oxide in the scratch
//! prototype `tinyflate`). Huffman codes are decoded a bit at a time, the
//! smallest form; the link, not this, is the update's bottleneck. If this
//! becomes a shared `lp-base` crate, it moves there whole.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The input ended inside a block.
    Truncated,
    /// A block type of 3, a bad stored length, or an impossible code.
    Corrupt,
    /// The output buffer is full.
    NoRoom,
    /// A distance reaching before the start of the buffer.
    FarBack,
}

// ---------------------------------------------------------------- decoding

struct Bits<'a> {
    src: &'a [u8],
    pos: usize,
    acc: u32,
    n: u32,
}

impl<'a> Bits<'a> {
    fn need(&mut self, k: u32) -> Result<(), Error> {
        while self.n < k {
            let b = *self.src.get(self.pos).ok_or(Error::Truncated)?;
            self.pos += 1;
            self.acc |= u32::from(b) << self.n;
            self.n += 8;
        }
        Ok(())
    }

    fn take(&mut self, k: u32) -> Result<u32, Error> {
        if k == 0 {
            return Ok(0);
        }
        self.need(k)?;
        let v = self.acc & ((1 << k) - 1);
        self.acc >>= k;
        self.n -= k;
        Ok(v)
    }

    fn align(&mut self) {
        let drop = self.n % 8;
        self.acc >>= drop;
        self.n -= drop;
    }
}

/// A canonical Huffman code as counts per length and symbols in code order:
/// decoded one bit at a time (small, and fast enough off the hot path).
struct Huff<const N: usize> {
    count: [u16; 16],
    symbol: [u16; N],
}

impl<const N: usize> Huff<N> {
    fn new(lengths: &[u8]) -> Result<Self, Error> {
        let mut h = Huff {
            count: [0; 16],
            symbol: [0; N],
        };
        for &l in lengths {
            h.count[usize::from(l)] += 1;
        }
        h.count[0] = 0;
        // Over-subscribed codes are corrupt; incomplete ones are allowed (RFC
        // permits a single distance code).
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

    fn decode(&self, bits: &mut Bits) -> Result<u16, Error> {
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

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049,
    3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13,
];

fn fixed_tables() -> (Huff<288>, Huff<30>) {
    let mut l = [0u8; 288];
    l[..144].fill(8);
    l[144..256].fill(9);
    l[256..280].fill(7);
    l[280..].fill(8);
    let d = [5u8; 30];
    (Huff::new(&l).unwrap(), Huff::new(&d).unwrap())
}

/// Decode the raw deflate stream `src` into `buf[start..]`. `buf[..start]` is
/// a preset dictionary (history a match may reach into). Returns the number
/// of bytes written after `start`.
pub fn inflate(src: &[u8], buf: &mut [u8], start: usize) -> Result<usize, Error> {
    let mut bits = Bits {
        src,
        pos: 0,
        acc: 0,
        n: 0,
    };
    let mut out = start;
    loop {
        let last = bits.take(1)?;
        match bits.take(2)? {
            0 => {
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
            }
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

fn dynamic_tables(bits: &mut Bits) -> Result<(Huff<288>, Huff<30>), Error> {
    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
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
        return Err(Error::Corrupt);
    }
    Ok((Huff::new(&lens[..nlen])?, Huff::new(&lens[nlen..nlen + ndist])?))
}

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
