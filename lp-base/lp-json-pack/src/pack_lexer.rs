//! The lexer (feature `lex`): JSON text in, pack events out, into the same
//! [`PackEncoder`] the event API writes. Text and events can mix in one frame,
//! which is how a pre-serialized `RawValue` inside a typed message packs.
//!
//! It is a byte-at-a-time state machine, fed slices of any size. A string's
//! text is written *tentatively* straight into the output, and when its closing
//! quote arrives the encoder rewrites it in place into its final form (a
//! dictionary code, a back-reference, or inline text). The rewrite only moves
//! that string's own bytes a few positions, so there is no second buffer.
//!
//! The lexer accepts exactly the JSON whose byte-identical text the decoder
//! reproduces, and refuses the rest with [`PackError::Malformed`] rather than
//! pack something that would decode differently:
//! - no whitespace between tokens (the wire's serializers write none);
//! - string escapes only as `ser-write-json` writes them: `\"` `\\` `\b` `\f`
//!   `\n` `\r` `\t`, and `\u00XX` (upper-case hex) for the other C0 controls;
//!   no `\/`, no other `\u`, no raw control bytes;
//! - numbers are anything number-shaped; the ones whose text the decimal form
//!   cannot reproduce travel verbatim.
//!
//! A caller that gets `Malformed` sends that message as JSON instead.

use crate::PackError;
use crate::pack_encoder::PackEncoder;

/// Longest number token (`ryu-js` prints at most ~25 bytes).
const NUMBER_MAX: usize = 40;
/// Deepest nesting the lexer tracks (one bit per level).
const MAX_DEPTH: u32 = 64;

/// What the grammar allows next, between tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// A value (the top level, after `:`, after `,` in an array).
    Value,
    /// A value or `]` (just after `[`).
    ValueOrEnd,
    /// A key or `}` (just after `{`).
    KeyOrEnd,
    /// A key (after `,` in an object).
    Key,
    /// `:` after a key.
    Colon,
    /// `,` or the container's close.
    CommaOrEnd,
    /// Nothing: the top-level value is complete.
    Done,
}

/// Where inside a token the lexer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Between,
    Str,
    StrEscape,
    StrUnicode,
    Number,
    Literal,
}

/// Streaming JSON-text lexer. Holds only lexer state; the output is the
/// [`PackEncoder`] passed to every call, so both can live side by side in a
/// serializer sink.
pub struct PackLexer {
    expect: Expect,
    token: Token,
    /// One bit per open container, 1 = object.
    stack: u64,
    depth: u32,
    str_at: usize,
    str_is_key: bool,
    unicode: [u8; 4],
    unicode_len: u8,
    number: [u8; NUMBER_MAX],
    number_len: usize,
    literal: &'static [u8],
    literal_at: u8,
}

impl Default for PackLexer {
    fn default() -> Self {
        Self::new()
    }
}

impl PackLexer {
    /// A lexer expecting one complete JSON value.
    pub const fn new() -> Self {
        Self {
            expect: Expect::Value,
            token: Token::Between,
            stack: 0,
            depth: 0,
            str_at: 0,
            str_is_key: false,
            unicode: [0; 4],
            unicode_len: 0,
            number: [0; NUMBER_MAX],
            number_len: 0,
            literal: b"",
            literal_at: 0,
        }
    }

    /// Feed the next slice of JSON text into `enc`.
    pub fn push(&mut self, enc: &mut PackEncoder<'_>, bytes: &[u8]) -> Result<(), PackError> {
        enc.check()?;
        let mut i = 0;
        while i < bytes.len() {
            let r = if self.token == Token::Str {
                // A string's plain run goes out in one copy; the serializer
                // writes whole string slices, so this is most bytes.
                let rest = &bytes[i..];
                let run = rest
                    .iter()
                    .position(|&b| b == b'"' || b == b'\\' || b < 0x20)
                    .unwrap_or(rest.len());
                if run > 0 {
                    i += run;
                    enc.push_tentative(&rest[..run])
                } else {
                    i += 1;
                    self.step(enc, rest[0])
                }
            } else {
                i += 1;
                self.step(enc, bytes[i - 1])
            };
            if let Err(e) = r {
                return Err(enc.fail(e));
            }
        }
        Ok(())
    }

    /// The value is complete: flush a trailing number and check nothing is
    /// left open.
    pub fn finish(&mut self, enc: &mut PackEncoder<'_>) -> Result<(), PackError> {
        enc.check()?;
        if self.token == Token::Number
            && let Err(e) = self.flush_number(enc)
        {
            return Err(enc.fail(e));
        }
        if self.token != Token::Between || self.expect != Expect::Done {
            return Err(enc.fail(PackError::Malformed));
        }
        Ok(())
    }

    fn step(&mut self, enc: &mut PackEncoder<'_>, b: u8) -> Result<(), PackError> {
        match self.token {
            Token::Between => self.between(enc, b),
            Token::Str => self.string_byte(enc, b),
            Token::StrEscape => self.escape(enc, b),
            Token::StrUnicode => self.unicode_digit(enc, b),
            Token::Number => {
                if matches!(b, b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-') {
                    let slot = self
                        .number
                        .get_mut(self.number_len)
                        .ok_or(PackError::Malformed)?;
                    *slot = b;
                    self.number_len += 1;
                    Ok(())
                } else {
                    self.flush_number(enc)?;
                    self.between(enc, b)
                }
            }
            Token::Literal => {
                if self.literal.get(usize::from(self.literal_at)) != Some(&b) {
                    return Err(PackError::Malformed);
                }
                self.literal_at += 1;
                if usize::from(self.literal_at) == self.literal.len() {
                    self.token = Token::Between;
                    self.after_value();
                }
                Ok(())
            }
        }
    }

    fn between(&mut self, enc: &mut PackEncoder<'_>, b: u8) -> Result<(), PackError> {
        match b {
            b'"' => {
                self.str_is_key = match self.expect {
                    Expect::Key | Expect::KeyOrEnd => true,
                    Expect::Value | Expect::ValueOrEnd => false,
                    _ => return Err(PackError::Malformed),
                };
                self.str_at = enc.begin_tentative()?;
                self.token = Token::Str;
                Ok(())
            }
            b'{' | b'[' => {
                self.value_position()?;
                if self.depth == MAX_DEPTH {
                    return Err(PackError::Malformed);
                }
                let object = b == b'{';
                if object {
                    enc.begin_map()?;
                } else {
                    enc.begin_seq()?;
                }
                self.stack = (self.stack << 1) | u64::from(object);
                self.depth += 1;
                self.expect = if object {
                    Expect::KeyOrEnd
                } else {
                    Expect::ValueOrEnd
                };
                Ok(())
            }
            b'}' | b']' => {
                let object = b == b'}';
                let allowed = match self.expect {
                    Expect::CommaOrEnd => true,
                    Expect::KeyOrEnd => object,
                    Expect::ValueOrEnd => !object,
                    _ => false,
                };
                if !allowed || self.depth == 0 || self.top_is_object() != object {
                    return Err(PackError::Malformed);
                }
                if object {
                    enc.end_map()?;
                } else {
                    enc.end_seq()?;
                }
                self.stack >>= 1;
                self.depth -= 1;
                self.after_value();
                Ok(())
            }
            b',' if self.expect == Expect::CommaOrEnd => {
                self.expect = if self.top_is_object() {
                    Expect::Key
                } else {
                    Expect::Value
                };
                Ok(())
            }
            b':' if self.expect == Expect::Colon => {
                self.expect = Expect::Value;
                Ok(())
            }
            b'-' | b'0'..=b'9' => {
                self.value_position()?;
                self.number[0] = b;
                self.number_len = 1;
                self.token = Token::Number;
                Ok(())
            }
            b't' | b'f' | b'n' => {
                self.value_position()?;
                let (literal, r): (&'static [u8], _) = match b {
                    b't' => (b"true", enc.bool(true)),
                    b'f' => (b"false", enc.bool(false)),
                    _ => (b"null", enc.null()),
                };
                r?;
                self.literal = literal;
                self.literal_at = 1;
                self.token = Token::Literal;
                Ok(())
            }
            _ => Err(PackError::Malformed),
        }
    }

    fn string_byte(&mut self, enc: &mut PackEncoder<'_>, b: u8) -> Result<(), PackError> {
        match b {
            b'"' => {
                self.token = Token::Between;
                enc.end_tentative(self.str_at, self.str_is_key)?;
                if self.str_is_key {
                    self.expect = Expect::Colon;
                } else {
                    self.after_value();
                }
                Ok(())
            }
            b'\\' => {
                self.token = Token::StrEscape;
                Ok(())
            }
            0x00..=0x1F => Err(PackError::Malformed),
            _ => enc.push_tentative(&[b]),
        }
    }

    fn escape(&mut self, enc: &mut PackEncoder<'_>, b: u8) -> Result<(), PackError> {
        self.token = Token::Str;
        let c = match b {
            b'"' => b'"',
            b'\\' => b'\\',
            b'b' => 0x08,
            b'f' => 0x0C,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'u' => {
                self.token = Token::StrUnicode;
                self.unicode_len = 0;
                return Ok(());
            }
            _ => return Err(PackError::Malformed),
        };
        enc.push_tentative(&[c])
    }

    /// `\u00XX`: accepted only in the form the decoder prints back, which is a
    /// C0 control without a short escape, in upper-case hex.
    fn unicode_digit(&mut self, enc: &mut PackEncoder<'_>, b: u8) -> Result<(), PackError> {
        self.unicode[usize::from(self.unicode_len)] = b;
        self.unicode_len += 1;
        if self.unicode_len < 4 {
            return Ok(());
        }
        self.token = Token::Str;
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let u = &self.unicode;
        let hi = HEX.iter().position(|&h| h == u[2]);
        let lo = HEX.iter().position(|&h| h == u[3]);
        let (Some(hi), Some(lo)) = (hi, lo) else {
            return Err(PackError::Malformed);
        };
        let c = (hi * 16 + lo) as u8;
        let canonical = u[0] == b'0'
            && u[1] == b'0'
            && c < 0x20
            && !matches!(c, 0x08 | 0x09 | 0x0A | 0x0C | 0x0D);
        if !canonical {
            return Err(PackError::Malformed);
        }
        enc.push_tentative(&[c])
    }

    fn flush_number(&mut self, enc: &mut PackEncoder<'_>) -> Result<(), PackError> {
        self.token = Token::Between;
        enc.number_bytes(&self.number[..self.number_len])?;
        self.after_value();
        Ok(())
    }

    fn value_position(&self) -> Result<(), PackError> {
        match self.expect {
            Expect::Value | Expect::ValueOrEnd => Ok(()),
            _ => Err(PackError::Malformed),
        }
    }

    fn after_value(&mut self) {
        self.expect = if self.depth == 0 {
            Expect::Done
        } else {
            Expect::CommaOrEnd
        };
    }

    fn top_is_object(&self) -> bool {
        self.depth > 0 && self.stack & 1 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dictionary;

    fn pack(text: &[u8]) -> Result<usize, PackError> {
        let mut buf = [0u8; 256];
        let mut e = PackEncoder::new(&mut buf, &Dictionary::EMPTY);
        e.json_value(text)?;
        e.finish()
    }

    #[test]
    fn accepts_the_wire_shapes() {
        for t in [
            &br#"{"a":[1,-2,3.5,"x",true,false,null,{}],"b":[]}"#[..],
            br#""esc\"\\\n\t\u001F""#,
            b"-0",
            b"12",
            br#"[[[[]]]]"#,
        ] {
            assert!(pack(t).is_ok(), "{}", core::str::from_utf8(t).unwrap());
        }
    }

    #[test]
    fn refuses_what_would_not_round_trip() {
        for t in [
            &b"{\"a\": 1}"[..],
            br#""\/""#,
            b"\"\\u00e9\"",
            b"\"\\u0041\"",
            br#""\u000A""#,
            b"\"raw\x01\"",
            br#"{"a":1,}"#,
            br#"{"a"}"#,
            br#"[1 2]"#,
            b"tru",
            b"trux",
            b"1 ",
            br#"{"a":1}{"#,
            b"",
            br#"[}"#,
        ] {
            assert_eq!(
                pack(t),
                Err(PackError::Malformed),
                "{}",
                core::str::from_utf8(t).unwrap_or("?")
            );
        }
    }

    #[test]
    fn slices_of_any_size_pack_the_same() {
        let text = br#"{"kind":"a long string value over thirty-one bytes","n":[1,2.25,-7e-7],"kind":"a long string value over thirty-one bytes"}"#;
        let mut whole = [0u8; 256];
        let mut e = PackEncoder::new(&mut whole, &Dictionary::EMPTY);
        e.json_value(text).unwrap();
        let n = e.finish().unwrap();
        for step in 1..8 {
            let mut buf = [0u8; 256];
            let mut e = PackEncoder::new(&mut buf, &Dictionary::EMPTY);
            let mut lx = PackLexer::new();
            for chunk in text.chunks(step) {
                lx.push(&mut e, chunk).unwrap();
            }
            lx.finish(&mut e).unwrap();
            let m = e.finish().unwrap();
            assert_eq!(&buf[..m], &whole[..n]);
        }
    }
}
