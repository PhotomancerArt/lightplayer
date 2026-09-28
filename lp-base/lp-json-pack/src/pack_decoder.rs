//! The decoder: one JSON Pack frame → the byte-identical JSON text.
//!
//! String escaping mirrors `ser-write-json` exactly (`"`, `\`, and C0 controls;
//! `\b \t \n \f \r` short, the other controls as upper-case `\u00XX`), blobs
//! come back as canonical padded base64, and decimals as their JS layout. That
//! is what makes the round trip byte-exact.
//!
//! It never recurses (containers are tracked in a bit stack, 64 deep) and never
//! panics on hostile input: every malformed frame is a [`DecodeError`].

use crate::pack_base64::encode_base64;
use crate::pack_decimal::{DECIMAL_TEXT_MAX, layout_decimal};
use crate::pack_dictionary::Dictionary;
use crate::pack_learned::{HeaderMismatch, LEARN_KEY_MAX_LEN, LearnStore, read_header};
use crate::pack_tags as tag;
use crate::pack_varint::{read_varint, unzigzag};
use crate::{MAX_BACKREFS, is_backref_candidate};

/// Deepest nesting a frame may have.
pub const DECODE_MAX_DEPTH: u32 = 64;

/// A byte sink for decoded JSON text.
pub trait JsonOut {
    /// Append `bytes`, or report that the sink is full.
    fn write_json(&mut self, bytes: &[u8]) -> Result<(), JsonOutFull>;
}

/// The [`JsonOut`] ran out of room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonOutFull;

/// A [`JsonOut`] over a fixed buffer.
pub struct SliceJsonOut<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl<'a> SliceJsonOut<'a> {
    /// Write into `buf` from its start.
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    /// Bytes written so far.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The text written so far.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

impl JsonOut for SliceJsonOut<'_> {
    fn write_json(&mut self, bytes: &[u8]) -> Result<(), JsonOutFull> {
        let end = self.len + bytes.len();
        self.buf
            .get_mut(self.len..end)
            .ok_or(JsonOutFull)?
            .copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }
}

#[cfg(feature = "alloc")]
impl JsonOut for alloc::vec::Vec<u8> {
    fn write_json(&mut self, bytes: &[u8]) -> Result<(), JsonOutFull> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

/// Why a frame did not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The frame ended mid-value.
    Truncated,
    /// A tag byte that means nothing in its position.
    BadTag(u8),
    /// A dictionary code or back-reference with no entry.
    BadIndex,
    /// A decimal whose exponent no layout can print.
    BadNumber,
    /// Nested deeper than [`DECODE_MAX_DEPTH`].
    TooDeep,
    /// Bytes left after the frame's one value.
    Trailing,
    /// The [`JsonOut`] is full.
    OutputFull,
    /// A learned frame whose header does not match the reader's table.
    Learned(HeaderMismatch),
}

impl From<JsonOutFull> for DecodeError {
    fn from(_: JsonOutFull) -> Self {
        DecodeError::OutputFull
    }
}

/// Decode one whole frame (`packed` must hold exactly one value) into `out`.
pub fn decode(dict: &Dictionary, packed: &[u8], out: &mut impl JsonOut) -> Result<(), DecodeError> {
    let mut d = Decoder {
        dict,
        input: packed,
        pos: 0,
        backref_at: [0; MAX_BACKREFS],
        backref_len: [0; MAX_BACKREFS],
        backref_count: 0,
        out,
        learned: None,
    };
    d.run()?;
    if d.pos != packed.len() {
        return Err(DecodeError::Trailing);
    }
    Ok(())
}

/// Decode one learned frame (header, then one value) coded against
/// `dict ++ learned`, learning as the encoder did. On any error the table is
/// truncated back to where it stood, so a frame that did not decode defined
/// nothing. The output may hold a partial value on error.
pub fn decode_learned(
    dict: &Dictionary,
    learned: &mut dyn LearnStore,
    packed: &[u8],
    out: &mut impl JsonOut,
) -> Result<(), DecodeError> {
    let start = read_header(packed, learned).map_err(DecodeError::Learned)?;
    let mark = learned.mark();
    let mut d = Decoder {
        dict,
        input: packed,
        pos: start,
        backref_at: [0; MAX_BACKREFS],
        backref_len: [0; MAX_BACKREFS],
        backref_count: 0,
        out,
        learned: Some(learned),
    };
    let r = d.run().and_then(|()| {
        if d.pos != packed.len() {
            Err(DecodeError::Trailing)
        } else {
            Ok(())
        }
    });
    if r.is_err()
        && let Some(l) = d.learned
    {
        l.truncate(mark);
    }
    r
}

/// A string the decoder found: in the frame or dictionary, or copied out of
/// the learned table.
enum Text<'a> {
    Borrowed(&'a [u8]),
    Copied([u8; LEARN_KEY_MAX_LEN], usize),
}

struct Decoder<'a, 'o, O: JsonOut> {
    dict: &'a Dictionary,
    input: &'a [u8],
    pos: usize,
    backref_at: [u32; MAX_BACKREFS],
    backref_len: [u16; MAX_BACKREFS],
    backref_count: usize,
    out: &'o mut O,
    learned: Option<&'o mut dyn LearnStore>,
}

impl<'a, O: JsonOut> Decoder<'a, '_, O> {
    fn run(&mut self) -> Result<(), DecodeError> {
        // One bit per open container, 1 = object.
        let mut stack: u64 = 0;
        let mut depth: u32 = 0;
        // Whether the current container has had an element yet.
        let mut first = true;
        loop {
            if depth > 0 {
                if stack & 1 == 1 {
                    let k = self.byte()?;
                    if k == tag::OBJECT_END {
                        self.emit(b"}")?;
                        stack >>= 1;
                        depth -= 1;
                        first = false;
                        if depth == 0 {
                            return Ok(());
                        }
                        continue;
                    }
                    if !first {
                        self.emit(b",")?;
                    }
                    let key = self.key(k)?;
                    self.text(key)?;
                    self.emit(b":")?;
                } else {
                    if self.peek()? == tag::ARRAY_END {
                        self.pos += 1;
                        self.emit(b"]")?;
                        stack >>= 1;
                        depth -= 1;
                        first = false;
                        if depth == 0 {
                            return Ok(());
                        }
                        continue;
                    }
                    if !first {
                        self.emit(b",")?;
                    }
                }
            }
            let t = self.byte()?;
            if t == tag::OBJECT || t == tag::ARRAY {
                if depth == DECODE_MAX_DEPTH {
                    return Err(DecodeError::TooDeep);
                }
                let object = t == tag::OBJECT;
                self.emit(if object { b"{" } else { b"[" })?;
                stack = (stack << 1) | u64::from(object);
                depth += 1;
                first = true;
                continue;
            }
            self.scalar(t)?;
            first = false;
            if depth == 0 {
                return Ok(());
            }
        }
    }

    fn key(&mut self, k: u8) -> Result<Text<'a>, DecodeError> {
        match k {
            0..=0xEF => self.dict_text(usize::from(k), true),
            tag::KEY_DICT_WIDE_BASE..=tag::KEY_DICT_WIDE_MAX => {
                let lo = self.byte()?;
                let i = tag::KEY_DICT_INLINE_COUNT
                    + ((usize::from(k - tag::KEY_DICT_WIDE_BASE) << 8) | usize::from(lo));
                self.dict_text(i, true)
            }
            tag::KEY_INLINE => {
                let n = self.length()?;
                let s = self.inline(n)?;
                if let Some(l) = self.learned.as_deref_mut() {
                    l.learn_key(s);
                }
                Ok(Text::Borrowed(s))
            }
            tag::KEY_BACKREF => self.backref().map(Text::Borrowed),
            _ => Err(DecodeError::BadTag(k)),
        }
    }

    /// Code `i` of the key or value table: the dictionary first, then the
    /// learned table after it.
    fn dict_text(&self, i: usize, is_key: bool) -> Result<Text<'a>, DecodeError> {
        let (n, hit) = if is_key {
            (self.dict.keys.len(), self.dict.key(i))
        } else {
            (self.dict.values.len(), self.dict.value(i))
        };
        if let Some(s) = hit {
            return Ok(Text::Borrowed(s));
        }
        let l = self.learned.as_deref().ok_or(DecodeError::BadIndex)?;
        let s = if is_key { l.key(i - n) } else { l.value(i - n) }.ok_or(DecodeError::BadIndex)?;
        let mut buf = [0u8; LEARN_KEY_MAX_LEN];
        buf.get_mut(..s.len())
            .ok_or(DecodeError::BadIndex)?
            .copy_from_slice(s);
        Ok(Text::Copied(buf, s.len()))
    }

    fn text(&mut self, t: Text<'_>) -> Result<(), DecodeError> {
        match t {
            Text::Borrowed(s) => self.string(s),
            Text::Copied(buf, n) => self.string(&buf[..n]),
        }
    }

    fn learn_value(&mut self, s: &[u8]) {
        if let Some(l) = self.learned.as_deref_mut() {
            l.learn_value(s);
        }
    }

    fn scalar(&mut self, t: u8) -> Result<(), DecodeError> {
        match t {
            0..=tag::UINT_INLINE_MAX => self.int(false, u64::from(t)),
            0x40..=0x7F => {
                let i = usize::from(t - tag::VALUE_DICT_INLINE_BASE);
                let s = self.dict_text(i, false)?;
                self.text(s)
            }
            tag::VALUE_DICT_HIGH_BASE..=0xFF => {
                let i = tag::VALUE_DICT_INLINE_COUNT + usize::from(t - tag::VALUE_DICT_HIGH_BASE);
                let s = self.dict_text(i, false)?;
                self.text(s)
            }
            0x80..=0x9F => {
                let s = self.inline(usize::from(t - tag::STR_INLINE_BASE))?;
                self.learn_value(s);
                self.string(s)
            }
            tag::NULL => self.emit(b"null"),
            tag::FALSE => self.emit(b"false"),
            tag::TRUE => self.emit(b"true"),
            tag::UINT | tag::NEG_INT => {
                let m = self.varint()?;
                self.int(t == tag::NEG_INT, m)
            }
            tag::DECIMAL_POS | tag::DECIMAL_NEG => {
                let exp =
                    i32::try_from(unzigzag(self.varint()?)).map_err(|_| DecodeError::BadNumber)?;
                let coeff = self.varint()?;
                let mut buf = [0u8; DECIMAL_TEXT_MAX];
                let n = layout_decimal(t == tag::DECIMAL_NEG, coeff, exp, &mut buf)
                    .ok_or(DecodeError::BadNumber)?;
                self.emit(&buf[..n])
            }
            tag::STRING => {
                let n = self.length()?;
                let s = self.inline(n)?;
                self.learn_value(s);
                self.string(s)
            }
            tag::BLOB => {
                let n = self.length()?;
                let raw = self.inline(n)?;
                self.blob(raw)
            }
            tag::BLOB_BACKREF => {
                let raw = self.backref()?;
                self.blob(raw)
            }
            tag::VALUE_DICT => {
                let i = usize::try_from(self.varint()?)
                    .ok()
                    .and_then(|i| i.checked_add(tag::VALUE_CODE_SHORT_COUNT))
                    .ok_or(DecodeError::BadIndex)?;
                let s = self.dict_text(i, false)?;
                self.text(s)
            }
            tag::BACKREF => {
                let s = self.backref()?;
                self.string(s)
            }
            tag::NUMBER_TEXT => {
                let n = self.length()?;
                let s = self.take(n)?;
                self.emit(s)
            }
            _ => Err(DecodeError::BadTag(t)),
        }
    }

    fn byte(&mut self) -> Result<u8, DecodeError> {
        let b = self.peek()?;
        self.pos += 1;
        Ok(b)
    }

    fn peek(&self) -> Result<u8, DecodeError> {
        self.input
            .get(self.pos)
            .copied()
            .ok_or(DecodeError::Truncated)
    }

    fn varint(&mut self) -> Result<u64, DecodeError> {
        read_varint(self.input, &mut self.pos).ok_or(DecodeError::Truncated)
    }

    fn length(&mut self) -> Result<usize, DecodeError> {
        usize::try_from(self.varint()?).map_err(|_| DecodeError::Truncated)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated)?;
        let s = self
            .input
            .get(self.pos..end)
            .ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(s)
    }

    /// Inline text or blob bytes: taken, and numbered for back-references
    /// exactly as the encoder numbered them.
    fn inline(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let at = self.pos;
        let s = self.take(n)?;
        if is_backref_candidate(n) && self.backref_count < MAX_BACKREFS {
            self.backref_at[self.backref_count] = at as u32;
            self.backref_len[self.backref_count] = n as u16;
            self.backref_count += 1;
        }
        Ok(s)
    }

    fn backref(&mut self) -> Result<&'a [u8], DecodeError> {
        let r = usize::try_from(self.varint()?).map_err(|_| DecodeError::BadIndex)?;
        if r >= self.backref_count {
            return Err(DecodeError::BadIndex);
        }
        let at = self.backref_at[r] as usize;
        Ok(&self.input[at..at + usize::from(self.backref_len[r])])
    }

    fn blob(&mut self, raw: &[u8]) -> Result<(), DecodeError> {
        self.emit(b"\"")?;
        encode_base64(raw, |chunk| self.out.write_json(chunk))?;
        self.emit(b"\"")
    }

    fn emit(&mut self, bytes: &[u8]) -> Result<(), DecodeError> {
        Ok(self.out.write_json(bytes)?)
    }

    fn int(&mut self, neg: bool, mut v: u64) -> Result<(), DecodeError> {
        let mut buf = [0u8; 21];
        let mut i = buf.len();
        loop {
            i -= 1;
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        if neg {
            i -= 1;
            buf[i] = b'-';
        }
        self.emit(&buf[i..])
    }

    fn string(&mut self, s: &[u8]) -> Result<(), DecodeError> {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        self.emit(b"\"")?;
        let mut start = 0;
        for (i, &b) in s.iter().enumerate() {
            let short = match b {
                0x08 => b'b',
                0x09 => b't',
                0x0A => b'n',
                0x0C => b'f',
                0x0D => b'r',
                b'"' => b'"',
                b'\\' => b'\\',
                0x00..=0x1F => b'u',
                _ => continue,
            };
            if start < i {
                self.emit(&s[start..i])?;
            }
            if short == b'u' {
                self.emit(&[
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    HEX[usize::from(b >> 4)],
                    HEX[usize::from(b & 15)],
                ])?;
            } else {
                self.emit(&[b'\\', short])?;
            }
            start = i + 1;
        }
        self.emit(&s[start..])?;
        self.emit(b"\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackEncoder;

    fn round_trip(
        f: impl FnOnce(&mut PackEncoder<'_>) -> Result<(), crate::PackError>,
    ) -> ([u8; 256], usize) {
        let mut packed = [0u8; 256];
        let mut e = PackEncoder::new(&mut packed, &Dictionary::EMPTY);
        f(&mut e).unwrap();
        let n = e.finish().unwrap();
        let mut json = [0u8; 256];
        let mut out = SliceJsonOut::new(&mut json);
        decode(&Dictionary::EMPTY, &packed[..n], &mut out).unwrap();
        let m = out.len();
        (json, m)
    }

    #[test]
    fn events_decode_to_compact_json() {
        let (j, n) = round_trip(|e| {
            e.begin_map()?;
            e.key("a")?;
            e.begin_seq()?;
            e.u64(1)?;
            e.i64(-70)?;
            e.decimal_text("2.5e-7")?;
            e.null()?;
            e.bool(false)?;
            e.begin_map()?;
            e.end_map()?;
            e.begin_seq()?;
            e.end_seq()?;
            e.end_seq()?;
            e.key("s")?;
            e.str("q\"\u{1}\n")?;
            e.key("b")?;
            e.blob(b"foob")?;
            e.end_map()
        });
        assert_eq!(
            core::str::from_utf8(&j[..n]).unwrap(),
            r#"{"a":[1,-70,2.5e-7,null,false,{},[]],"s":"q\"\u0001\n","b":"Zm9vYg=="}"#
        );
    }

    #[test]
    fn a_repeated_blob_is_a_back_reference() {
        let mut packed = [0u8; 64];
        let mut e = PackEncoder::new(&mut packed, &Dictionary::EMPTY);
        e.begin_seq().unwrap();
        e.blob(b"pixels").unwrap();
        e.str("pixels").unwrap();
        e.blob(b"pixels").unwrap();
        e.end_seq().unwrap();
        let n = e.finish().unwrap();
        assert_eq!(
            &packed[n - 5..n],
            &[tag::BACKREF, 0, tag::BLOB_BACKREF, 0, tag::ARRAY_END]
        );
        let mut json = [0u8; 64];
        let mut out = SliceJsonOut::new(&mut json);
        decode(&Dictionary::EMPTY, &packed[..n], &mut out).unwrap();
        assert_eq!(out.as_bytes(), br#"["cGl4ZWxz","pixels","cGl4ZWxz"]"#);
    }

    #[test]
    fn hostile_frames_are_errors() {
        let mut json = [0u8; 64];
        for bad in [
            &[][..],
            &[tag::OBJECT],
            &[tag::ARRAY, 1],
            &[tag::BACKREF, 0],
            &[tag::OBJECT, 0, 1, tag::OBJECT_END],
            &[tag::STRING, 0x80],
            &[tag::STRING, 5, b'a'],
            &[0xBF],
            &[tag::OBJECT, tag::ARRAY],
            &[1, 2],
            &[tag::DECIMAL_POS, 0xFE, 0xFF, 0xFF, 0xFF, 0x0F, 1],
        ] {
            let mut out = SliceJsonOut::new(&mut json);
            assert!(
                decode(&Dictionary::EMPTY, bad, &mut out).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn deep_nesting_is_refused_not_overflowed() {
        let packed = [tag::ARRAY; 100];
        let mut json = [0u8; 256];
        let mut out = SliceJsonOut::new(&mut json);
        assert_eq!(
            decode(&Dictionary::EMPTY, &packed, &mut out),
            Err(DecodeError::TooDeep)
        );
    }

    #[test]
    fn a_small_output_is_full_not_a_panic() {
        let packed = [tag::ARRAY, 5, 6, tag::ARRAY_END];
        for size in 0..5 {
            let mut json = [0u8; 8];
            let mut out = SliceJsonOut::new(&mut json[..size]);
            assert_eq!(
                decode(&Dictionary::EMPTY, &packed, &mut out),
                Err(DecodeError::OutputFull)
            );
        }
    }
}
