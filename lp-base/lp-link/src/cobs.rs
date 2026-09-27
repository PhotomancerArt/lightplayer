//! Consistent Overhead Byte Stuffing (COBS): rewrites a byte string so it holds
//! no `0x00`, at a cost of one byte per 254 (plus one). The link then uses
//! `0x00` as the frame delimiter, and a lost byte can only damage the frame it
//! sits in: the next `0x00` always starts clean.
//!
//! Written from the published algorithm (Cheshire & Baker, 1999).

use alloc::vec::Vec;

/// The input is not valid COBS (a zero code byte, or a block running past the
/// end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CobsError;

/// Append the COBS encoding of `src` to `out`. The appended bytes hold no zero.
pub fn encode_into(src: &[u8], out: &mut Vec<u8>) {
    let mut code_at = out.len();
    out.push(0);
    let mut code: u8 = 1;
    for &b in src {
        if b == 0 {
            out[code_at] = code;
            code_at = out.len();
            out.push(0);
            code = 1;
        } else {
            out.push(b);
            code += 1;
            if code == 0xFF {
                out[code_at] = code;
                code_at = out.len();
                out.push(0);
                code = 1;
            }
        }
    }
    out[code_at] = code;
}

/// Append the decoding of `src` (which must hold no zero) to `out`.
pub fn decode_into(src: &[u8], out: &mut Vec<u8>) -> Result<(), CobsError> {
    let mut i = 0;
    while i < src.len() {
        let code = src[i];
        if code == 0 {
            return Err(CobsError);
        }
        i += 1;
        let n = code as usize - 1;
        if i + n > src.len() {
            return Err(CobsError);
        }
        out.extend_from_slice(&src[i..i + n]);
        i += n;
        if code != 0xFF && i < src.len() {
            out.push(0);
        }
    }
    Ok(())
}

/// Largest encoding of an `n`-byte input.
pub const fn max_encoded_len(n: usize) -> usize {
    n + n / 254 + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn round_trips_edge_cases() {
        let cases: [Vec<u8>; 7] = [
            vec![],
            vec![0],
            vec![0, 0],
            vec![1, 2, 3],
            vec![0x11, 0, 0x22, 0],
            (1..=254u8).collect(),
            (0..600).map(|i| (i % 7) as u8).collect(),
        ];
        for src in cases {
            let mut enc = Vec::new();
            encode_into(&src, &mut enc);
            assert!(!enc.contains(&0), "encoding of {src:?} holds a zero");
            assert!(enc.len() <= max_encoded_len(src.len()));
            let mut dec = Vec::new();
            decode_into(&enc, &mut dec).unwrap();
            assert_eq!(dec, src);
        }
    }

    #[test]
    fn known_vectors() {
        let mut enc = Vec::new();
        encode_into(&[0x11, 0x22, 0x00, 0x33], &mut enc);
        assert_eq!(enc, [0x03, 0x11, 0x22, 0x02, 0x33]);
    }

    #[test]
    fn rejects_malformed() {
        let mut out = Vec::new();
        assert_eq!(decode_into(&[0x05, 1, 2], &mut out), Err(CobsError));
        assert_eq!(decode_into(&[0x00], &mut out), Err(CobsError));
    }
}
