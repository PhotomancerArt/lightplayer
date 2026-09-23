//! Framing a binary frame on a byte stream that also carries console text.
//!
//! Proposal: `0x00 'B' COBS(payload) 0x00`. Console log lines never contain a
//! NUL byte, and COBS output never does either, so a reader in text mode that
//! meets `0x00` knows a binary frame starts, and the next `0x00` ends it — even
//! after a torn write (the reader resyncs at the next NUL). `'B'` leaves room
//! for other frame kinds. Overhead: 3 bytes + 1 per 254 payload bytes.

/// Encoded size of a payload of `n` bytes, framing included.
pub fn framed_len(n: usize) -> usize {
    3 + n + n / 254 + 1
}

/// COBS-encode `payload` into `out` inside the frame markers. Returns the
/// framed length, or `None` if `out` is too small.
pub fn encode(payload: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut w = 0;
    let put = |out: &mut [u8], i: usize, b: u8| -> Option<()> {
        *out.get_mut(i)? = b;
        Some(())
    };
    put(out, w, 0)?;
    put(out, w + 1, b'B')?;
    w += 2;
    let mut code_at = w;
    w += 1;
    let mut code: u8 = 1;
    for &b in payload {
        if b == 0 {
            put(out, code_at, code)?;
            code_at = w;
            w += 1;
            code = 1;
        } else {
            put(out, w, b)?;
            w += 1;
            code += 1;
            if code == 0xFF {
                put(out, code_at, code)?;
                code_at = w;
                w += 1;
                code = 1;
            }
        }
    }
    put(out, code_at, code)?;
    put(out, w, 0)?;
    Some(w + 1)
}

/// Decode a COBS body (between the markers) into `out`, returning its length.
pub fn decode(body: &[u8], out: &mut [u8]) -> Option<usize> {
    let (mut r, mut w) = (0, 0);
    while r < body.len() {
        let code = body[r] as usize;
        if code == 0 {
            return None;
        }
        r += 1;
        for _ in 1..code {
            *out.get_mut(w)? = *body.get(r)?;
            w += 1;
            r += 1;
        }
        if code < 0xFF && r < body.len() {
            *out.get_mut(w)? = 0;
            w += 1;
        }
    }
    Some(w)
}
