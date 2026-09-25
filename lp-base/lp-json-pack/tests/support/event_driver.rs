//! Drives the event API from JSON text, the way a serializer's tokens would:
//! integers as integers, floats as their printed text, and (when asked) the
//! base64 strings under blob keys as raw bytes. A test stand-in for P3's
//! token adapter; it trusts its input to be the compact JSON of the sample.

use lp_json_pack::{PackEncoder, PackError};

/// Keys whose string values are base64 blobs on the wire.
pub const BLOB_KEYS: &[&str] = &["data", "bytes"];

/// Emit `json` as events. With `blobs`, canonical base64 under [`BLOB_KEYS`]
/// becomes [`PackEncoder::blob`]. Returns how many blobs it emitted.
pub fn drive(enc: &mut PackEncoder<'_>, json: &[u8], blobs: bool) -> Result<usize, PackError> {
    let mut d = Driver {
        s: json,
        i: 0,
        blobs,
        blob_count: 0,
    };
    d.value(enc, false)?;
    assert_eq!(d.i, json.len(), "trailing text");
    Ok(d.blob_count)
}

struct Driver<'s> {
    s: &'s [u8],
    i: usize,
    blobs: bool,
    blob_count: usize,
}

impl Driver<'_> {
    fn value(&mut self, enc: &mut PackEncoder<'_>, under_blob_key: bool) -> Result<(), PackError> {
        match self.s[self.i] {
            b'{' => {
                self.i += 1;
                enc.begin_map()?;
                if self.s[self.i] == b'}' {
                    self.i += 1;
                    return enc.end_map();
                }
                loop {
                    let k = self.string();
                    enc.key(&k)?;
                    assert_eq!(self.s[self.i], b':');
                    self.i += 1;
                    let blob_key = BLOB_KEYS.contains(&k.as_str());
                    self.value(enc, blob_key)?;
                    match self.s[self.i] {
                        b',' => self.i += 1,
                        b'}' => {
                            self.i += 1;
                            return enc.end_map();
                        }
                        c => panic!("unexpected {}", c as char),
                    }
                }
            }
            b'[' => {
                self.i += 1;
                enc.begin_seq()?;
                if self.s[self.i] == b']' {
                    self.i += 1;
                    return enc.end_seq();
                }
                loop {
                    self.value(enc, false)?;
                    match self.s[self.i] {
                        b',' => self.i += 1,
                        b']' => {
                            self.i += 1;
                            return enc.end_seq();
                        }
                        c => panic!("unexpected {}", c as char),
                    }
                }
            }
            b'"' => {
                let v = self.string();
                if self.blobs && under_blob_key {
                    if let Some(raw) = base64_decode_canonical(v.as_bytes()) {
                        self.blob_count += 1;
                        return enc.blob(&raw);
                    }
                }
                enc.str(&v)
            }
            b't' => {
                self.i += 4;
                enc.bool(true)
            }
            b'f' => {
                self.i += 5;
                enc.bool(false)
            }
            b'n' => {
                self.i += 4;
                enc.null()
            }
            _ => {
                let start = self.i;
                while self.i < self.s.len()
                    && matches!(
                        self.s[self.i],
                        b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-'
                    )
                {
                    self.i += 1;
                }
                let text = std::str::from_utf8(&self.s[start..self.i]).unwrap();
                if let Ok(v) = text.parse::<u64>() {
                    enc.u64(v)
                } else if let Some(v) = text.parse::<i64>().ok().filter(|_| text != "-0") {
                    enc.i64(v)
                } else {
                    enc.decimal_text(text)
                }
            }
        }
    }

    fn string(&mut self) -> String {
        assert_eq!(self.s[self.i], b'"');
        self.i += 1;
        let mut out = Vec::new();
        loop {
            let c = self.s[self.i];
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).unwrap(),
                b'\\' => {
                    let e = self.s[self.i];
                    self.i += 1;
                    out.push(match e {
                        b'"' => b'"',
                        b'\\' => b'\\',
                        b'/' => b'/',
                        b'b' => 0x08,
                        b'f' => 0x0C,
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'u' => {
                            let hex = std::str::from_utf8(&self.s[self.i..self.i + 4]).unwrap();
                            self.i += 4;
                            let cp = u32::from_str_radix(hex, 16).unwrap();
                            let ch = char::from_u32(cp).unwrap();
                            let mut b = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
                            continue;
                        }
                        _ => panic!("bad escape"),
                    });
                }
                _ => out.push(c),
            }
        }
    }
}

/// Decode padded standard base64, only when re-encoding reproduces it.
pub fn base64_decode_canonical(text: &[u8]) -> Option<Vec<u8>> {
    if text.is_empty() || text.len() % 4 != 0 {
        return None;
    }
    let sextet = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let pad = text.iter().rev().take_while(|&&c| c == b'=').count();
    if pad > 2 {
        return None;
    }
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for &c in &text[..text.len() - pad] {
        acc = (acc << 6) | sextet(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    // Leftover bits must be zero, or re-encoding would differ.
    if acc & ((1 << bits) - 1) != 0 {
        return None;
    }
    Some(out)
}
