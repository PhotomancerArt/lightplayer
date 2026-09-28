//! Splits the incoming byte stream into frames and console text.
//!
//! Frames are `0x00 …COBS… 0x00`; anything outside a frame is text (the boot
//! ROM's banner, a panic message, a raw `println!`), which the link hands up
//! as-is so a plain serial monitor and Studio's console both still read it.
//! Text never holds `0x00`, so the first `0x00` opens a frame and the next one
//! closes it.
//!
//! Resync rules, for a stream that loses bytes:
//! - A frame that decodes and verifies: the next byte is text or a new `0x00`.
//! - A frame that fails: its closing `0x00` may really have been the *opening*
//!   of the next frame (the tail of this one was lost), so stay inside a frame.
//! - A frame longer than the maximum is discarded up to the next `0x00`.
//! - A partial frame that goes quiet for the frame-abandon time (seconds, not
//!   the text idle time: a busy peer writes one frame in pieces) is flushed:
//!   as text if it is all printable (it was text after all), else dropped and
//!   counted.
//!
//! **The text mark** (COBS-FF framing only): `0xFF` never occurs inside a
//! COBS-FF frame, so a raw `0xFF` means "text follows": whatever frame was in
//! progress is abandoned and the deframer is outside a frame again. A panic
//! handler writes `0xFF` before its message, so the message arrives as text
//! however fast the rebooted board's first frame follows it (silicon, M3: the
//! ROM banner and the app's first frame come within milliseconds, too soon
//! for the idle flush, and a text run that long overflows the frame buffer).

use alloc::vec::Vec;
use core::mem;

use crate::Micros;

/// Text is handed up at a newline, at a frame start, or at this many bytes.
const TEXT_CHUNK: usize = 256;

/// What one pushed byte completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deframed {
    Nothing,
    /// A complete frame body is in [`Deframer::frame`]; call
    /// [`Deframer::frame_done`] after handling it.
    Frame,
    /// Text is ready in [`Deframer::take_text`].
    Text,
    /// An over-long frame was discarded.
    Overflow,
    /// A text mark cut short the frame in progress.
    Abandoned,
}

/// The raw byte that starts text (COBS-FF framing; module docs).
pub const TEXT_MARK: u8 = 0xFF;

/// What an idle flush did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleFlush {
    Nothing,
    Text,
    /// A partial binary frame was dropped.
    Garbage,
}

pub struct Deframer {
    buf: Vec<u8>,
    text: Vec<u8>,
    in_frame: bool,
    discarding: bool,
    max_frame: usize,
    last_byte_at: Micros,
    text_mark: bool,
}

impl Deframer {
    /// `max_frame`: the largest COBS body (delimiters excluded) to accept.
    /// `reserve`: allocate the buffers now, at their largest (a link on a
    /// stream does; one on datagrams never feeds its deframer).
    pub fn new(max_frame: usize, reserve: bool) -> Self {
        let (buf, text) = if reserve {
            (max_frame, max_frame.max(TEXT_CHUNK))
        } else {
            (0, 0)
        };
        Deframer {
            buf: Vec::with_capacity(buf),
            text: Vec::with_capacity(text),
            in_frame: false,
            discarding: false,
            max_frame,
            last_byte_at: 0,
            text_mark: false,
        }
    }

    /// Honour [`TEXT_MARK`] (only where frames can never hold `0xFF`).
    pub fn with_text_mark(mut self, on: bool) -> Self {
        self.text_mark = on;
        self
    }

    pub fn push(&mut self, now: Micros, b: u8) -> Deframed {
        self.last_byte_at = now;
        if self.text_mark && b == TEXT_MARK {
            let abandoned = self.in_frame && (!self.buf.is_empty() || self.discarding);
            self.in_frame = false;
            self.discarding = false;
            self.buf.clear();
            return if abandoned {
                Deframed::Abandoned
            } else {
                Deframed::Nothing
            };
        }
        if b == 0 {
            if !self.in_frame {
                self.in_frame = true;
                self.buf.clear();
                return if self.text.is_empty() {
                    Deframed::Nothing
                } else {
                    Deframed::Text
                };
            }
            if self.discarding {
                self.discarding = false;
                self.buf.clear();
                return Deframed::Overflow;
            }
            return if self.buf.is_empty() {
                Deframed::Nothing
            } else {
                Deframed::Frame
            };
        }
        if self.in_frame {
            if !self.discarding {
                if self.buf.len() == self.max_frame {
                    self.discarding = true;
                } else {
                    self.buf.push(b);
                }
            }
            return Deframed::Nothing;
        }
        self.text.push(b);
        if b == b'\n' || self.text.len() >= TEXT_CHUNK {
            Deframed::Text
        } else {
            Deframed::Nothing
        }
    }

    /// The COBS body of the frame [`push`](Self::push) just completed.
    pub fn frame(&self) -> &[u8] {
        &self.buf
    }

    /// `ok`: the frame verified. A failed frame's closing delimiter is taken as
    /// the opening of the next one.
    pub fn frame_done(&mut self, ok: bool) {
        self.buf.clear();
        self.in_frame = !ok;
    }

    pub fn take_text(&mut self) -> Vec<u8> {
        mem::take(&mut self.text)
    }

    /// The text [`push`](Self::push) or [`flush_idle`](Self::flush_idle)
    /// reported; [`clear_text`](Self::clear_text) once it is handled.
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    pub fn clear_text(&mut self) {
        self.text.clear();
    }

    /// When an idle flush is due, if anything is pending: pending text after
    /// `text_idle` of quiet, a partial frame after `frame_idle` (see
    /// [`LinkConfig::frame_abandon`](crate::LinkConfig::frame_abandon) for why
    /// the two differ).
    pub fn idle_deadline(&self, text_idle: Micros, frame_idle: Micros) -> Option<Micros> {
        if !self.text.is_empty() {
            return Some(self.last_byte_at + text_idle);
        }
        let partial = self.in_frame && (!self.buf.is_empty() || self.discarding);
        partial.then_some(self.last_byte_at + frame_idle)
    }

    pub fn flush_idle(&mut self) -> IdleFlush {
        if !self.text.is_empty() {
            return IdleFlush::Text;
        }
        if !self.in_frame || (self.buf.is_empty() && !self.discarding) {
            return IdleFlush::Nothing;
        }
        self.in_frame = false;
        self.discarding = false;
        if self
            .buf
            .iter()
            .all(|&b| matches!(b, 0x20..=0x7E | b'\t' | b'\r' | b'\n'))
        {
            self.text.extend_from_slice(&self.buf);
            self.buf.clear();
            IdleFlush::Text
        } else {
            self.buf.clear();
            IdleFlush::Garbage
        }
    }

    /// Bytes held (for RAM accounting).
    pub fn capacity(&self) -> usize {
        self.buf.capacity() + self.text.capacity()
    }

    /// [`capacity`](Self::capacity) at its largest: a frame body in `buf`, and
    /// in `text` a text chunk or a partial frame flushed as text.
    pub const fn ram_bound(max_frame: usize) -> usize {
        let text = if max_frame > TEXT_CHUNK {
            max_frame
        } else {
            TEXT_CHUNK
        };
        max_frame + text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn feed(d: &mut Deframer, bytes: &[u8]) -> Vec<Deframed> {
        let mut out = vec![];
        for &b in bytes {
            let r = d.push(0, b);
            match r {
                Deframed::Frame => d.frame_done(true),
                Deframed::Text => drop(d.take_text()),
                _ => {}
            }
            if r != Deframed::Nothing {
                out.push(r);
            }
        }
        out
    }

    #[test]
    fn text_frames_and_text() {
        let mut d = Deframer::new(64, false);
        let r = feed(&mut d, b"boot\n\x00\x02\x01\x00hi\n");
        assert_eq!(r, [Deframed::Text, Deframed::Frame, Deframed::Text]);
    }

    #[test]
    fn failed_frame_keeps_frame_mode() {
        let mut d = Deframer::new(64, false);
        // First "frame" lost its closing delimiter; its next 0x00 is really the
        // opening of the next frame.
        assert_eq!(d.push(0, 0), Deframed::Nothing);
        for &b in b"\x05abc" {
            d.push(0, b);
        }
        assert_eq!(d.push(0, 0), Deframed::Frame);
        d.frame_done(false);
        for &b in b"\x03xy" {
            d.push(0, b);
        }
        assert_eq!(d.push(0, 0), Deframed::Frame);
        assert_eq!(d.frame(), b"\x03xy");
    }

    #[test]
    fn overflow_is_discarded() {
        let mut d = Deframer::new(4, false);
        d.push(0, 0);
        for _ in 0..10 {
            d.push(0, 7);
        }
        assert_eq!(d.push(0, 0), Deframed::Overflow);
    }

    #[test]
    fn a_text_mark_abandons_the_frame_and_the_text_arrives() {
        let mut d = Deframer::new(64, false).with_text_mark(true);
        // A frame cut off mid-way by a panic: 0xFF, the message, then the
        // rebooted board's first frame straight after.
        let r = feed(&mut d, b"\x00\x05ab\xFFpanicked\nROM\n\x00\x02\x01\x00");
        assert_eq!(
            r,
            [
                Deframed::Abandoned,
                Deframed::Text,
                Deframed::Text,
                Deframed::Frame
            ]
        );
        // Without the mark honoured, the same bytes are one bad frame.
        let mut plain = Deframer::new(64, false);
        let r = feed(&mut plain, b"\x00\x05ab\xFFpanicked\n\x00");
        assert_eq!(r, [Deframed::Frame]);
    }

    #[test]
    fn idle_partial_printable_becomes_text() {
        let mut d = Deframer::new(64, false);
        d.push(0, 0);
        for &b in b"panic!" {
            d.push(5, b);
        }
        // A partial frame waits for the frame-abandon time, not the text one.
        assert_eq!(d.idle_deadline(10, 1000), Some(1005));
        assert_eq!(d.flush_idle(), IdleFlush::Text);
        assert_eq!(d.take_text(), b"panic!");
    }

    #[test]
    fn pending_text_flushes_on_the_short_idle() {
        let mut d = Deframer::new(64, false);
        for &b in b"boot" {
            d.push(5, b);
        }
        assert_eq!(d.idle_deadline(10, 1000), Some(15));
        assert_eq!(d.flush_idle(), IdleFlush::Text);
    }
}
