//! One file inside a share envelope.

use serde::{Deserialize, Serialize};

use super::share_error::ShareError;

/// A file's bytes, carried as text when they are valid UTF-8 and as base64
/// otherwise.
///
/// The text arm is the point of the JSON channel: a shared project is
/// mostly `.json` and `.glsl`, and keeping those readable means a pasted
/// envelope can be skimmed, diffed, and hand-edited in a chat window. Only
/// genuinely binary files (an embedded PNG) pay the base64 tax.
///
/// Decoding accepts either arm for any path — a producer that base64s
/// everything is still readable, just less pleasant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShareFile {
    /// UTF-8 file content, verbatim.
    Text { text: String },
    /// Non-UTF-8 file content, standard base64 with padding.
    Base64 { base64: String },
}

impl ShareFile {
    /// Wrap raw bytes, choosing the arm by whether they are valid UTF-8.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        match core::str::from_utf8(bytes) {
            Ok(text) => Self::Text {
                text: text.to_string(),
            },
            Err(_) => Self::Base64 {
                base64: base64_encode(bytes),
            },
        }
    }

    /// Recover the raw bytes.
    pub fn to_bytes(&self, path: &str) -> Result<Vec<u8>, ShareError> {
        match self {
            Self::Text { text } => Ok(text.as_bytes().to_vec()),
            Self::Base64 { base64 } => base64_decode(base64)
                .ok_or_else(|| ShareError::Malformed(format!("{path}: invalid base64"))),
        }
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for byte in text.bytes() {
        // Tolerate the line breaks a clipboard round-trip through a chat
        // client or an editor's hard wrap can introduce.
        if byte.is_ascii_whitespace() || byte == b'=' {
            continue;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_bytes_stay_readable_text() {
        let file = ShareFile::from_bytes(b"void main() {}");
        assert_eq!(
            file,
            ShareFile::Text {
                text: "void main() {}".to_string()
            }
        );
        assert_eq!(file.to_bytes("x.glsl").unwrap(), b"void main() {}");
    }

    #[test]
    fn non_utf8_bytes_fall_back_to_base64() {
        // A PNG header: 0x89 is not valid UTF-8 on its own.
        let bytes = [0x89u8, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        let file = ShareFile::from_bytes(&bytes);
        assert!(matches!(file, ShareFile::Base64 { .. }));
        assert_eq!(file.to_bytes("logo.png").unwrap(), bytes);
    }

    #[test]
    fn base64_round_trips_every_length_remainder() {
        // The three padding cases (0, 1, 2 trailing bytes) are where naive
        // encoders break.
        for len in 0..=16usize {
            let bytes: Vec<u8> = (0..len)
                .map(|index| (index as u8).wrapping_mul(37))
                .collect();
            let encoded = base64_encode(&bytes);
            assert_eq!(
                base64_decode(&encoded).as_deref(),
                Some(bytes.as_slice()),
                "len {len}"
            );
        }
    }

    #[test]
    fn base64_decoding_survives_a_clipboard_hard_wrap() {
        let bytes: Vec<u8> = (0..64u8).collect();
        let encoded = base64_encode(&bytes);
        let wrapped = format!("{}\n{}", &encoded[..20], &encoded[20..]);
        assert_eq!(base64_decode(&wrapped), Some(bytes));
    }

    #[test]
    fn invalid_base64_is_rejected_with_the_path() {
        let file = ShareFile::Base64 {
            base64: "not base64!".to_string(),
        };
        let error = file.to_bytes("logo.png").unwrap_err();
        assert!(error.to_string().contains("logo.png"), "{error}");
    }
}
