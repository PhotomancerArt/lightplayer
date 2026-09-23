//! The decoder: LPBJ → the byte-identical JSON text `ser_write_json` wrote.
//!
//! Output escaping mirrors `ser_write_json`'s `format_escaped_str_contents`
//! exactly (`"` `\` and C0 controls; `\b \t \n \f \r`, other controls as
//! upper-case `\u00XX`), which is what makes the round trip byte-exact.

use crate::format_tags as tag;
use crate::{MAX_BACKREFS, base64_blob, decimal_text, varint, wire_dictionary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    Truncated,
    BadTag(u8),
    BadIndex,
    TooDeep,
}

/// Decode one LPBJ frame, emitting JSON text through `emit`. Returns the number
/// of input bytes consumed.
pub fn decode_to_json(input: &[u8], emit: &mut dyn FnMut(&[u8])) -> Result<usize, DecodeError> {
    let mut d = Decoder { input, pos: 0, backrefs: [(0, 0); MAX_BACKREFS], backref_count: 0, emit };
    d.value(0)?;
    Ok(d.pos)
}

struct Decoder<'a, 'e> {
    input: &'a [u8],
    pos: usize,
    backrefs: [(u32, u16); MAX_BACKREFS],
    backref_count: usize,
    emit: &'e mut dyn FnMut(&[u8]),
}

impl<'a> Decoder<'a, '_> {
    fn byte(&mut self) -> Result<u8, DecodeError> {
        let b = *self.input.get(self.pos).ok_or(DecodeError::Truncated)?;
        self.pos += 1;
        Ok(b)
    }

    fn uvar(&mut self) -> Result<u64, DecodeError> {
        varint::read(self.input, &mut self.pos).ok_or(DecodeError::Truncated)
    }

    fn take(&mut self, n: usize) -> Result<(usize, usize), DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated)?;
        if end > self.input.len() {
            return Err(DecodeError::Truncated);
        }
        let at = self.pos;
        self.pos = end;
        Ok((at, n))
    }

    fn remember(&mut self, at: usize, len: usize) {
        if len >= 2 && self.backref_count < MAX_BACKREFS {
            self.backrefs[self.backref_count] = (at as u32, len as u16);
            self.backref_count += 1;
        }
    }

    fn backref(&mut self) -> Result<&'a [u8], DecodeError> {
        let n = self.uvar()? as usize;
        let &(a, l) = self.backrefs[..self.backref_count].get(n).ok_or(DecodeError::BadIndex)?;
        Ok(&self.input[a as usize..a as usize + l as usize])
    }

    fn inline(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let (at, n) = self.take(n)?;
        self.remember(at, n);
        Ok(&self.input[at..at + n])
    }

    fn value(&mut self, depth: u32) -> Result<(), DecodeError> {
        if depth > 64 {
            return Err(DecodeError::TooDeep);
        }
        let t = self.byte()?;
        match t {
            0..=tag::UINT_INLINE_MAX => self.uint(false, u64::from(t)),
            0x40..=0x7F => {
                let s = wire_dictionary::value(usize::from(t - tag::VDICT_INLINE_BASE))
                    .ok_or(DecodeError::BadIndex)?;
                self.string(s);
            }
            0x80..=0x9F => {
                let s = self.inline(usize::from(t - tag::STR_INLINE_BASE))?;
                self.string(s);
            }
            tag::OBJECT => {
                (self.emit)(b"{");
                let mut first = true;
                loop {
                    let k = self.byte()?;
                    if k == tag::OBJECT_END {
                        break;
                    }
                    if !first {
                        (self.emit)(b",");
                    }
                    first = false;
                    let key = match k {
                        0..=0xEF => wire_dictionary::key(usize::from(k)).ok_or(DecodeError::BadIndex)?,
                        tag::KDICT_WIDE_BASE..=tag::KDICT_WIDE_MAX => {
                            let lo = self.byte()?;
                            let i = tag::KDICT_INLINE_COUNT
                                + ((usize::from(k - tag::KDICT_WIDE_BASE) << 8) | usize::from(lo));
                            wire_dictionary::key(i).ok_or(DecodeError::BadIndex)?
                        }
                        tag::KEY_INLINE => {
                            let n = self.uvar()? as usize;
                            self.inline(n)?
                        }
                        tag::KEY_BACKREF => self.backref()?,
                        _ => return Err(DecodeError::BadTag(k)),
                    };
                    self.string(key);
                    (self.emit)(b":");
                    self.value(depth + 1)?;
                }
                (self.emit)(b"}");
            }
            tag::ARRAY => {
                (self.emit)(b"[");
                let mut first = true;
                while *self.input.get(self.pos).ok_or(DecodeError::Truncated)? != tag::ARRAY_END {
                    if !first {
                        (self.emit)(b",");
                    }
                    first = false;
                    self.value(depth + 1)?;
                }
                self.pos += 1;
                (self.emit)(b"]");
            }
            tag::NULL => (self.emit)(b"null"),
            tag::FALSE => (self.emit)(b"false"),
            tag::TRUE => (self.emit)(b"true"),
            tag::UINT | tag::NEG_INT => {
                let m = self.uvar()?;
                self.uint(t == tag::NEG_INT, m);
            }
            tag::DECIMAL_POS | tag::DECIMAL_NEG => {
                let exp = varint::unzigzag(self.uvar()?);
                let coeff = self.uvar()?;
                let mut buf = [0u8; 48];
                let exp = i32::try_from(exp).map_err(|_| DecodeError::BadIndex)?;
                let n = decimal_text::layout(t == tag::DECIMAL_NEG, coeff, exp, &mut buf)
                    .ok_or(DecodeError::BadIndex)?;
                (self.emit)(&buf[..n]);
            }
            tag::STRING => {
                let n = self.uvar()? as usize;
                let s = self.inline(n)?;
                self.string(s);
            }
            tag::BLOB => {
                let n = self.uvar()? as usize;
                let (at, n) = self.take(n)?;
                (self.emit)(b"\"");
                base64_blob::encode(&self.input[at..at + n], self.emit);
                (self.emit)(b"\"");
            }
            tag::VDICT => {
                let i = self.uvar()? as usize + tag::VDICT_INLINE_COUNT;
                let s = wire_dictionary::value(i).ok_or(DecodeError::BadIndex)?;
                self.string(s);
            }
            tag::BACKREF => {
                let s = self.backref()?;
                self.string(s);
            }
            tag::NUMBER_TEXT => {
                let n = self.uvar()? as usize;
                let (at, n) = self.take(n)?;
                (self.emit)(&self.input[at..at + n]);
            }
            _ => return Err(DecodeError::BadTag(t)),
        }
        Ok(())
    }

    fn uint(&mut self, neg: bool, v: u64) {
        let mut buf = [0u8; 21];
        let mut i = buf.len();
        let mut v = v;
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
        (self.emit)(&buf[i..]);
    }

    fn string(&mut self, s: &[u8]) {
        (self.emit)(b"\"");
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
                (self.emit)(&s[start..i]);
            }
            if short == b'u' {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                (self.emit)(&[b'\\', b'u', b'0', b'0', HEX[usize::from(b >> 4)], HEX[usize::from(b & 15)]]);
            } else {
                (self.emit)(&[b'\\', short]);
            }
            start = i + 1;
        }
        (self.emit)(&s[start..]);
        (self.emit)(b"\"");
    }
}
