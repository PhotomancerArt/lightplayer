//! Follow-up F1: LPBJ straight from the serializer's structural tokens.
//!
//! [`TokenEncoder`] is a `SerWrite` sink that *takes* every token the (forked)
//! `ser-write-json` serializer offers through `SerWrite::token`, so the JSON text
//! is never produced and never re-lexed. Same serializer instantiation, same
//! LPBJ bytes out as [`crate::Encoder`]: integers arrive as integers, and the
//! few floats are printed with the serializer's own `ryu-js` and sent as the
//! same text-exact decimals.
//!
//! Text the serializer writes directly (a `RawValue`) is lexed into the same
//! frame by the text encoder; anything that fails there falls back to the
//! JSON line.

use crate::format_tags as tag;
use crate::{EncodeError, MAX_BACKREFS, base64_blob, varint, wire_dictionary};
use ser_write::{SerWrite, Token};

/// Token-fed LPBJ encoder over `out`.
pub struct TokenEncoder<'a> {
    out: &'a mut [u8],
    len: usize,
    blob_next: bool,
    backrefs: [(u32, u16); MAX_BACKREFS],
    backref_count: usize,
}

impl<'a> TokenEncoder<'a> {
    pub fn new(out: &'a mut [u8]) -> Self {
        Self { out, len: 0, blob_next: false, backrefs: [(0, 0); MAX_BACKREFS], backref_count: 0 }
    }

    /// Encoded length so far (the whole frame once serialization returned).
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn key(&mut self, k: &str) -> Result<(), EncodeError> {
        let k = k.as_bytes();
        self.blob_next = wire_dictionary::is_blob_key(k);
        if let Some(i) = wire_dictionary::find_key(k) {
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
        if let Some(n) = self.find_backref(k) {
            self.put(tag::KEY_BACKREF)?;
            return self.put_varint(n as u64);
        }
        self.put(tag::KEY_INLINE)?;
        self.put_varint(k.len() as u64)?;
        self.put_text(k)
    }

    fn string(&mut self, v: &str) -> Result<(), EncodeError> {
        let v = v.as_bytes();
        if core::mem::take(&mut self.blob_next) && base64_blob::is_canonical(v) {
            let pad = v.iter().rev().take_while(|&&c| c == b'=').count();
            let n = v.len() / 4 * 3 - pad;
            self.put(tag::BLOB)?;
            self.put_varint(n as u64)?;
            let at = self.len;
            self.put_all(v)?; // decode in place over its own copy
            let got = base64_blob::decode_in_place(&mut self.out[at..at + v.len()]);
            debug_assert_eq!(got, n);
            self.len = at + got;
            return Ok(());
        }
        if let Some(i) = wire_dictionary::find_value(v) {
            if i < tag::VDICT_INLINE_COUNT {
                return self.put(tag::VDICT_INLINE_BASE + i as u8);
            }
            self.put(tag::VDICT)?;
            return self.put_varint((i - tag::VDICT_INLINE_COUNT) as u64);
        }
        if let Some(n) = self.find_backref(v) {
            self.put(tag::BACKREF)?;
            return self.put_varint(n as u64);
        }
        if v.len() <= tag::STR_INLINE_MAX_LEN {
            self.put(tag::STR_INLINE_BASE + v.len() as u8)?;
        } else {
            self.put(tag::STRING)?;
            self.put_varint(v.len() as u64)?;
        }
        self.put_text(v)
    }

    fn float(&mut self, text: &str) -> Result<(), EncodeError> {
        match crate::decimal_text::classify(text.as_bytes()) {
            crate::decimal_text::NumberToken::Int { neg: false, mag } => self.uint(mag),
            crate::decimal_text::NumberToken::Int { neg: true, mag } => {
                self.put(tag::NEG_INT)?;
                self.put_varint(mag)
            }
            crate::decimal_text::NumberToken::Decimal { neg, coeff, exp } => {
                self.put(if neg { tag::DECIMAL_NEG } else { tag::DECIMAL_POS })?;
                self.put_varint(varint::zigzag(i64::from(exp)))?;
                self.put_varint(coeff)
            }
            crate::decimal_text::NumberToken::Text => {
                self.put(tag::NUMBER_TEXT)?;
                self.put_varint(text.len() as u64)?;
                self.put_all(text.as_bytes())
            }
        }
    }

    fn uint(&mut self, v: u64) -> Result<(), EncodeError> {
        if v <= u64::from(tag::UINT_INLINE_MAX) {
            return self.put(v as u8);
        }
        self.put(tag::UINT)?;
        self.put_varint(v)
    }

    /// Inline text: written, and numbered for back-references.
    fn put_text(&mut self, t: &[u8]) -> Result<(), EncodeError> {
        let at = self.len;
        self.put_all(t)?;
        if t.len() >= 2 && self.backref_count < MAX_BACKREFS {
            self.backrefs[self.backref_count] = (at as u32, t.len() as u16);
            self.backref_count += 1;
        }
        Ok(())
    }

    fn find_backref(&self, t: &[u8]) -> Option<usize> {
        if t.len() < 2 {
            return None;
        }
        self.backrefs[..self.backref_count]
            .iter()
            .position(|&(a, l)| l as usize == t.len() && &self.out[a as usize..a as usize + t.len()] == t)
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

impl SerWrite for TokenEncoder<'_> {
    type Error = EncodeError;
    const TAKES_TOKENS: bool = true;

    /// Text the serializer wrote itself. The one case on the wire is a
    /// `RawValue` (pre-serialized slot JSON, written whole in one call): it is
    /// lexed by the text encoder into the same frame, sharing the frame's
    /// back-reference table. Anything else that is not a complete JSON value
    /// fails, and the caller falls back to the JSON line.
    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.blob_next = false;
        let mut lex = crate::Encoder::continuing(&mut *self.out, self.len, self.backrefs, self.backref_count);
        lex.push(buf)?;
        let (len, backrefs, count) = lex.finish_parts()?;
        self.len = len;
        self.backrefs = backrefs;
        self.backref_count = count;
        Ok(())
    }

    fn token(&mut self, token: Token<'_>) -> Result<bool, Self::Error> {
        if !matches!(token, Token::Key(_) | Token::Separator | Token::Str(_)) {
            self.blob_next = false;
        }
        match token {
            Token::MapBegin => self.put(tag::OBJECT)?,
            Token::MapEnd => self.put(tag::OBJECT_END)?,
            Token::SeqBegin => self.put(tag::ARRAY)?,
            Token::SeqEnd => self.put(tag::ARRAY_END)?,
            Token::Key(k) => self.key(k)?,
            Token::Str(v) => self.string(v)?,
            Token::U64(v) => self.uint(v)?,
            Token::I64(v) if v >= 0 => self.uint(v as u64)?,
            Token::I64(v) => {
                self.put(tag::NEG_INT)?;
                self.put_varint(v.unsigned_abs())?;
            }
            // Floats are printed here with the serializer's own `ryu-js` (already
            // in every device image) and sent as the same text-exact decimals as
            // the lexing path, so decoders need no float printer (Studio does not
            // link `ryu-js`; it would cost the wasm ~15.6 KB). A lens reply
            // carries ~3 floats, so the formatting is noise.
            Token::F32(v) => self.float(ryu_js::Buffer::new().format_finite(v))?,
            Token::F64(v) => self.float(ryu_js::Buffer::new().format_finite(v))?,
            Token::Bool(v) => self.put(if v { tag::TRUE } else { tag::FALSE })?,
            Token::Null => self.put(tag::NULL)?,
            Token::Separator => {}
            _ => return Ok(false),
        }
        Ok(true)
    }
}
