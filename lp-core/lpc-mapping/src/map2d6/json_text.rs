//! JSON text, read a token at a time: what the format-6 reader reads its
//! document with (`doc_read.rs`).
//!
//! **Why not serde.** The document structs' derived deserializers cost the
//! C6 engine about 30 KB of flash: a visitor, a field enum and an error path
//! per struct and enum, each monomorphized over serde_json's reader. This
//! reader is not generic, so it is compiled once; a document struct's reader
//! is a short loop over its keys; and a failure is a unit value
//! ([`Fail`]) whose words wait in the reader, so `?` moves nothing. It
//! builds no value tree: the document is read straight into its structs.
//!
//! **It accepts what serde_json accepted** for these documents (RFC 8259
//! JSON, nothing more): no comments, no trailing commas, no leading zeros, no
//! control characters in strings, no lone surrogates in a `\u` escape of a
//! string that is read, only whitespace after the document, and no more than
//! 127 nested objects and arrays in what is read (serde_json's recursion
//! limit; a value skipped as unknown may nest without limit, as there).
//! Numbers read as serde_json read them into the documents' types: a `u32`
//! is an integer with no sign, fraction or exponent, in range; an `f32` is
//! any number whose value is finite as an `f64` (serde_json reads `f64`, then
//! narrows), so `1e39` reads as infinity and `1e400` is refused.

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::map2d6::map2d6_error::Map2d6Error;

/// The document did not read; [`JsonText::error`] says why.
pub(crate) struct Fail;

pub(crate) type Read<T> = Result<T, Fail>;

/// serde_json's recursion limit: entering the 128th nested object or array
/// is refused.
const DEPTH_LIMIT: u8 = 128;

pub(crate) struct JsonText<'a> {
    text: &'a str,
    at: usize,
    /// Objects and arrays that may still be entered.
    depth_left: u8,
    /// The container just entered has no member yet (no comma before the
    /// next one). Set by `begin_*`, cleared by every `next_*`.
    first: bool,
    /// Why it failed, once it has.
    error: String,
}

impl<'a> JsonText<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            at: 0,
            depth_left: DEPTH_LIMIT,
            first: false,
            error: String::new(),
        }
    }

    /// Only whitespace is left.
    pub fn end(&mut self) -> Read<()> {
        match self.peek() {
            None => Ok(()),
            Some(_) => Err(self.fail("trailing characters")),
        }
    }

    /// The refusal for the [`Fail`] just returned.
    pub fn error(&mut self) -> Map2d6Error {
        Map2d6Error::Parse(core::mem::take(&mut self.error))
    }

    // ---- objects and arrays ----------------------------------------------

    /// `{`
    pub fn begin_object(&mut self) -> Read<()> {
        self.begin(b'{', "expected an object")
    }

    /// The next key of the object begun, or `None` past its `}`.
    pub fn next_key(&mut self) -> Read<Option<Cow<'a, str>>> {
        if !self.next_member(b'}')? {
            self.depth_left += 1;
            return Ok(None);
        }
        let key = self.key()?;
        Ok(Some(key))
    }

    /// `[`
    pub fn begin_array(&mut self) -> Read<()> {
        self.begin(b'[', "expected an array")
    }

    /// Is there another element in the array begun (`false` past its `]`)?
    pub fn next_element(&mut self) -> Read<bool> {
        let more = self.next_member(b']')?;
        if !more {
            self.depth_left += 1;
        }
        Ok(more)
    }

    /// Mark a struct's field read, refusing it the second time (`seen` is
    /// the struct's fields so far, one bit each).
    pub fn once(&mut self, seen: &mut u32, bit: u32, key: &str) -> Read<()> {
        if *seen & (1 << bit) != 0 {
            return Err(self.fail_naming("duplicate field", key));
        }
        *seen |= 1 << bit;
        Ok(())
    }

    /// A struct's field that has no default, missing.
    #[cold]
    pub fn missing(&mut self, field: &str) -> Fail {
        self.fail_naming("missing field", field)
    }

    fn begin(&mut self, open: u8, what: &str) -> Read<()> {
        self.expect(open, what)?;
        self.depth_left -= 1;
        if self.depth_left == 0 {
            return Err(self.fail("recursion limit exceeded"));
        }
        self.first = true;
        Ok(())
    }

    /// Past a `,` to the next member (`true`), or past `close` (`false`).
    fn next_member(&mut self, close: u8) -> Read<bool> {
        let first = core::mem::replace(&mut self.first, false);
        match self.peek() {
            Some(b) if b == close => {
                self.at += 1;
                Ok(false)
            }
            Some(b',') if !first => {
                self.at += 1;
                match self.peek() {
                    Some(b) if b == close => Err(self.fail("trailing comma")),
                    _ => Ok(true),
                }
            }
            Some(_) if first => Ok(true),
            Some(_) => Err(self.fail("expected ',' or the end")),
            None => Err(self.fail("unexpected end")),
        }
    }

    /// `"key":`
    fn key(&mut self) -> Read<Cow<'a, str>> {
        if self.peek() != Some(b'"') {
            return Err(self.fail("expected a key"));
        }
        let key = self.string_cow()?;
        self.expect(b':', "expected ':'")?;
        Ok(key)
    }

    // ---- enums --------------------------------------------------------------

    /// An externally tagged enum's variant: `"word"` (`false`) or
    /// `{"word": value}` (`true`; read the value, then
    /// [`Self::end_variant`]).
    pub fn variant(&mut self) -> Read<(Cow<'a, str>, bool)> {
        match self.peek() {
            Some(b'"') => Ok((self.string_cow()?, false)),
            Some(b'{') => {
                self.begin_object()?;
                self.first = false;
                Ok((self.key()?, true))
            }
            _ => Err(self.fail("expected a string or an object of one key")),
        }
    }

    /// The `}` after a variant's value.
    pub fn end_variant(&mut self) -> Read<()> {
        self.expect(b'}', "expected '}'")?;
        self.depth_left += 1;
        Ok(())
    }

    /// Which of `words` a variant is (unknown words refused), and whether a
    /// value follows. A word at or past `with_value` must have one; a word
    /// before it is a unit variant, `"word"` or `{"word": null}`, read whole.
    pub fn variant_of(&mut self, words: &[&str], with_value: usize) -> Read<usize> {
        let (word, has_value) = self.variant()?;
        let Some(index) = words.iter().position(|w| **w == *word) else {
            return Err(self.fail_naming("unknown variant", &word));
        };
        match (index < with_value, has_value) {
            (true, false) => Ok(index),
            (true, true) => {
                if !self.null()? {
                    return Err(self.fail("expected null"));
                }
                self.end_variant()?;
                Ok(index)
            }
            (false, true) => Ok(index),
            (false, false) => Err(self.fail_naming("expected an object for", &word)),
        }
    }

    /// A unit variant named in `words`: its index.
    pub fn unit_variant(&mut self, words: &[&str]) -> Read<usize> {
        self.variant_of(words, words.len())
    }

    // ---- scalars ------------------------------------------------------------

    /// `null`, consumed; `false` (nothing consumed) for anything else.
    pub fn null(&mut self) -> Read<bool> {
        if self.peek() != Some(b'n') {
            return Ok(false);
        }
        self.literal("null")?;
        Ok(true)
    }

    pub fn bool(&mut self) -> Read<bool> {
        match self.peek() {
            Some(b't') => self.literal("true").map(|()| true),
            Some(b'f') => self.literal("false").map(|()| false),
            _ => Err(self.fail("expected a boolean")),
        }
    }

    pub fn string(&mut self) -> Read<String> {
        if self.peek() != Some(b'"') {
            return Err(self.fail("expected a string"));
        }
        Ok(self.string_cow()?.into_owned())
    }

    /// An integer from 0 to `u32::MAX`.
    pub fn u32(&mut self) -> Read<u32> {
        let (text, integer) = self.number()?;
        if !integer || text.starts_with('-') {
            return Err(self.fail("expected an unsigned integer"));
        }
        let mut n: u32 = 0;
        for b in text.bytes() {
            match n.checked_mul(10).and_then(|n| n.checked_add((b - b'0') as u32)) {
                Some(next) => n = next,
                None => return Err(self.fail("expected an integer that fits a u32")),
            }
        }
        Ok(n)
    }

    /// A number, narrowed to `f32` (module doc).
    pub fn f32(&mut self) -> Read<f32> {
        let (text, _) = self.number()?;
        if f64_overflows(text.as_bytes()) {
            return Err(self.fail("number out of range"));
        }
        // The grammar is checked, so this parse cannot fail.
        Ok(text.parse::<f32>().unwrap_or(0.0))
    }

    /// `[x, y, …]` of exactly `out.len()` numbers.
    pub fn f32s(&mut self, out: &mut [f32]) -> Read<()> {
        self.begin_array()?;
        for slot in out.iter_mut() {
            if !self.next_element()? {
                return Err(self.fail("too few numbers"));
            }
            *slot = self.f32()?;
        }
        if self.next_element()? {
            return Err(self.fail("too many numbers"));
        }
        Ok(())
    }

    /// `[x, y]`
    pub fn point(&mut self) -> Read<[f32; 2]> {
        let mut point = [0.0; 2];
        self.f32s(&mut point)?;
        Ok(point)
    }

    /// `[[x, y], …]`
    pub fn points(&mut self) -> Read<Vec<[f32; 2]>> {
        let mut points = Vec::new();
        self.begin_array()?;
        while self.next_element()? {
            points.push(self.point()?);
        }
        Ok(points)
    }

    /// Any value, read and dropped. It must still be JSON, but as in
    /// serde_json's `ignore_value` its nesting has no limit (it is walked
    /// with a stack, not recursion) and its strings may hold lone
    /// surrogates.
    pub fn skip(&mut self) -> Read<()> {
        // What is open, innermost last, one bit each (set for an object):
        // the first 64 in `bits`, any deeper in `deeper`, so a skip
        // allocates nothing until then.
        let mut depth = 0usize;
        let mut bits = 0u64;
        let mut deeper: Vec<bool> = Vec::new();
        loop {
            match self.peek() {
                Some(b @ (b'{' | b'[')) => {
                    self.at += 1;
                    self.first = true;
                    let object = b == b'{';
                    if depth < 64 {
                        bits = bits & !(1 << depth) | (object as u64) << depth;
                    } else {
                        deeper.push(object);
                    }
                    depth += 1;
                }
                Some(b'"') => self.skip_string()?,
                Some(b't' | b'f') => drop(self.bool()?),
                Some(b'n') => self.literal("null")?,
                _ => drop(self.number()?),
            }
            // Close what ends here, then step to the next member.
            loop {
                let Some(inner) = depth.checked_sub(1) else {
                    return Ok(());
                };
                let object = match inner.checked_sub(64) {
                    None => bits >> inner & 1 != 0,
                    Some(at) => deeper[at],
                };
                if !self.next_member(if object { b'}' } else { b']' })? {
                    depth = inner;
                    if depth >= 64 {
                        deeper.pop();
                    }
                    continue;
                }
                if object {
                    if self.peek() != Some(b'"') {
                        return Err(self.fail("expected a key"));
                    }
                    self.skip_string()?;
                    self.expect(b':', "expected ':'")?;
                }
                break;
            }
        }
    }

    // ---- the token underneath -------------------------------------------------

    /// The next byte after whitespace (not consumed).
    fn peek(&mut self) -> Option<u8> {
        let bytes = self.text.as_bytes();
        while self.at < bytes.len() && matches!(bytes[self.at], b' ' | b'\n' | b'\t' | b'\r') {
            self.at += 1;
        }
        bytes.get(self.at).copied()
    }

    fn byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn expect(&mut self, byte: u8, what: &str) -> Read<()> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.fail(what))
        }
    }

    fn literal(&mut self, word: &str) -> Read<()> {
        if self.text[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(())
        } else {
            Err(self.fail("expected a value"))
        }
    }

    /// A string at `"`, its escapes read; borrowed when it has none.
    fn string_cow(&mut self) -> Read<Cow<'a, str>> {
        self.at += 1;
        let mut owned: Option<String> = None;
        let mut run = self.at;
        loop {
            match self.byte() {
                None => return Err(self.fail("unterminated string")),
                Some(b'"') => {
                    let tail = &self.text[run..self.at];
                    self.at += 1;
                    return Ok(match owned {
                        None => Cow::Borrowed(tail),
                        Some(mut s) => {
                            s.push_str(tail);
                            Cow::Owned(s)
                        }
                    });
                }
                Some(b'\\') => {
                    let s = owned.get_or_insert_with(String::new);
                    s.push_str(&self.text[run..self.at]);
                    self.at += 1;
                    let escaped = match self.escape()? {
                        Some(c) => c,
                        None => self.unicode_escape()?,
                    };
                    owned.get_or_insert_with(String::new).push(escaped);
                    run = self.at;
                }
                Some(0x00..=0x1f) => return Err(self.fail("control character in a string")),
                Some(_) => self.at += 1,
            }
        }
    }

    /// A string at `"`, checked and dropped (lone surrogates allowed).
    fn skip_string(&mut self) -> Read<()> {
        self.at += 1;
        loop {
            match self.byte() {
                None => return Err(self.fail("unterminated string")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(());
                }
                Some(b'\\') => {
                    self.at += 1;
                    if self.escape()?.is_none() {
                        self.hex4()?;
                    }
                }
                Some(0x00..=0x1f) => return Err(self.fail("control character in a string")),
                Some(_) => self.at += 1,
            }
        }
    }

    /// The letter after `\`, consumed: its character, or `None` for `u`
    /// (its four digits not yet read).
    fn escape(&mut self) -> Read<Option<char>> {
        let c = match self.byte() {
            Some(b'"') => '"',
            Some(b'\\') => '\\',
            Some(b'/') => '/',
            Some(b'b') => '\u{8}',
            Some(b'f') => '\u{c}',
            Some(b'n') => '\n',
            Some(b'r') => '\r',
            Some(b't') => '\t',
            Some(b'u') => {
                self.at += 1;
                return Ok(None);
            }
            _ => return Err(self.fail("invalid escape")),
        };
        self.at += 1;
        Ok(Some(c))
    }

    /// After `\u`: four hex digits, and a low surrogate's escape after a
    /// high one.
    fn unicode_escape(&mut self) -> Read<char> {
        let high = self.hex4()?;
        let code = match high {
            0xD800..=0xDBFF => {
                if !self.text[self.at..].starts_with("\\u") {
                    return Err(self.fail("lone leading surrogate in hex escape"));
                }
                self.at += 2;
                let low = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&low) {
                    return Err(self.fail("lone leading surrogate in hex escape"));
                }
                0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
            }
            0xDC00..=0xDFFF => return Err(self.fail("lone trailing surrogate in hex escape")),
            code => code,
        };
        // Every code left is a scalar value.
        Ok(char::from_u32(code).unwrap_or('\u{fffd}'))
    }

    fn hex4(&mut self) -> Read<u32> {
        let mut n = 0;
        for _ in 0..4 {
            let Some(digit) = self.byte().and_then(|b| (b as char).to_digit(16)) else {
                return Err(self.fail("invalid escape"));
            };
            n = n * 16 + digit;
            self.at += 1;
        }
        Ok(n)
    }

    /// A number's text, its grammar checked, and whether it is an integer
    /// (no fraction, no exponent).
    fn number(&mut self) -> Read<(&'a str, bool)> {
        let start = match self.peek() {
            Some(b'-' | b'0'..=b'9') => self.at,
            _ => return Err(self.fail("expected a value")),
        };
        if self.byte() == Some(b'-') {
            self.at += 1;
        }
        match self.byte() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => drop(self.digits()),
            _ => return Err(self.fail("invalid number")),
        }
        let mut integer = true;
        if self.byte() == Some(b'.') {
            integer = false;
            self.at += 1;
            if self.digits() == 0 {
                return Err(self.fail("invalid number"));
            }
        }
        if matches!(self.byte(), Some(b'e' | b'E')) {
            integer = false;
            self.at += 1;
            if matches!(self.byte(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                return Err(self.fail("invalid number"));
            }
        } else if integer && self.byte().is_some_and(|b| b.is_ascii_digit()) {
            // A digit after a leading `0`.
            return Err(self.fail("invalid number"));
        }
        Ok((&self.text[start..self.at], integer))
    }

    /// Digits, consumed; how many.
    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.byte().is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
        self.at - start
    }

    /// Fail here, with the line and column.
    #[cold]
    pub fn fail(&mut self, what: &str) -> Fail {
        let before = &self.text.as_bytes()[..self.at.min(self.text.len())];
        let line = before.iter().filter(|b| **b == b'\n').count() + 1;
        let column = before.iter().rev().take_while(|b| **b != b'\n').count() + 1;
        self.error = format!("{what} at line {line} column {column}");
        Fail
    }

    #[cold]
    fn fail_naming(&mut self, what: &str, name: &str) -> Fail {
        self.fail(&format!("{what} {name:?}"))
    }
}

/// Is a checked number's value beyond `f64` (serde_json refuses those: it
/// reads every number as an `f64` first)?
fn f64_overflows(text: &[u8]) -> bool {
    let end = text.iter().position(|c| matches!(c, b'e' | b'E'));
    let (mantissa, mut exponent) = (&text[..end.unwrap_or(text.len())], 0i64);
    if let Some(end) = end {
        for &c in &text[end + 1..] {
            if c.is_ascii_digit() {
                exponent = exponent.saturating_mul(10).saturating_add((c - b'0') as i64);
            }
        }
        if text[end + 1] == b'-' {
            exponent = -exponent;
        }
    }
    let whole = mantissa.iter().take_while(|c| **c != b'.').filter(|c| c.is_ascii_digit());
    let mut power = whole.count() as i64 - 1;
    let mut digits = mantissa.iter().copied().filter(u8::is_ascii_digit);
    // The first significant digit and its power of ten.
    let first = loop {
        match digits.next() {
            None => return false, // zero
            Some(b'0') => power -= 1,
            Some(d) => break d,
        }
    };
    let magnitude = power.saturating_add(exponent);
    if magnitude != 308 {
        return magnitude > 308;
    }
    // f64::MAX is 1.7976931348623157e308; a decimal rounds to infinity from
    // halfway past it.
    let mut rest = core::iter::once(first).chain(digits);
    for &limit in b"179769313486231580793728971" {
        match rest.next() {
            None => return false,
            Some(d) if d != limit => return d > limit,
            Some(_) => {}
        }
    }
    rest.any(|d| d != b'0')
}
