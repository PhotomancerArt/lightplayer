//! Consistent Overhead Byte Stuffing (COBS): rewrites a byte string so it holds
//! no `0x00`, at a cost of one byte per 254 (plus one). The link then uses
//! `0x00` as the frame delimiter, and a lost byte can only damage the frame it
//! sits in: the next `0x00` always starts clean.
//!
//! Written from the published algorithm (Cheshire & Baker, 1999).
//!
//! # COBS-FF: no `0xFF` on the wire either
//!
//! The link's stream framing uses [`encode_no_ff_into`], which keeps `0xFF`
//! off the wire as well as `0x00`. Chromium opens a macOS tty with `PARMRK`
//! set and `IGNBRK` clear, so the kernel doubles every `0xFF` data byte, and
//! the macOS serial driver's free-space arithmetic then wraps and drops ~1 KB
//! runs (plan `reliable-device-link`, M1: 60 MB through a raw-termios reader
//! lost nothing; packed frames, ~1% `0xFF`, lost through Web Serial; JSON,
//! with none, never did). A frame with no `0xFF` never meets that path.
//!
//! Two steps, both invertible:
//!
//! 1. escape: `0xFE` → `0xFE 0x01`, `0xFF` → `0xFE 0x00`;
//! 2. COBS with the largest block code `0xFE` (253 data bytes) instead of
//!    `0xFF`, so no code byte is `0xFF`. The zeros step 1 made are absorbed
//!    into code bytes like any other zero.
//!
//! Cost on random data (a frame's CRC and compressed payloads): ~2 in 256
//! bytes escaped plus one code byte per 253, about 1.2% against plain COBS's
//! 0.4%. Worst case (all `0xFE`/`0xFF`) doubles.

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

/// The largest COBS-FF block code: a full block of 253 bytes, no zero after.
const FF_FULL: u8 = 0xFE;
/// The escape byte, and what follows it.
const ESC: u8 = 0xFE;
const ESC_FF: u8 = 0x00;
const ESC_FE: u8 = 0x01;

/// Append the COBS-FF encoding of `src` to `out` (module docs). The appended
/// bytes hold no `0x00` and no `0xFF`.
pub fn encode_no_ff_into(src: &[u8], out: &mut Vec<u8>) {
    let mut code_at = out.len();
    out.push(0);
    let mut code: u8 = 1;
    let mut put = |b: u8, out: &mut Vec<u8>| {
        if b == 0 {
            out[code_at] = code;
            code_at = out.len();
            out.push(0);
            code = 1;
        } else {
            out.push(b);
            code += 1;
            if code == FF_FULL {
                out[code_at] = code;
                code_at = out.len();
                out.push(0);
                code = 1;
            }
        }
    };
    for &b in src {
        match b {
            0xFF => {
                put(ESC, out);
                put(ESC_FF, out);
            }
            0xFE => {
                put(ESC, out);
                put(ESC_FE, out);
            }
            _ => put(b, out),
        }
    }
    out[code_at] = code;
}

/// Append the decoding of COBS-FF `src` to `out`.
pub fn decode_no_ff_into(src: &[u8], out: &mut Vec<u8>) -> Result<(), CobsError> {
    let start = out.len();
    let mut i = 0;
    while i < src.len() {
        let code = src[i];
        if code == 0 || code == 0xFF {
            return Err(CobsError);
        }
        i += 1;
        let n = code as usize - 1;
        if i + n > src.len() {
            return Err(CobsError);
        }
        let block = &src[i..i + n];
        if block.contains(&0xFF) {
            return Err(CobsError);
        }
        out.extend_from_slice(block);
        i += n;
        if code != FF_FULL && i < src.len() {
            out.push(0);
        }
    }
    // Undo the escape in place.
    let (mut r, mut w) = (start, start);
    while r < out.len() {
        let b = out[r];
        if b == ESC {
            let v = match out.get(r + 1) {
                Some(&ESC_FF) => 0xFF,
                Some(&ESC_FE) => 0xFE,
                _ => return Err(CobsError),
            };
            out[w] = v;
            r += 2;
        } else {
            out[w] = b;
            r += 1;
        }
        w += 1;
    }
    out.truncate(w);
    Ok(())
}

/// Largest COBS-FF encoding of an `n`-byte input (every byte escaped).
pub const fn max_encoded_no_ff_len(n: usize) -> usize {
    let m = 2 * n;
    m + m / 253 + 1
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
    fn cobs_ff_round_trips_and_holds_neither_zero_nor_ff() {
        let mut cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![0],
            vec![0xFF],
            vec![0xFE],
            vec![0xFE, 0x00, 0xFF, 0x01],
            vec![0xFF; 600],
            vec![0xFE; 600],
            (0..=255u8).collect(),
            (1..=253u8).collect(),
            (1..=254u8).collect(),
            (0..2000).map(|i| (i * 37 % 256) as u8).collect(),
        ];
        let mut x = 0x1234_5678u32;
        for len in [1usize, 252, 253, 254, 255, 506, 4096] {
            cases.push(
                (0..len)
                    .map(|_| {
                        x ^= x << 13;
                        x ^= x >> 17;
                        x ^= x << 5;
                        x as u8
                    })
                    .collect(),
            );
        }
        for src in cases {
            let mut enc = Vec::new();
            encode_no_ff_into(&src, &mut enc);
            assert!(
                !enc.contains(&0),
                "a zero in the encoding of {} bytes",
                src.len()
            );
            assert!(
                !enc.contains(&0xFF),
                "a 0xFF in the encoding of {} bytes",
                src.len()
            );
            assert!(enc.len() <= max_encoded_no_ff_len(src.len()));
            let mut dec = vec![9, 9];
            decode_no_ff_into(&enc, &mut dec).unwrap();
            assert_eq!(&dec[2..], &src[..], "{} bytes", src.len());
        }
    }

    #[test]
    fn cobs_ff_overhead_on_random_bytes_is_about_one_percent() {
        let mut x = 0x9E37_79B9u32;
        let src: Vec<u8> = (0..1_000_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 7) as u8
            })
            .collect();
        let (mut plain, mut ff) = (Vec::new(), Vec::new());
        encode_into(&src, &mut plain);
        encode_no_ff_into(&src, &mut ff);
        let plain_pct = (plain.len() - src.len()) as f64 * 100.0 / src.len() as f64;
        let ff_pct = (ff.len() - src.len()) as f64 * 100.0 / src.len() as f64;
        assert!(plain_pct < 0.5, "{plain_pct}");
        assert!(ff_pct > 0.9 && ff_pct < 1.4, "{ff_pct}");
    }

    #[test]
    fn cobs_ff_rejects_malformed() {
        let mut out = Vec::new();
        assert_eq!(decode_no_ff_into(&[0xFF, 1], &mut out), Err(CobsError));
        assert_eq!(
            decode_no_ff_into(&[0x03, 0xFF, 1], &mut out),
            Err(CobsError)
        );
        // An escape with nothing (or something foreign) after it.
        assert_eq!(decode_no_ff_into(&[0x02, 0xFE], &mut out), Err(CobsError));
        assert_eq!(
            decode_no_ff_into(&[0x03, 0xFE, 0x07], &mut out),
            Err(CobsError)
        );
    }

    #[test]
    fn rejects_malformed() {
        let mut out = Vec::new();
        assert_eq!(decode_into(&[0x05, 1, 2], &mut out), Err(CobsError));
        assert_eq!(decode_into(&[0x00], &mut out), Err(CobsError));
    }
}
