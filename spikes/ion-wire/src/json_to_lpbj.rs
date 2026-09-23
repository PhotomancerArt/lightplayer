//! The streaming encoder: JSON bytes in (as the serializer writes them), LPBJ out.
//!
//! It is a byte-at-a-time lexer that writes straight into a caller-owned output
//! buffer (on the device, the existing static frame buffer). No heap, no second
//! buffer: a string's text is written *tentatively* into the output, and when
//! its closing quote arrives the encoder decides in place what it becomes — a
//! dictionary code, a back-reference, an inline string, or (under a blob key) the
//! raw bytes of its base64. Rewriting only ever moves the current string's own
//! bytes a few positions, so there is no length back-patching of containers:
//! objects and arrays are end-delimited (Ion 1.1's delimited containers, not Ion
//! 1.0's length prefixes).
//!
//! Input is trusted to be well-formed JSON as `ser_write_json` produces it;
//! malformed input yields [`EncodeError::Malformed`] rather than garbage, and the
//! caller falls back to sending the JSON line.

use crate::format_tags as tag;
use crate::{MAX_BACKREFS, base64_blob, decimal_text, varint, wire_dictionary};
use decimal_text::NumberToken;

/// Why a frame could not be encoded. Either way the caller sends JSON instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// The output buffer is full.
    Full,
    /// The input was not JSON this encoder understands.
    Malformed,
}

/// Header room reserved before a tentative string: one tag byte plus a LEB128
/// length of up to 3 bytes (2 MiB), far above the 16 KiB frame budget.
const RESERVE: usize = 4;
/// Longest number token (ryu-js prints at most ~25 bytes).
const NUM_MAX: usize = 40;
/// Deepest nesting (the wire's is under 16).
const MAX_DEPTH: u32 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lex {
    Between,
    Str,
    StrEscape,
    StrUnicode,
    Number,
    Literal,
}

/// Streaming JSON → LPBJ encoder over `out`. Feed with [`Encoder::push`],
/// complete with [`Encoder::finish`].
pub struct Encoder<'a> {
    out: &'a mut [u8],
    len: usize,
    /// One bit per open container, 1 = object.
    stack: u64,
    depth: u32,
    expect_key: bool,
    blob_next: bool,
    lex: Lex,
    str_at: usize,
    str_is_key: bool,
    unicode: u32,
    unicode_digits: u8,
    high_surrogate: u32,
    num: [u8; NUM_MAX],
    num_len: usize,
    literal_left: u8,
    backrefs: [(u32, u16); MAX_BACKREFS],
    backref_count: usize,
    failed: Option<EncodeError>,
}

impl<'a> Encoder<'a> {
    pub fn new(out: &'a mut [u8]) -> Self {
        Self {
            out,
            len: 0,
            stack: 0,
            depth: 0,
            expect_key: false,
            blob_next: false,
            lex: Lex::Between,
            str_at: 0,
            str_is_key: false,
            unicode: 0,
            unicode_digits: 0,
            high_surrogate: 0,
            num: [0; NUM_MAX],
            num_len: 0,
            literal_left: 0,
            backrefs: [(0, 0); MAX_BACKREFS],
            backref_count: 0,
            failed: None,
        }
    }

    /// Feed the next slice of JSON text.
    pub fn push(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        if let Some(e) = self.failed {
            return Err(e);
        }
        for &b in bytes {
            if let Err(e) = self.step(b) {
                self.failed = Some(e);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Complete the frame, returning the encoded length.
    pub fn finish(mut self) -> Result<usize, EncodeError> {
        if let Some(e) = self.failed {
            return Err(e);
        }
        if self.lex == Lex::Number {
            self.flush_number()?;
        }
        if self.lex != Lex::Between || self.depth != 0 || self.len == 0 {
            return Err(EncodeError::Malformed);
        }
        Ok(self.len)
    }

    fn step(&mut self, b: u8) -> Result<(), EncodeError> {
        match self.lex {
            Lex::Str => self.step_string(b),
            Lex::StrEscape => self.step_escape(b),
            Lex::StrUnicode => self.step_unicode(b),
            Lex::Literal => {
                self.literal_left -= 1;
                if self.literal_left == 0 {
                    self.lex = Lex::Between;
                }
                Ok(())
            }
            Lex::Number => {
                if matches!(b, b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-') {
                    if self.num_len == NUM_MAX {
                        return Err(EncodeError::Malformed);
                    }
                    self.num[self.num_len] = b;
                    self.num_len += 1;
                    Ok(())
                } else {
                    self.flush_number()?;
                    self.step_between(b)
                }
            }
            Lex::Between => self.step_between(b),
        }
    }

    fn step_between(&mut self, b: u8) -> Result<(), EncodeError> {
        match b {
            b'"' => {
                self.str_is_key = self.expect_key;
                if !self.str_is_key {
                    self.check_value_position()?;
                }
                self.str_at = self.len;
                self.reserve(RESERVE)?;
                self.lex = Lex::Str;
            }
            b'{' => self.open(true)?,
            b'[' => self.open(false)?,
            b'}' => self.close(true)?,
            b']' => self.close(false)?,
            b',' => self.expect_key = self.top_is_object(),
            b':' => {}
            b' ' | b'\n' | b'\r' | b'\t' => {}
            b'-' | b'0'..=b'9' => {
                self.check_value_position()?;
                self.num[0] = b;
                self.num_len = 1;
                self.lex = Lex::Number;
            }
            b't' | b'f' | b'n' => {
                self.check_value_position()?;
                let (t, rest) = match b {
                    b't' => (tag::TRUE, 3),
                    b'f' => (tag::FALSE, 4),
                    _ => (tag::NULL, 3),
                };
                self.blob_next = false;
                self.put(t)?;
                self.literal_left = rest;
                self.lex = Lex::Literal;
            }
            _ => return Err(EncodeError::Malformed),
        }
        Ok(())
    }

    fn step_string(&mut self, b: u8) -> Result<(), EncodeError> {
        match b {
            b'"' => {
                self.lex = Lex::Between;
                if self.str_is_key {
                    self.finish_key()
                } else {
                    self.finish_string_value()
                }
            }
            b'\\' => {
                self.lex = Lex::StrEscape;
                Ok(())
            }
            _ => self.put(b),
        }
    }

    fn step_escape(&mut self, b: u8) -> Result<(), EncodeError> {
        self.lex = Lex::Str;
        let c = match b {
            b'"' => b'"',
            b'\\' => b'\\',
            b'/' => b'/',
            b'b' => 0x08,
            b'f' => 0x0C,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'u' => {
                self.lex = Lex::StrUnicode;
                self.unicode = 0;
                self.unicode_digits = 0;
                return Ok(());
            }
            _ => return Err(EncodeError::Malformed),
        };
        self.put(c)
    }

    fn step_unicode(&mut self, b: u8) -> Result<(), EncodeError> {
        let d = (b as char).to_digit(16).ok_or(EncodeError::Malformed)?;
        self.unicode = (self.unicode << 4) | d;
        self.unicode_digits += 1;
        if self.unicode_digits < 4 {
            return Ok(());
        }
        self.lex = Lex::Str;
        let u = self.unicode;
        if (0xD800..0xDC00).contains(&u) {
            self.high_surrogate = u;
            return Ok(());
        }
        let cp = if (0xDC00..0xE000).contains(&u) {
            if self.high_surrogate == 0 {
                return Err(EncodeError::Malformed);
            }
            let hi = core::mem::take(&mut self.high_surrogate);
            0x10000 + ((hi - 0xD800) << 10) + (u - 0xDC00)
        } else {
            u
        };
        let ch = char::from_u32(cp).ok_or(EncodeError::Malformed)?;
        let mut buf = [0u8; 4];
        self.put_all(ch.encode_utf8(&mut buf).as_bytes())
    }

    fn open(&mut self, object: bool) -> Result<(), EncodeError> {
        self.check_value_position()?;
        if self.depth == MAX_DEPTH {
            return Err(EncodeError::Malformed);
        }
        self.blob_next = false;
        self.put(if object { tag::OBJECT } else { tag::ARRAY })?;
        self.stack = (self.stack << 1) | u64::from(object);
        self.depth += 1;
        self.expect_key = object;
        Ok(())
    }

    fn close(&mut self, object: bool) -> Result<(), EncodeError> {
        if self.depth == 0 || self.top_is_object() != object {
            return Err(EncodeError::Malformed);
        }
        self.put(if object { tag::OBJECT_END } else { tag::ARRAY_END })?;
        self.stack >>= 1;
        self.depth -= 1;
        self.expect_key = false;
        Ok(())
    }

    fn top_is_object(&self) -> bool {
        self.depth > 0 && self.stack & 1 == 1
    }

    fn check_value_position(&mut self) -> Result<(), EncodeError> {
        if self.expect_key {
            return Err(EncodeError::Malformed);
        }
        Ok(())
    }

    fn flush_number(&mut self) -> Result<(), EncodeError> {
        self.lex = Lex::Between;
        self.blob_next = false;
        let n = self.num_len;
        match decimal_text::classify(&self.num[..n]) {
            NumberToken::Int { neg: false, mag } if mag <= u64::from(tag::UINT_INLINE_MAX) => {
                self.put(mag as u8)
            }
            NumberToken::Int { neg, mag } => {
                self.put(if neg { tag::NEG_INT } else { tag::UINT })?;
                self.put_varint(mag)
            }
            NumberToken::Decimal { neg, coeff, exp } => {
                self.put(if neg { tag::DECIMAL_NEG } else { tag::DECIMAL_POS })?;
                self.put_varint(varint::zigzag(i64::from(exp)))?;
                self.put_varint(coeff)
            }
            NumberToken::Text => {
                self.put(tag::NUMBER_TEXT)?;
                self.put_varint(n as u64)?;
                let num = self.num;
                self.put_all(&num[..n])
            }
        }
    }

    fn finish_key(&mut self) -> Result<(), EncodeError> {
        let text_at = self.str_at + RESERVE;
        let text_len = self.len - text_at;
        let (is_blob, found) = {
            let text = &self.out[text_at..self.len];
            (wire_dictionary::is_blob_key(text), wire_dictionary::find_key(text))
        };
        self.blob_next = is_blob;
        self.expect_key = false;
        if let Some(i) = found {
            self.len = self.str_at;
            if i < tag::KDICT_INLINE_COUNT {
                return self.put(i as u8);
            }
            let wide = i - tag::KDICT_INLINE_COUNT;
            let hi = tag::KDICT_WIDE_BASE as usize + (wide >> 8);
            if hi > tag::KDICT_WIDE_MAX as usize {
                return Err(EncodeError::Malformed);
            }
            self.put(hi as u8)?;
            return self.put(wide as u8);
        }
        if let Some(n) = self.find_backref(text_at, text_len) {
            self.len = self.str_at;
            self.put(tag::KEY_BACKREF)?;
            return self.put_varint(n as u64);
        }
        self.write_inline_text(tag::KEY_INLINE, text_at, text_len, false)
    }

    fn finish_string_value(&mut self) -> Result<(), EncodeError> {
        let text_at = self.str_at + RESERVE;
        let text_len = self.len - text_at;
        let blob = core::mem::take(&mut self.blob_next);
        if blob && base64_blob::is_canonical(&self.out[text_at..self.len]) {
            let n = base64_blob::decode_in_place(&mut self.out[text_at..self.len]);
            self.len = text_at + n;
            return self.write_inline_text(tag::BLOB, text_at, n, true);
        }
        if let Some(i) = wire_dictionary::find_value(&self.out[text_at..self.len]) {
            self.len = self.str_at;
            if i < tag::VDICT_INLINE_COUNT {
                return self.put(tag::VDICT_INLINE_BASE + i as u8);
            }
            self.put(tag::VDICT)?;
            return self.put_varint((i - tag::VDICT_INLINE_COUNT) as u64);
        }
        if let Some(n) = self.find_backref(text_at, text_len) {
            self.len = self.str_at;
            self.put(tag::BACKREF)?;
            return self.put_varint(n as u64);
        }
        if text_len <= tag::STR_INLINE_MAX_LEN {
            self.out[self.str_at] = tag::STR_INLINE_BASE + text_len as u8;
            self.out.copy_within(text_at..text_at + text_len, self.str_at + 1);
            self.len = self.str_at + 1 + text_len;
            self.remember(self.str_at + 1, text_len);
            return Ok(());
        }
        self.write_inline_text(tag::STRING, text_at, text_len, false)
    }

    /// Replace the tentative header with `tag` + LEB128 length and slide the
    /// text down to meet it.
    fn write_inline_text(
        &mut self,
        t: u8,
        text_at: usize,
        text_len: usize,
        is_blob: bool,
    ) -> Result<(), EncodeError> {
        let mut hdr = [0u8; 11];
        hdr[0] = t;
        let h = 1 + varint::write(&mut hdr[1..], text_len as u64);
        if h > RESERVE {
            return Err(EncodeError::Full);
        }
        self.out[self.str_at..self.str_at + h].copy_from_slice(&hdr[..h]);
        let dst = self.str_at + h;
        self.out.copy_within(text_at..text_at + text_len, dst);
        self.len = dst + text_len;
        if !is_blob {
            self.remember(dst, text_len);
        }
        Ok(())
    }

    fn remember(&mut self, at: usize, len: usize) {
        if len >= 2 && self.backref_count < MAX_BACKREFS {
            self.backrefs[self.backref_count] = (at as u32, len as u16);
            self.backref_count += 1;
        }
    }

    fn find_backref(&self, text_at: usize, text_len: usize) -> Option<usize> {
        if text_len < 2 {
            return None;
        }
        let text = &self.out[text_at..text_at + text_len];
        self.backrefs[..self.backref_count]
            .iter()
            .position(|&(a, l)| l as usize == text_len && &self.out[a as usize..a as usize + text_len] == text)
    }

    fn reserve(&mut self, n: usize) -> Result<(), EncodeError> {
        if self.len + n > self.out.len() {
            return Err(EncodeError::Full);
        }
        self.len += n;
        Ok(())
    }

    fn put(&mut self, b: u8) -> Result<(), EncodeError> {
        *self.out.get_mut(self.len).ok_or(EncodeError::Full)? = b;
        self.len += 1;
        Ok(())
    }

    fn put_all(&mut self, s: &[u8]) -> Result<(), EncodeError> {
        let end = self.len + s.len();
        self.out.get_mut(self.len..end).ok_or(EncodeError::Full)?.copy_from_slice(s);
        self.len = end;
        Ok(())
    }

    fn put_varint(&mut self, v: u64) -> Result<(), EncodeError> {
        let mut buf = [0u8; 10];
        let n = varint::write(&mut buf, v);
        self.put_all(&buf[..n])
    }
}
