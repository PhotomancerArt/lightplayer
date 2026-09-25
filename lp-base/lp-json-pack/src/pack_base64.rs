//! Canonical base64 for blobs: standard alphabet, `=`-padded.
//!
//! A blob is carried raw in a frame and decoded back to the text every JSON
//! serializer on the wire prints for it (the `base64` crate's `STANDARD`
//! engine). The encoder never *parses* base64: blobs arrive as bytes because
//! the wire type says so, not because a string looked like base64.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 length of `n` raw bytes.
pub fn base64_len(n: usize) -> usize {
    n.div_ceil(3) * 4
}

/// Encode `raw` as padded base64, handing out chunks of at most 64 bytes.
pub fn encode_base64<E>(raw: &[u8], mut emit: impl FnMut(&[u8]) -> Result<(), E>) -> Result<(), E> {
    let mut buf = [0u8; 64];
    let mut n = 0;
    for chunk in raw.chunks(3) {
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let v = (u32::from(chunk[0]) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        let q = &mut buf[n..n + 4];
        q[0] = ALPHABET[(v >> 18) as usize & 63];
        q[1] = ALPHABET[(v >> 12) as usize & 63];
        q[2] = if chunk.len() > 1 {
            ALPHABET[(v >> 6) as usize & 63]
        } else {
            b'='
        };
        q[3] = if chunk.len() > 2 {
            ALPHABET[v as usize & 63]
        } else {
            b'='
        };
        n += 4;
        if n == buf.len() {
            emit(&buf)?;
            n = 0;
        }
    }
    if n > 0 {
        emit(&buf[..n])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(raw: &[u8]) -> ([u8; 256], usize) {
        let mut out = [0u8; 256];
        let mut n = 0;
        encode_base64::<()>(raw, |s| {
            out[n..n + s.len()].copy_from_slice(s);
            n += s.len();
            Ok(())
        })
        .unwrap();
        (out, n)
    }

    #[test]
    fn rfc4648_vectors() {
        for (raw, want) in [
            (&b""[..], ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ] {
            let (out, n) = encode(raw);
            assert_eq!(&out[..n], want.as_bytes());
            assert_eq!(n, base64_len(raw.len()));
        }
    }

    #[test]
    fn long_input_crosses_chunks() {
        let raw: [u8; 100] = core::array::from_fn(|i| (i * 37) as u8);
        let (_, n) = encode(&raw);
        assert_eq!(n, base64_len(100));
    }
}
