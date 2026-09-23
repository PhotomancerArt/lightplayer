//! LEB128 (7 bits per byte, low group first, high bit = more) and zigzag.
//!
//! Ion 1.0's VarUInt is big-endian with the stop bit on the *last* byte; LEB128 is
//! what Ion 1.1's FlexUInt family and most of the ecosystem moved to, and it
//! decodes with one loop. The byte count is identical.

/// Bytes needed for `v`.
pub fn len(mut v: u64) -> usize {
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}

/// Write `v` into `out`, returning the byte count. `out` must hold [`len`] bytes.
pub fn write(out: &mut [u8], mut v: u64) -> usize {
    let mut i = 0;
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out[i] = b;
            return i + 1;
        }
        out[i] = b | 0x80;
        i += 1;
    }
}

/// Read a LEB128 value from `input[*pos..]`, advancing `pos`.
pub fn read(input: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v: u64 = 0;
    let mut shift = 0;
    loop {
        let b = *input.get(*pos)?;
        *pos += 1;
        if shift >= 64 {
            return None;
        }
        v |= u64::from(b & 0x7F) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
    }
}

pub fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

pub fn unzigzag(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}
