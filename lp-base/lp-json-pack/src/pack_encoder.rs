//! The encoder: pack events in, one JSON Pack frame out, into a caller-owned
//! buffer. No heap and no second buffer.
//!
//! The events mirror JSON's grammar one for one. The caller emits a
//! well-formed sequence (one value; inside a map, `key` before every value);
//! the encoder does not re-check the structure, because its callers (a
//! serializer, or the [`PackLexer`](crate::PackLexer)) already have it.
//!
//! Every event returns `Err(PackError::Full)` when the buffer runs out, and
//! the error is sticky: once an event failed, every later one fails the same
//! way, so a caller can check only at the end.

use crate::pack_decimal::{PackNumber, classify_number};
use crate::pack_dictionary::Dictionary;
use crate::pack_learned::LearnStore;
use crate::pack_tags as tag;
use crate::pack_varint::{VARINT_MAX_LEN, write_varint, zigzag};
use crate::{MAX_BACKREFS, PackError, is_backref_candidate};

/// Room reserved in front of a string whose text is written before its
/// encoding is known (the lexer's strings): a tag byte plus a LEB128 length of
/// up to three bytes, so strings up to 2 MiB.
#[cfg(feature = "lex")]
pub(crate) const TENTATIVE_RESERVE: usize = 4;

/// Streaming JSON Pack encoder over `out`.
pub struct PackEncoder<'a> {
    out: &'a mut [u8],
    len: usize,
    dict: &'static Dictionary,
    backref_at: [u32; MAX_BACKREFS],
    backref_len: [u16; MAX_BACKREFS],
    backref_count: usize,
    failed: Option<PackError>,
    /// The link's learned table, coded after `dict` ([`crate::pack_learned`]).
    learned: Option<&'a mut dyn LearnStore>,
}

/// What a key or string becomes.
enum TextCode {
    Dict(usize),
    Backref(usize),
    Inline,
}

impl<'a> PackEncoder<'a> {
    /// A new frame written from the start of `out`, coded against `dict`.
    pub fn new(out: &'a mut [u8], dict: &'static Dictionary) -> Self {
        Self {
            out,
            len: 0,
            dict,
            backref_at: [0; MAX_BACKREFS],
            backref_len: [0; MAX_BACKREFS],
            backref_count: 0,
            failed: None,
            learned: None,
        }
    }

    /// A learned frame coded against `dict ++ learned`, learning as it goes.
    /// The header is written first. If the frame is then not sent, the caller
    /// truncates `learned` to the [`LearnMark`](crate::LearnMark) it took
    /// before this call.
    pub fn with_learned(
        out: &'a mut [u8],
        dict: &'static Dictionary,
        learned: &'a mut dyn LearnStore,
    ) -> Self {
        let mut e = Self::new(out, dict);
        match crate::pack_learned::write_header(e.out, &*learned) {
            Some(n) => e.len = n,
            None => e.failed = Some(PackError::Full),
        }
        e.learned = Some(learned);
        e
    }

    /// Bytes written so far.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The dictionary this frame is coded against.
    pub fn dictionary(&self) -> &'static Dictionary {
        self.dict
    }

    /// The first error any event hit, if one did.
    pub fn error(&self) -> Option<PackError> {
        self.failed
    }

    /// The frame's length, or the first error any event hit.
    pub fn finish(self) -> Result<usize, PackError> {
        match self.failed {
            Some(e) => Err(e),
            None => Ok(self.len),
        }
    }

    /// `{`
    pub fn begin_map(&mut self) -> Result<(), PackError> {
        self.guard(|e| e.put(tag::OBJECT))
    }

    /// `}`
    pub fn end_map(&mut self) -> Result<(), PackError> {
        self.guard(|e| e.put(tag::OBJECT_END))
    }

    /// `[`
    pub fn begin_seq(&mut self) -> Result<(), PackError> {
        self.guard(|e| e.put(tag::ARRAY))
    }

    /// `]`
    pub fn end_seq(&mut self) -> Result<(), PackError> {
        self.guard(|e| e.put(tag::ARRAY_END))
    }

    /// An object key (unescaped text).
    pub fn key(&mut self, key: &str) -> Result<(), PackError> {
        self.guard(|e| {
            let k = key.as_bytes();
            match e.lookup(k, true) {
                TextCode::Dict(i) => e.put_key_code(i),
                TextCode::Backref(n) => e.put_tagged_varint(tag::KEY_BACKREF, n as u64),
                TextCode::Inline => {
                    e.put_tagged_varint(tag::KEY_INLINE, k.len() as u64)?;
                    e.put_text(k)?;
                    if let Some(l) = e.learned.as_deref_mut() {
                        l.learn_key(k);
                    }
                    Ok(())
                }
            }
        })
    }

    /// A string value (unescaped text).
    pub fn str(&mut self, value: &str) -> Result<(), PackError> {
        self.guard(|e| {
            let v = value.as_bytes();
            match e.lookup(v, false) {
                TextCode::Dict(i) => e.put_value_code(i),
                TextCode::Backref(n) => e.put_tagged_varint(tag::BACKREF, n as u64),
                TextCode::Inline => {
                    if v.len() <= tag::STR_INLINE_MAX_LEN {
                        e.put(tag::STR_INLINE_BASE + v.len() as u8)?;
                    } else {
                        e.put_tagged_varint(tag::STRING, v.len() as u64)?;
                    }
                    e.put_text(v)?;
                    if let Some(l) = e.learned.as_deref_mut() {
                        l.learn_value(v);
                    }
                    Ok(())
                }
            }
        })
    }

    /// An unsigned integer.
    pub fn u64(&mut self, v: u64) -> Result<(), PackError> {
        self.guard(|e| e.put_uint(v))
    }

    /// A signed integer.
    pub fn i64(&mut self, v: i64) -> Result<(), PackError> {
        self.guard(|e| {
            if v >= 0 {
                e.put_uint(v as u64)
            } else {
                e.put_tagged_varint(tag::NEG_INT, v.unsigned_abs())
            }
        })
    }

    /// A number already printed as JSON text (a float as `ryu-js` prints it).
    /// It decodes to exactly this text. Text that is not number-shaped
    /// (`[0-9.eE+-]`, non-empty) is [`PackError::Malformed`].
    pub fn decimal_text(&mut self, text: &str) -> Result<(), PackError> {
        self.guard(|e| e.put_number(text.as_bytes()))
    }

    /// `true` or `false`.
    pub fn bool(&mut self, v: bool) -> Result<(), PackError> {
        self.guard(|e| e.put(if v { tag::TRUE } else { tag::FALSE }))
    }

    /// `null`.
    pub fn null(&mut self) -> Result<(), PackError> {
        self.guard(|e| e.put(tag::NULL))
    }

    /// Raw bytes that JSON carries as a padded standard-base64 string. They
    /// travel raw and decode to that base64 text.
    /// A blob seen earlier in the frame becomes a back-reference.
    pub fn blob(&mut self, bytes: &[u8]) -> Result<(), PackError> {
        self.guard(|e| {
            if let Some(r) = e.find_backref(bytes) {
                return e.put_tagged_varint(tag::BLOB_BACKREF, r as u64);
            }
            e.put_tagged_varint(tag::BLOB, bytes.len() as u64)?;
            e.put_text(bytes)
        })
    }

    /// One complete JSON value given as text (a pre-serialized `RawValue`),
    /// lexed into this frame. It shares the frame's back-references.
    #[cfg(feature = "lex")]
    pub fn json_value(&mut self, text: &[u8]) -> Result<(), PackError> {
        let mut lexer = crate::PackLexer::new();
        lexer.push(self, text)?;
        lexer.finish(self)
    }

    // ---- the lexer's side door -------------------------------------------

    /// Fail with the sticky error, if there is one.
    pub(crate) fn check(&self) -> Result<(), PackError> {
        match self.failed {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Record `e` as the frame's error and return it.
    pub(crate) fn fail(&mut self, e: PackError) -> PackError {
        self.failed.get_or_insert(e);
        e
    }

    /// Start a string whose text is written before its encoding is known.
    /// Returns where it starts; pass that to [`Self::end_tentative`].
    #[cfg(feature = "lex")]
    pub(crate) fn begin_tentative(&mut self) -> Result<usize, PackError> {
        self.guard_value(|e| {
            let at = e.len;
            if at + TENTATIVE_RESERVE > e.out.len() {
                return Err(PackError::Full);
            }
            e.len += TENTATIVE_RESERVE;
            Ok(at)
        })
    }

    /// Append unescaped text to the open tentative string.
    #[cfg(feature = "lex")]
    pub(crate) fn push_tentative(&mut self, text: &[u8]) -> Result<(), PackError> {
        self.guard(|e| e.put_all(text))
    }

    /// Close the tentative string that began at `start`, rewriting it in place
    /// into its final form (a code, a back-reference, or inline text).
    #[cfg(feature = "lex")]
    pub(crate) fn end_tentative(&mut self, start: usize, is_key: bool) -> Result<(), PackError> {
        self.guard(|e| {
            let text_at = start + TENTATIVE_RESERVE;
            let n = e.len - text_at;
            let code = {
                let out = &*e.out;
                e.lookup_in(&out[text_at..text_at + n], is_key)
            };
            match code {
                TextCode::Dict(i) => {
                    e.len = start;
                    if is_key {
                        e.put_key_code(i)
                    } else {
                        e.put_value_code(i)
                    }
                }
                TextCode::Backref(r) => {
                    e.len = start;
                    let t = if is_key {
                        tag::KEY_BACKREF
                    } else {
                        tag::BACKREF
                    };
                    e.put_tagged_varint(t, r as u64)
                }
                TextCode::Inline => {
                    let mut hdr = [0u8; 1 + VARINT_MAX_LEN];
                    let h = if is_key {
                        hdr[0] = tag::KEY_INLINE;
                        1 + write_varint(&mut hdr[1..], n as u64)
                    } else if n <= tag::STR_INLINE_MAX_LEN {
                        hdr[0] = tag::STR_INLINE_BASE + n as u8;
                        1
                    } else {
                        hdr[0] = tag::STRING;
                        1 + write_varint(&mut hdr[1..], n as u64)
                    };
                    if h > TENTATIVE_RESERVE {
                        return Err(PackError::Full);
                    }
                    e.out[start..start + h].copy_from_slice(&hdr[..h]);
                    e.out.copy_within(text_at..text_at + n, start + h);
                    e.len = start + h + n;
                    e.remember(start + h, n);
                    if let Some(l) = e.learned.as_deref_mut() {
                        let t = &e.out[start + h..start + h + n];
                        if is_key {
                            l.learn_key(t);
                        } else {
                            l.learn_value(t);
                        }
                    }
                    Ok(())
                }
            }
        })
    }

    /// A number token from JSON text.
    #[cfg(feature = "lex")]
    pub(crate) fn number_bytes(&mut self, text: &[u8]) -> Result<(), PackError> {
        self.guard(|e| e.put_number(text))
    }

    // ---- internals --------------------------------------------------------

    fn guard(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<(), PackError>,
    ) -> Result<(), PackError> {
        self.guard_value(f)
    }

    fn guard_value<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, PackError>,
    ) -> Result<T, PackError> {
        self.check()?;
        f(self).map_err(|e| self.fail(e))
    }

    fn lookup(&self, text: &[u8], is_key: bool) -> TextCode {
        self.lookup_in(text, is_key)
    }

    /// Dictionary first, then this frame's earlier inline texts. `text` may
    /// lie inside `self.out` (the lexer's tentative strings).
    fn lookup_in(&self, text: &[u8], is_key: bool) -> TextCode {
        let found = if is_key {
            self.dict.find_key(text).filter(|&i| i < tag::KEY_DICT_MAX)
        } else {
            self.dict.find_value(text)
        };
        if let Some(i) = found {
            return TextCode::Dict(i);
        }
        if let Some(l) = self.learned.as_deref() {
            let (base, hit) = if is_key {
                (self.dict.keys.len(), l.find_key(text))
            } else {
                (self.dict.values.len(), l.find_value(text))
            };
            if let Some(i) = hit.map(|i| base + i)
                && (!is_key || i < tag::KEY_DICT_MAX)
            {
                return TextCode::Dict(i);
            }
        }
        match self.find_backref(text) {
            Some(r) => TextCode::Backref(r),
            None => TextCode::Inline,
        }
    }

    /// An earlier inline text or blob of this frame with exactly these bytes.
    fn find_backref(&self, bytes: &[u8]) -> Option<usize> {
        if !is_backref_candidate(bytes.len()) {
            return None;
        }
        (0..self.backref_count).find(|&r| {
            let at = self.backref_at[r] as usize;
            usize::from(self.backref_len[r]) == bytes.len()
                && self.out[at..at + bytes.len()] == *bytes
        })
    }

    fn remember(&mut self, at: usize, len: usize) {
        if is_backref_candidate(len) && self.backref_count < MAX_BACKREFS {
            self.backref_at[self.backref_count] = at as u32;
            self.backref_len[self.backref_count] = len as u16;
            self.backref_count += 1;
        }
    }

    fn put_key_code(&mut self, i: usize) -> Result<(), PackError> {
        if i < tag::KEY_DICT_INLINE_COUNT {
            return self.put(i as u8);
        }
        let wide = i - tag::KEY_DICT_INLINE_COUNT;
        self.put(tag::KEY_DICT_WIDE_BASE + (wide >> 8) as u8)?;
        self.put(wide as u8)
    }

    fn put_value_code(&mut self, i: usize) -> Result<(), PackError> {
        if i < tag::VALUE_DICT_INLINE_COUNT {
            return self.put(tag::VALUE_DICT_INLINE_BASE + i as u8);
        }
        if i < tag::VALUE_CODE_SHORT_COUNT {
            return self.put(tag::VALUE_DICT_HIGH_BASE + (i - tag::VALUE_DICT_INLINE_COUNT) as u8);
        }
        self.put_tagged_varint(tag::VALUE_DICT, (i - tag::VALUE_CODE_SHORT_COUNT) as u64)
    }

    fn put_uint(&mut self, v: u64) -> Result<(), PackError> {
        if v <= u64::from(tag::UINT_INLINE_MAX) {
            return self.put(v as u8);
        }
        self.put_tagged_varint(tag::UINT, v)
    }

    fn put_number(&mut self, text: &[u8]) -> Result<(), PackError> {
        if text.is_empty()
            || !text
                .iter()
                .all(|c| matches!(c, b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-'))
        {
            return Err(PackError::Malformed);
        }
        match classify_number(text) {
            PackNumber::Int { neg: false, mag } => self.put_uint(mag),
            PackNumber::Int { neg: true, mag } => self.put_tagged_varint(tag::NEG_INT, mag),
            PackNumber::Decimal { neg, coeff, exp } => {
                let t = if neg {
                    tag::DECIMAL_NEG
                } else {
                    tag::DECIMAL_POS
                };
                self.put_tagged_varint(t, zigzag(i64::from(exp)))?;
                self.put_varint(coeff)
            }
            PackNumber::Text => {
                self.put_tagged_varint(tag::NUMBER_TEXT, text.len() as u64)?;
                self.put_all(text)
            }
        }
    }

    /// Inline text or blob bytes: written, and numbered for back-references.
    fn put_text(&mut self, text: &[u8]) -> Result<(), PackError> {
        let at = self.len;
        self.put_all(text)?;
        self.remember(at, text.len());
        Ok(())
    }

    fn put(&mut self, b: u8) -> Result<(), PackError> {
        *self.out.get_mut(self.len).ok_or(PackError::Full)? = b;
        self.len += 1;
        Ok(())
    }

    fn put_all(&mut self, s: &[u8]) -> Result<(), PackError> {
        let end = self.len.checked_add(s.len()).ok_or(PackError::Full)?;
        self.out
            .get_mut(self.len..end)
            .ok_or(PackError::Full)?
            .copy_from_slice(s);
        self.len = end;
        Ok(())
    }

    fn put_varint(&mut self, v: u64) -> Result<(), PackError> {
        let mut buf = [0u8; VARINT_MAX_LEN];
        let n = write_varint(&mut buf, v);
        self.put_all(&buf[..n])
    }

    fn put_tagged_varint(&mut self, t: u8, v: u64) -> Result<(), PackError> {
        let mut buf = [0u8; 1 + VARINT_MAX_LEN];
        buf[0] = t;
        let n = 1 + write_varint(&mut buf[1..], v);
        self.put_all(&buf[..n])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack_dictionary::{HASH_EMPTY, PackStrings};

    static DICT: Dictionary = Dictionary {
        keys: PackStrings {
            text: "kindvalue",
            offsets: &[0, 4, 9],
        },
        values: PackStrings {
            text: "rgb",
            offsets: &[0, 3],
        },
        key_hash: &[HASH_EMPTY, HASH_EMPTY, 1, 0],
        value_hash: &[0, HASH_EMPTY],
    };

    fn encode(f: impl FnOnce(&mut PackEncoder<'_>) -> Result<(), PackError>) -> ([u8; 128], usize) {
        let mut buf = [0u8; 128];
        let mut e = PackEncoder::new(&mut buf, &DICT);
        f(&mut e).unwrap();
        let n = e.finish().unwrap();
        (buf, n)
    }

    #[test]
    fn small_values_ride_in_the_tag() {
        let (b, n) = encode(|e| {
            e.begin_seq()?;
            e.u64(5)?;
            e.u64(64)?;
            e.i64(-2)?;
            e.bool(true)?;
            e.null()?;
            e.str("rgb")?;
            e.str("hi")?;
            e.str("hi")?;
            e.end_seq()
        });
        assert_eq!(
            &b[..n],
            &[
                tag::ARRAY,
                5,
                tag::UINT,
                64,
                tag::NEG_INT,
                2,
                tag::TRUE,
                tag::NULL,
                tag::VALUE_DICT_INLINE_BASE,
                tag::STR_INLINE_BASE + 2,
                b'h',
                b'i',
                tag::BACKREF,
                0,
                tag::ARRAY_END
            ]
        );
    }

    #[test]
    fn keys_use_codes_then_backrefs() {
        let (b, n) = encode(|e| {
            e.begin_map()?;
            e.key("value")?;
            e.u64(1)?;
            e.key("other")?;
            e.u64(2)?;
            e.key("other")?;
            e.u64(3)?;
            e.end_map()
        });
        assert_eq!(
            &b[..n],
            &[
                tag::OBJECT,
                1,
                1,
                tag::KEY_INLINE,
                5,
                b'o',
                b't',
                b'h',
                b'e',
                b'r',
                2,
                tag::KEY_BACKREF,
                0,
                3,
                tag::OBJECT_END
            ]
        );
    }

    #[test]
    fn full_is_sticky_and_never_panics() {
        for size in 0..20 {
            let mut buf = [0u8; 20];
            let mut e = PackEncoder::new(&mut buf[..size], &DICT);
            let r = (|| {
                e.begin_map()?;
                e.key("a long inline key")?;
                e.blob(&[1, 2, 3])?;
                e.end_map()
            })();
            if r.is_err() {
                assert_eq!(e.u64(1), Err(PackError::Full));
                assert_eq!(e.finish(), Err(PackError::Full));
            } else {
                assert!(e.finish().is_ok());
            }
        }
    }

    #[test]
    fn decimal_text_rejects_non_numbers() {
        let mut buf = [0u8; 8];
        let mut e = PackEncoder::new(&mut buf, &DICT);
        assert_eq!(e.decimal_text("\"x\""), Err(PackError::Malformed));
        assert_eq!(e.decimal_text(""), Err(PackError::Malformed));
    }
}
