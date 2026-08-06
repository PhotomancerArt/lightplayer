//! What to call the bytes coming off the content plane.
//!
//! The service is content-opaque (D3): it does not know what a blob *is*,
//! and it stores no MIME type. But an `og:image` that a link unfurler
//! refuses to render is a broken share card, so the one thing worth
//! recognizing is recognized — from the bytes themselves, by magic number,
//! not from anything a client claimed.

/// The fallback: bytes of unknown kind.
pub const OCTET_STREAM: &str = "application/octet-stream";

/// A content type for stored bytes, sniffed from their leading magic number.
pub fn sniff(bytes: &[u8]) -> &'static str {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    const JPEG: &[u8] = b"\xff\xd8\xff";
    const GIF: &[u8] = b"GIF8";
    const WEBP_RIFF: &[u8] = b"RIFF";

    if bytes.starts_with(PNG) {
        "image/png"
    } else if bytes.starts_with(JPEG) {
        "image/jpeg"
    } else if bytes.starts_with(GIF) {
        "image/gif"
    } else if bytes.starts_with(WEBP_RIFF) && bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        OCTET_STREAM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case that matters: a preview PNG served as `og:image`.
    #[test]
    fn a_png_is_recognized() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\nrest"), "image/png");
    }

    #[test]
    fn anything_unrecognized_is_opaque_bytes() {
        assert_eq!(sniff(b"{\"project\": true}"), OCTET_STREAM);
        assert_eq!(sniff(b""), OCTET_STREAM);
        assert_eq!(sniff(b"RIFF1234"), OCTET_STREAM);
    }
}
