//! Framing a binary payload on a byte stream that also carries console text:
//! `0x00 kind COBS(payload) 0x00`.
//!
//! Console text never contains a NUL byte and COBS output never does either,
//! so a reader in text mode that meets `0x00` knows a frame starts, and the
//! next `0x00` ends it. A torn write resyncs at a NUL. The kind byte leaves
//! room for other frame kinds; a JSON Pack frame is [`FRAME_KIND_PACK`].
//!
//! Overhead: three bytes, plus one per 254 payload bytes. The payload may hold
//! any byte, `0x0A` included, which is why a line splitter must learn frames
//! before it can see one.
//!
//! On the device the frame is built **in place**: the packed payload is
//! written at [`in_place_headroom`] into the frame buffer, then
//! [`frame_in_place`] encodes it forward from offset 0. The writer never
//! overtakes the reader, so no second buffer is needed.

/// The byte that opens and closes every frame.
pub const FRAME_DELIMITER: u8 = 0x00;

/// The kind byte of a JSON Pack frame.
pub const FRAME_KIND_PACK: u8 = b'P';

/// Largest framed size of an `n`-byte payload.
pub fn max_framed_len(n: usize) -> usize {
    3 + n + n / 254 + 1
}

/// Where to write a payload of at most `max_payload` bytes so that
/// [`frame_in_place`] can frame it into the same buffer from offset 0.
pub fn in_place_headroom(max_payload: usize) -> usize {
    4 + max_payload / 254
}

/// COBS-frame `payload` into `out` as `0x00 kind COBS 0x00`. Returns the framed
/// length, or `None` if `out` is too small.
pub fn frame(kind: u8, payload: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut w = CobsWriter::start(out, kind)?;
    for &b in payload {
        w.byte(b)?;
    }
    w.end()
}

/// Frame `buf[src_at..src_at + n]` into `buf[0..]` in place, returning the
/// framed length. `None` if `src_at` is below [`in_place_headroom`]`(n)` or the
/// payload does not lie inside `buf`. The framed bytes never extend past
/// `src_at + n`, so the buffer needs no room beyond the payload.
pub fn frame_in_place(buf: &mut [u8], kind: u8, src_at: usize, n: usize) -> Option<usize> {
    let end = src_at.checked_add(n)?;
    if src_at < in_place_headroom(n) || end > buf.len() {
        return None;
    }
    let mut w = CobsWriter::start(buf, kind)?;
    for r in src_at..end {
        let b = w.out[r];
        w.byte(b)?;
    }
    w.end()
}

/// Decode a COBS body (the bytes between `kind` and the closing `0x00`) into
/// `out`, returning the payload length. `None` if the body is not valid COBS
/// or `out` is too small. (An empty body is not valid COBS: the empty payload
/// encodes as `[0x01]`.)
pub fn cobs_decode(body: &[u8], out: &mut [u8]) -> Option<usize> {
    if body.is_empty() {
        return None;
    }
    let (mut r, mut w) = (0, 0);
    while r < body.len() {
        let code = usize::from(body[r]);
        if code == 0 {
            return None;
        }
        r += 1;
        let run = body.get(r..r + code - 1)?;
        out.get_mut(w..w + run.len())?.copy_from_slice(run);
        w += run.len();
        r += run.len();
        if code < 0xFF && r < body.len() {
            *out.get_mut(w)? = 0;
            w += 1;
        }
    }
    Some(w)
}

/// Decode a COBS body in place (the payload never outgrows its encoding),
/// returning the payload length, or `None` if the body is not valid COBS.
pub fn cobs_decode_in_place(buf: &mut [u8]) -> Option<usize> {
    if buf.is_empty() {
        return None;
    }
    let (mut r, mut w) = (0, 0);
    while r < buf.len() {
        let code = usize::from(buf[r]);
        if code == 0 {
            return None;
        }
        r += 1;
        let end = r.checked_add(code - 1).filter(|&e| e <= buf.len())?;
        buf.copy_within(r..end, w);
        w += end - r;
        r = end;
        if code < 0xFF && r < buf.len() {
            buf[w] = 0;
            w += 1;
        }
    }
    Some(w)
}

/// Forward COBS writer: the code byte of the current run is back-filled when
/// the run ends.
struct CobsWriter<'a> {
    out: &'a mut [u8],
    w: usize,
    code_at: usize,
    code: u8,
}

impl<'a> CobsWriter<'a> {
    fn start(out: &'a mut [u8], kind: u8) -> Option<Self> {
        if out.len() < 3 || kind == FRAME_DELIMITER {
            return None;
        }
        out[0] = FRAME_DELIMITER;
        out[1] = kind;
        Some(Self {
            out,
            w: 3,
            code_at: 2,
            code: 1,
        })
    }

    fn byte(&mut self, b: u8) -> Option<()> {
        if b == 0 {
            self.close_run()?;
        } else {
            *self.out.get_mut(self.w)? = b;
            self.w += 1;
            self.code += 1;
            if self.code == 0xFF {
                self.close_run()?;
            }
        }
        Some(())
    }

    fn close_run(&mut self) -> Option<()> {
        self.out[self.code_at] = self.code;
        self.code_at = self.w;
        // Reserve the next run's code byte.
        if self.w >= self.out.len() {
            return None;
        }
        self.w += 1;
        self.code = 1;
        Some(())
    }

    fn end(self) -> Option<usize> {
        self.out[self.code_at] = self.code;
        *self.out.get_mut(self.w)? = FRAME_DELIMITER;
        Some(self.w + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(n: usize, seed: usize) -> [u8; 1200] {
        core::array::from_fn(|i| {
            if i >= n {
                0
            } else if (i * 31 + seed) % 97 == 0 {
                0
            } else {
                ((i * 7 + seed) as u8) | 1
            }
        })
    }

    #[test]
    fn in_place_matches_copying_for_every_length() {
        for n in (0..=1000).step_by(7).chain([253, 254, 255, 508, 509]) {
            for seed in [0, 3] {
                let src = pattern(n, seed);
                let payload = &src[..n];
                let mut a = [0u8; 1300];
                let na = frame(FRAME_KIND_PACK, payload, &mut a).unwrap();
                assert!(na <= max_framed_len(n), "{n}");
                assert!(!a[2..na - 1].contains(&0), "{n}");
                let at = in_place_headroom(n);
                let mut b = [0u8; 1300];
                b[at..at + n].copy_from_slice(payload);
                let nb = frame_in_place(&mut b[..at + n], FRAME_KIND_PACK, at, n).unwrap();
                assert_eq!(&a[..na], &b[..nb], "{n}");
                let mut back = [0u8; 1300];
                let m = cobs_decode(&a[2..na - 1], &mut back).unwrap();
                assert_eq!(&back[..m], payload, "{n}");
                let mut body = [0u8; 1300];
                body[..na - 3].copy_from_slice(&a[2..na - 1]);
                let m = cobs_decode_in_place(&mut body[..na - 3]).unwrap();
                assert_eq!(&body[..m], payload, "{n}");
            }
        }
    }

    #[test]
    fn all_nonzero_runs_of_254() {
        let payload = [0xAB; 254];
        let mut out = [0u8; 300];
        let n = frame(FRAME_KIND_PACK, &payload, &mut out).unwrap();
        assert_eq!(n, max_framed_len(254));
        let mut back = [0u8; 300];
        let m = cobs_decode(&out[2..n - 1], &mut back).unwrap();
        assert_eq!(&back[..m], &payload[..]);
    }

    #[test]
    fn too_small_is_none_not_a_panic() {
        let payload = pattern(300, 1);
        let mut out = [0u8; 400];
        let exact = frame(FRAME_KIND_PACK, &payload[..300], &mut out).unwrap();
        for size in 0..exact {
            assert_eq!(
                frame(FRAME_KIND_PACK, &payload[..300], &mut out[..size]),
                None
            );
        }
        assert_eq!(
            frame(FRAME_KIND_PACK, &payload[..300], &mut out[..exact]),
            Some(exact)
        );
        let mut buf = [0u8; 16];
        assert_eq!(frame_in_place(&mut buf, FRAME_KIND_PACK, 2, 8), None);
        assert_eq!(frame_in_place(&mut buf, FRAME_KIND_PACK, 4, 13), None);
    }

    #[test]
    fn bad_cobs_is_none() {
        let mut out = [0u8; 16];
        assert_eq!(cobs_decode(&[0], &mut out), None);
        assert_eq!(cobs_decode(&[], &mut out), None);
        assert_eq!(cobs_decode(&[5, 1, 2], &mut out), None);
        assert_eq!(cobs_decode_in_place(&mut [5, 1, 2]), None);
        assert_eq!(cobs_decode_in_place(&mut [2, 1, 0]), None);
    }
}
