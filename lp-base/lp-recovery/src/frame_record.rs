//! One fixed-size frame slot in the persistent stack.

use crate::frame_kind::FrameKind;

/// Bytes of frame name stored inline (display only; identity is the hash).
pub const FRAME_NAME_CAP: usize = 24;

/// A recovery frame as stored in the persistent region: fixed 32 bytes,
/// plain integers only, safe to reinterpret across reboots.
///
/// The truncated `name` is display-only; blame identity is `name_hash`,
/// the FNV-1a hash of the **full, untruncated** name.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct FrameRecord {
    kind: u8,
    name_len: u8,
    _pad: [u8; 2],
    name_hash: u32,
    name: [u8; FRAME_NAME_CAP],
}

impl FrameRecord {
    pub const EMPTY: Self = Self {
        kind: 0,
        name_len: 0,
        _pad: [0; 2],
        name_hash: 0,
        name: [0; FRAME_NAME_CAP],
    };

    /// Fill this slot from a kind + full name. Never fails: the name is
    /// truncated at a char boundary and non-UTF-8-safe reads are prevented
    /// at write time so [`Self::name`] is infallible.
    pub fn set(&mut self, kind: FrameKind, full_name: &str) {
        let hash = fnv1a_32(full_name.as_bytes());
        let end = truncation_boundary(full_name, FRAME_NAME_CAP);
        self.kind = kind as u8;
        self.name_len = end as u8;
        self.name_hash = hash;
        self.name = [0; FRAME_NAME_CAP];
        self.name[..end].copy_from_slice(&full_name.as_bytes()[..end]);
    }

    pub fn clear(&mut self) {
        *self = Self::EMPTY;
    }

    pub fn is_empty(&self) -> bool {
        self.kind == 0
    }

    pub fn kind(&self) -> Option<FrameKind> {
        FrameKind::from_u8(self.kind)
    }

    pub fn kind_raw(&self) -> u8 {
        self.kind
    }

    pub fn name_hash(&self) -> u32 {
        self.name_hash
    }

    /// The stored (possibly truncated) name. Falls back to `""` if the
    /// region bytes were corrupted into invalid UTF-8 or a bad length.
    pub fn name(&self) -> &str {
        let len = (self.name_len as usize).min(FRAME_NAME_CAP);
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }
}

/// Largest `end <= cap` that is a char boundary of `s`.
pub(crate) fn truncation_boundary(s: &str, cap: usize) -> usize {
    let mut end = s.len().min(cap);
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// FNV-1a, 32-bit. Stable across builds; used for frame-name identity.
pub(crate) fn fnv1a_32(bytes: &[u8]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for &b in bytes {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// FNV-1a, 64-bit. Used for hashing frame paths (sequences of frames).
pub(crate) fn fnv1a_64_step(hash: u64, bytes: &[u8]) -> u64 {
    let mut hash = hash;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(crate) const FNV1A_64_INIT: u64 = 0xcbf2_9ce4_8422_2325;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_is_32_bytes() {
        assert_eq!(core::mem::size_of::<FrameRecord>(), 32);
    }

    #[test]
    fn set_stores_kind_name_and_hash() {
        let mut rec = FrameRecord::EMPTY;
        rec.set(FrameKind::NodeRender, "nodes/fire");
        assert_eq!(rec.kind(), Some(FrameKind::NodeRender));
        assert_eq!(rec.name(), "nodes/fire");
        assert_eq!(rec.name_hash(), fnv1a_32(b"nodes/fire"));
        assert!(!rec.is_empty());
    }

    #[test]
    fn long_name_truncates_but_hash_covers_full_name() {
        let long = "a-very-long-shader-name-that-exceeds-the-inline-capacity.glsl";
        let mut rec = FrameRecord::EMPTY;
        rec.set(FrameKind::ShaderCompile, long);
        assert_eq!(rec.name(), &long[..FRAME_NAME_CAP]);
        // Identity is the full-name hash, so two names sharing a 24-byte
        // prefix still get distinct identities.
        assert_eq!(rec.name_hash(), fnv1a_32(long.as_bytes()));
        assert_ne!(rec.name_hash(), fnv1a_32(long[..FRAME_NAME_CAP].as_bytes()));
    }

    #[test]
    fn multibyte_name_truncates_at_char_boundary() {
        // Each 'é' is 2 bytes; 13 of them = 26 bytes > 24 cap.
        let name = "ééééééééééééé";
        let mut rec = FrameRecord::EMPTY;
        rec.set(FrameKind::ProjectLoad, name);
        // 24 is mid-char, so we must land on 24 - 1... = 24 rounded down to 24? 12 chars = 24 bytes exactly.
        assert_eq!(rec.name(), "éééééééééééé");
    }

    #[test]
    fn corrupted_length_is_clamped_and_does_not_panic() {
        let mut rec = FrameRecord::EMPTY;
        rec.set(FrameKind::Boot, "boot");
        rec.name_len = 250; // simulate corruption
        assert_eq!(rec.name(), "boot\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
    }
}
