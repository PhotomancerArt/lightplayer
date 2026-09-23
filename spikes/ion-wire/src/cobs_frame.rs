//! Framing a binary frame on a byte stream that also carries console text.
//!
//! Proposal: `0x00 'B' COBS(payload) 0x00`. Console log lines never contain a
//! NUL byte, and COBS output never does either, so a reader in text mode that
//! meets `0x00` knows a binary frame starts, and the next `0x00` ends it — even
//! after a torn write (the reader resyncs at the next NUL). `'B'` leaves room
//! for other frame kinds. Overhead: 3 bytes + 1 per 254 payload bytes.

/// Worst-case framed size of an `n`-byte payload (exact when the payload has
/// no NUL within any 254-byte run).
pub fn max_framed_len(n: usize) -> usize {
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

/// Where to place a payload of at most `max_payload` bytes so that
/// [`encode_in_place`] can frame it forward into the same buffer from offset 0.
pub fn in_place_headroom(max_payload: usize) -> usize {
    4 + max_payload / 254
}

/// Frame `buf[src_at..src_at + n]` into `buf[0..]` in place. `src_at` must be at
/// least [`in_place_headroom`]`(n)`: the writer then never overtakes the reader.
pub fn encode_in_place(buf: &mut [u8], src_at: usize, n: usize) -> Option<usize> {
    if src_at < in_place_headroom(n) || src_at + n > buf.len() {
        return None;
    }
    buf[0] = 0;
    buf[1] = b'B';
    let mut code_at = 2;
    let mut w = 3;
    let mut code: u8 = 1;
    for r in src_at..src_at + n {
        let b = buf[r];
        if b == 0 {
            buf[code_at] = code;
            code_at = w;
            w += 1;
            code = 1;
        } else {
            buf[w] = b;
            w += 1;
            code += 1;
            if code == 0xFF {
                buf[code_at] = code;
                code_at = w;
                w += 1;
                code = 1;
            }
        }
    }
    buf[code_at] = code;
    *buf.get_mut(w)? = 0;
    Some(w + 1)
}
