//! LEB128 varints (7 bits per byte, low group first, high bit = more) and zigzag.
//!
//! Ion 1.0's VarUInt is big-endian with the stop bit on the *last* byte. LEB128
//! is what Ion 1.1's FlexUInt family and most of the ecosystem use, and it
//! decodes with one loop. The byte count is the same.

/// Longest LEB128 encoding of a `u64`.
pub const VARINT_MAX_LEN: usize = 10;

/// Bytes needed to write `v`.
pub fn varint_len(mut v: u64) -> usize {
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}

/// Write `v` into `out`, returning the byte count. `out` must hold
/// [`varint_len`]`(v)` bytes; [`VARINT_MAX_LEN`] always suffices.
pub fn write_varint(out: &mut [u8], mut v: u64) -> usize {
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

/// Read a varint from `input[*pos..]`, advancing `pos`. `None` when the input
/// ends first or the value overflows a `u64`.
pub fn read_varint(input: &[u8], pos: &mut usize) -> Option<u64> {
    let mut v: u64 = 0;
    let mut shift = 0u32;
    loop {
        let b = *input.get(*pos)?;
        *pos += 1;
        let group = u64::from(b & 0x7F);
        if shift >= 64 || (shift == 63 && group > 1) {
            return None;
        }
        v |= group << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
    }
}

/// Zigzag: small magnitudes of either sign become small unsigned values.
pub fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

/// Inverse of [`zigzag`].
pub fn unzigzag(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_edges() {
        for v in [
            0,
            1,
            0x7F,
            0x80,
            0x3FFF,
            0x4000,
            u64::from(u32::MAX),
            u64::MAX,
        ] {
            let mut buf = [0u8; VARINT_MAX_LEN];
            let n = write_varint(&mut buf, v);
            assert_eq!(n, varint_len(v));
            let mut pos = 0;
            assert_eq!(read_varint(&buf[..n], &mut pos), Some(v));
            assert_eq!(pos, n);
        }
    }

    #[test]
    fn truncated_and_overlong_fail() {
        let mut pos = 0;
        assert_eq!(read_varint(&[0x80], &mut pos), None);
        let mut pos = 0;
        assert_eq!(read_varint(&[0xFF; 11], &mut pos), None);
        let mut pos = 0;
        let too_big = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x02];
        assert_eq!(read_varint(&too_big, &mut pos), None);
    }

    #[test]
    fn zigzag_round_trips() {
        for v in [0, -1, 1, -64, 63, i64::MIN, i64::MAX] {
            assert_eq!(unzigzag(zigzag(v)), v);
        }
        assert_eq!(zigzag(-1), 1);
        assert_eq!(zigzag(1), 2);
    }
}
