//! The build id field and the build hash.
//!
//! A **build id** is `<version>+<commit[..12]>` (the split image's D7;
//! `one-way-doors.md` §1), at most [`BUILD_ID_LEN`] bytes. On the wire (the
//! offer) and in the engine header it is a 64-byte field, zero-padded.
//!
//! The **build hash** is the 32-bit name the boot record and the progress
//! record give a build: CRC-32 (IEEE, [`lp_crc32`]) of the build id's
//! **text**, without the zero padding. **It must equal the split image's
//! `lp_bootctl::boot_record::build_hash` rule**, which is defined the same
//! way; hosts recompute it to read `refusedBuild`. This is a forever rule.

/// Bytes in the build id field.
pub const BUILD_ID_LEN: usize = 64;

/// The build id's text inside its zero-padded field: everything before the
/// first zero byte.
#[must_use]
pub fn build_id_text(field: &[u8; BUILD_ID_LEN]) -> &[u8] {
    let end = field.iter().position(|&b| b == 0).unwrap_or(BUILD_ID_LEN);
    &field[..end]
}

/// The zero-padded field for a build id's text, or `None` if the text is
/// longer than the field or contains a zero byte.
#[must_use]
pub fn build_id_field(text: &[u8]) -> Option<[u8; BUILD_ID_LEN]> {
    if text.len() > BUILD_ID_LEN || text.contains(&0) {
        return None;
    }
    let mut field = [0u8; BUILD_ID_LEN];
    field[..text.len()].copy_from_slice(text);
    Some(field)
}

/// The build hash of a build id's text: `lp_crc32::crc32(text)`.
#[must_use]
pub fn build_hash(text: &[u8]) -> u32 {
    lp_crc32::crc32(text)
}

/// The build hash of a zero-padded build id field.
#[must_use]
pub fn build_hash_of_field(field: &[u8; BUILD_ID_LEN]) -> u32 {
    build_hash(build_id_text(field))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_of_the_text_without_its_padding() {
        let text = b"2026.10.05-3+abc123456789";
        let field = build_id_field(text).unwrap();
        assert_eq!(build_id_text(&field), text);
        assert_eq!(build_hash_of_field(&field), lp_crc32::crc32(text));
        assert_ne!(build_hash_of_field(&field), lp_crc32::crc32(&field));
    }

    #[test]
    fn a_full_field_has_no_padding_and_a_too_long_text_has_no_field() {
        let full = [b'x'; BUILD_ID_LEN];
        assert_eq!(build_id_field(&full), Some(full));
        assert_eq!(build_id_text(&full), &full[..]);
        assert_eq!(build_id_field(&[b'x'; BUILD_ID_LEN + 1]), None);
        assert_eq!(build_id_field(b"a\0b"), None);
    }
}
