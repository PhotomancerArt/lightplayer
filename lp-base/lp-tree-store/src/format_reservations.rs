//! Numbers the format has allocated that version 3 does not write
//! (FORMAT.md "Reserved: the index checkpoint"). They are here so that the
//! code that builds the feature, and the image inspector, name them from
//! one place, and so a test can pin that a version-3 reader skips them.
//! A number, once allocated, is never handed out for anything else.

/// Record kind of an index chunk: sorted 14-byte entries
/// (`id u64 ‖ sector u16 ‖ offset u16 ‖ len u16`).
pub const INDEX_CHUNK_KIND: u8 = 6;
/// Record kind of an index fence (a run's search tree node) or the top.
pub const INDEX_FENCE_KIND: u8 = 7;
/// Root tail tag naming the index top: `top id u64 ‖ sector u16 ‖ offset u16`.
pub const INDEX_TOP_ROOT_TAG: u8 = 1;
/// Id tag of an index chunk (`id(6, payload)`).
pub const INDEX_CHUNK_ID_TAG: u8 = 6;
/// Id tag of an index fence or top (`id(7, payload)`).
pub const INDEX_FENCE_ID_TAG: u8 = 7;
/// Bytes of one index chunk entry.
pub const INDEX_ENTRY_LEN: usize = 14;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record_kind::RecordKind;

    /// Version 3 gives none of them a meaning: the kinds are unknown (so
    /// skipped as garbage), and no id tag it writes collides.
    #[test]
    fn version_3_defines_none_of_them() {
        assert_eq!(RecordKind::from_u8(INDEX_CHUNK_KIND), None);
        assert_eq!(RecordKind::from_u8(INDEX_FENCE_KIND), None);
        for k in [
            RecordKind::Blob,
            RecordKind::Multi,
            RecordKind::Dir,
            RecordKind::Root,
        ] {
            assert!(k.to_u8() < INDEX_CHUNK_KIND);
        }
        use crate::object_id::IdTag;
        for t in [IdTag::Blob, IdTag::Dir, IdTag::Multi, IdTag::Root] {
            assert!((t as u8) < INDEX_CHUNK_ID_TAG);
        }
    }
}
