//! Canonical standard-alphabet base64 (padded), checked and decoded in place.
//!
//! The wire's pixel and sample payloads are base64 strings in JSON (+33 %).
//! LPBJ carries them raw. Only *canonical* base64 is converted (length a
//! multiple of 4, `=` padding only at the end, unused bits zero), which is the
//! exact condition under which re-encoding reproduces the text.

fn sextet(c: u8) -> Option<u8> {
    Some(match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    })
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Whether `text` is canonical padded base64 (the empty string is not taken:
/// it is cheaper as an inline string).
pub fn is_canonical(text: &[u8]) -> bool {
    if text.is_empty() || text.len() % 4 != 0 {
        return false;
    }
    let pad = text.iter().rev().take_while(|&&c| c == b'=').count();
    if pad > 2 {
        return false;
    }
    let body = &text[..text.len() - pad];
    if !body.iter().all(|&c| sextet(c).is_some()) {
        return false;
    }
    // Unused low bits of the last sextet must be zero.
    let last = sextet(body[body.len() - 1]).unwrap_or(0);
    match pad {
        1 => last & 0b11 == 0,
        2 => last & 0b1111 == 0,
        _ => true,
    }
}

/// Decode canonical base64 in place (output never overtakes input), returning
/// the decoded length. Call only after [`is_canonical`].
pub fn decode_in_place(buf: &mut [u8]) -> usize {
    let mut w = 0;
    let mut acc: u32 = 0;
    let mut bits = 0;
    for r in 0..buf.len() {
        let c = buf[r];
        if c == b'=' {
            break;
        }
        acc = (acc << 6) | u32::from(sextet(c).unwrap_or(0));
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            buf[w] = (acc >> bits) as u8;
            w += 1;
        }
    }
    w
}

/// Encode `raw` as padded base64 through `emit`.
pub fn encode(raw: &[u8], emit: &mut dyn FnMut(&[u8])) {
    for chunk in raw.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let mut q = [b'='; 4];
        q[0] = ALPHABET[(n >> 18) as usize & 63];
        q[1] = ALPHABET[(n >> 12) as usize & 63];
        if chunk.len() > 1 {
            q[2] = ALPHABET[(n >> 6) as usize & 63];
        }
        if chunk.len() > 2 {
            q[3] = ALPHABET[n as usize & 63];
        }
        emit(&q);
    }
}
