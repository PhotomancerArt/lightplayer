//! A byte-stream scanner that separates COBS frames from the text around them.
//!
//! It knows nothing of lines or of `M!`: bytes outside frames come out as
//! [`ScanEvent::Text`] slices for the caller to split, and each completed frame
//! comes out COBS-decoded as [`ScanEvent::Frame`]. Push bytes as they arrive,
//! in slices of any size; a frame may span any number of pushes.
//!
//! Resync rules, so one torn frame costs at most itself:
//! - `0x00 0x00` is not a frame: the second `0x00` starts one.
//! - A frame whose body is not valid COBS is reported as
//!   [`ScanEvent::Dropped`], and its closing `0x00` is taken as the *start* of
//!   the next frame. That is the torn-write case: a board reset mid-frame
//!   leaves a start with no end, the boot text runs into the body, and the
//!   next real frame's opening `0x00` looks like this one's end.
//! - A frame that outgrows the buffer is reported as [`ScanEvent::Dropped`] and
//!   skipped to its closing `0x00`.
//!
//! A torn frame whose swallowed text happens to be valid COBS comes out as a
//! `Frame` with a garbage payload (the pack decoder rejects it), and the frame
//! after it is lost with it; valid COBS from console text is rare, but not
//! impossible.

use crate::cobs_frame::{FRAME_DELIMITER, cobs_decode_in_place};

/// What the scanner found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanEvent<'a> {
    /// Bytes outside any frame, in stream order. Not split into lines.
    Text(&'a [u8]),
    /// A complete frame: its kind byte and its COBS-decoded payload.
    Frame { kind: u8, payload: &'a [u8] },
    /// A frame that could not be delivered.
    Dropped(DropReason),
}

/// Why a frame was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    /// Its body was not valid COBS (a torn write).
    BadCobs,
    /// It outgrew the scanner's buffer.
    TooLong,
}

/// Where a frame's body is held while it arrives.
pub trait FrameBuffer {
    /// Forget the current body.
    fn clear(&mut self);
    /// Append to the body; `false` when there is no room.
    fn extend(&mut self, bytes: &[u8]) -> bool;
    /// The body so far, mutable (it is COBS-decoded in place).
    fn body_mut(&mut self) -> &mut [u8];
}

/// A [`FrameBuffer`] over a caller-owned slice: no heap.
pub struct SliceFrameBuffer<'b> {
    buf: &'b mut [u8],
    len: usize,
}

impl<'b> SliceFrameBuffer<'b> {
    /// Hold frame bodies of up to `buf.len()` bytes.
    pub fn new(buf: &'b mut [u8]) -> Self {
        Self { buf, len: 0 }
    }
}

impl FrameBuffer for SliceFrameBuffer<'_> {
    fn clear(&mut self) {
        self.len = 0;
    }

    fn extend(&mut self, bytes: &[u8]) -> bool {
        let end = self.len + bytes.len();
        match self.buf.get_mut(self.len..end) {
            Some(dst) => {
                dst.copy_from_slice(bytes);
                self.len = end;
                true
            }
            None => false,
        }
    }

    fn body_mut(&mut self) -> &mut [u8] {
        &mut self.buf[..self.len]
    }
}

/// A growable [`FrameBuffer`] with a size cap (feature `alloc`).
#[cfg(feature = "alloc")]
pub struct VecFrameBuffer {
    buf: alloc::vec::Vec<u8>,
    max: usize,
}

#[cfg(feature = "alloc")]
impl VecFrameBuffer {
    /// Hold frame bodies of up to `max` bytes.
    pub fn new(max: usize) -> Self {
        Self {
            buf: alloc::vec::Vec::new(),
            max,
        }
    }
}

#[cfg(feature = "alloc")]
impl FrameBuffer for VecFrameBuffer {
    fn clear(&mut self) {
        self.buf.clear();
    }

    fn extend(&mut self, bytes: &[u8]) -> bool {
        if self.buf.len() + bytes.len() > self.max {
            return false;
        }
        self.buf.extend_from_slice(bytes);
        true
    }

    fn body_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }
}

/// The owning scanner hosts use (feature `alloc`).
#[cfg(feature = "alloc")]
pub type VecFrameScanner = FrameScanner<VecFrameBuffer>;

#[cfg(feature = "alloc")]
impl VecFrameScanner {
    /// A scanner holding frames of up to `max_body` COBS bytes.
    pub fn with_max_body(max_body: usize) -> Self {
        FrameScanner::new(VecFrameBuffer::new(max_body))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Outside a frame.
    Text,
    /// Just read a frame's opening `0x00`; the kind byte is next.
    Kind,
    /// Inside a frame's body.
    Body,
    /// Inside a frame that outgrew the buffer: skipping to its `0x00`.
    Skip,
}

/// Splits a byte stream into text and frames. See the module docs.
pub struct FrameScanner<B: FrameBuffer> {
    buf: B,
    state: State,
    kind: u8,
}

impl<B: FrameBuffer> FrameScanner<B> {
    /// A scanner holding frame bodies in `buf`.
    pub fn new(buf: B) -> Self {
        Self {
            buf,
            state: State::Text,
            kind: 0,
        }
    }

    /// Whether the scanner is partway through a frame.
    pub fn in_frame(&self) -> bool {
        self.state != State::Text
    }

    /// Scan `input`, calling `on` for each event in stream order.
    pub fn push(&mut self, input: &[u8], mut on: impl FnMut(ScanEvent<'_>)) {
        let mut i = 0;
        while i < input.len() {
            match self.state {
                State::Text => {
                    let rest = &input[i..];
                    let Some(p) = rest.iter().position(|&b| b == FRAME_DELIMITER) else {
                        on(ScanEvent::Text(rest));
                        return;
                    };
                    if p > 0 {
                        on(ScanEvent::Text(&rest[..p]));
                    }
                    i += p + 1;
                    self.state = State::Kind;
                }
                State::Kind => {
                    let b = input[i];
                    i += 1;
                    if b != FRAME_DELIMITER {
                        self.kind = b;
                        self.buf.clear();
                        self.state = State::Body;
                    }
                    // A second 0x00 starts the frame again: stay in Kind.
                }
                State::Body | State::Skip => {
                    let rest = &input[i..];
                    let end = rest.iter().position(|&b| b == FRAME_DELIMITER);
                    let chunk = &rest[..end.unwrap_or(rest.len())];
                    if self.state == State::Body && !self.buf.extend(chunk) {
                        self.buf.clear();
                        self.state = State::Skip;
                        on(ScanEvent::Dropped(DropReason::TooLong));
                    }
                    let Some(end) = end else {
                        return;
                    };
                    i += end + 1;
                    if self.state == State::Skip {
                        self.state = State::Text;
                        continue;
                    }
                    let kind = self.kind;
                    let body = self.buf.body_mut();
                    match cobs_decode_in_place(body) {
                        Some(n) => {
                            on(ScanEvent::Frame {
                                kind,
                                payload: &body[..n],
                            });
                            self.state = State::Text;
                        }
                        None => {
                            on(ScanEvent::Dropped(DropReason::BadCobs));
                            // Torn: this 0x00 is the next frame's start.
                            self.state = State::Kind;
                        }
                    }
                    self.buf.clear();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cobs_frame::{FRAME_KIND_PACK, frame};

    /// Collects events as (tag, bytes) into fixed storage.
    struct Seen {
        text: [u8; 512],
        text_len: usize,
        frames: [([u8; 64], usize); 8],
        frame_count: usize,
        dropped: usize,
    }

    impl Seen {
        fn new() -> Self {
            Self {
                text: [0; 512],
                text_len: 0,
                frames: [([0; 64], 0); 8],
                frame_count: 0,
                dropped: 0,
            }
        }

        fn take(&mut self, e: ScanEvent<'_>) {
            match e {
                ScanEvent::Text(t) => {
                    self.text[self.text_len..self.text_len + t.len()].copy_from_slice(t);
                    self.text_len += t.len();
                }
                ScanEvent::Frame { kind, payload } => {
                    assert_eq!(kind, FRAME_KIND_PACK);
                    let f = &mut self.frames[self.frame_count];
                    f.0[..payload.len()].copy_from_slice(payload);
                    f.1 = payload.len();
                    self.frame_count += 1;
                }
                ScanEvent::Dropped(_) => self.dropped += 1,
            }
        }

        fn frame(&self, i: usize) -> &[u8] {
            &self.frames[i].0[..self.frames[i].1]
        }
    }

    fn framed(payload: &[u8]) -> ([u8; 128], usize) {
        let mut out = [0u8; 128];
        let n = frame(FRAME_KIND_PACK, payload, &mut out).unwrap();
        (out, n)
    }

    fn stream(parts: &[&[u8]]) -> ([u8; 512], usize) {
        let mut s = [0u8; 512];
        let mut n = 0;
        for p in parts {
            s[n..n + p.len()].copy_from_slice(p);
            n += p.len();
        }
        (s, n)
    }

    fn scan_in_steps(bytes: &[u8], step: usize) -> Seen {
        let mut body = [0u8; 64];
        let mut sc = FrameScanner::new(SliceFrameBuffer::new(&mut body));
        let mut seen = Seen::new();
        for chunk in bytes.chunks(step) {
            sc.push(chunk, |e| seen.take(e));
        }
        seen
    }

    #[test]
    fn frames_with_newlines_between_console_text_in_any_split() {
        let (f1, n1) = framed(b"a\nb\x00\n\x0a");
        let (f2, n2) = framed(b"\n");
        let (s, n) = stream(&[
            b"[INIT] boot\n",
            &f1[..n1],
            b"M!{\"x\":1}\n",
            &f2[..n2],
            b"tail",
        ]);
        for step in 1..=n {
            let seen = scan_in_steps(&s[..n], step);
            assert_eq!(seen.frame_count, 2, "step {step}");
            assert_eq!(seen.frame(0), b"a\nb\x00\n\x0a");
            assert_eq!(seen.frame(1), b"\n");
            assert_eq!(
                &seen.text[..seen.text_len],
                b"[INIT] boot\nM!{\"x\":1}\ntail"
            );
            assert_eq!(seen.dropped, 0);
        }
    }

    #[test]
    fn a_torn_frame_then_a_good_one() {
        // A frame start, a partial COBS body (its code promises 9 bytes), the
        // board's boot text, then a complete frame.
        let (good, ng) = framed(b"payload");
        let (s, n) = stream(&[
            b"\x00P\x09abc",
            b"ESP-ROM:esp32c6\n",
            &good[..ng],
            b"after\n",
        ]);
        for step in 1..=n {
            let seen = scan_in_steps(&s[..n], step);
            assert_eq!(seen.dropped, 1, "step {step}");
            assert_eq!(seen.frame_count, 1, "step {step}");
            assert_eq!(seen.frame(0), b"payload");
            assert_eq!(&seen.text[..seen.text_len], b"after\n");
        }
    }

    #[test]
    fn a_frame_too_long_is_skipped() {
        let big = [7u8; 100];
        let mut out = [0u8; 128];
        let nb = frame(FRAME_KIND_PACK, &big, &mut out).unwrap();
        let (good, ng) = framed(b"ok");
        let (s, n) = stream(&[&out[..nb], b"x", &good[..ng]]);
        let seen = scan_in_steps(&s[..n], 5);
        assert_eq!(seen.dropped, 1);
        assert_eq!(seen.frame_count, 1);
        assert_eq!(seen.frame(0), b"ok");
        assert_eq!(&seen.text[..seen.text_len], b"x");
    }

    #[test]
    fn doubled_delimiter_starts_the_frame_again() {
        let (good, ng) = framed(b"z");
        let (s, n) = stream(&[b"\x00", &good[..ng]]);
        let seen = scan_in_steps(&s[..n], 1);
        assert_eq!(seen.frame_count, 1);
        assert_eq!(seen.frame(0), b"z");
    }
}
