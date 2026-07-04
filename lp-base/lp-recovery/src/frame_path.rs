//! Identity of a stack (prefix) of recovery frames, for blame bookkeeping.

use crate::frame_kind::FrameKind;
use crate::frame_record::{FNV1A_64_INIT, fnv1a_64_step};

/// Maximum recovery-stack depth. Deep enough for
/// boot → project → node → shader-compile with headroom.
pub const MAX_FRAME_DEPTH: usize = 8;

/// A sequence of `(kind, name_hash)` pairs identifying a stack prefix.
///
/// This is the *identity* side of the frame stack: names are reduced to
/// their full-name FNV-1a hashes. Display names live in [`FrameRecord`]s
/// (live stack) and crash-record name snapshots.
///
/// [`FrameRecord`]: crate::FrameRecord
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct FramePath {
    len: u8,
    _pad: [u8; 3],
    kinds: [u8; MAX_FRAME_DEPTH],
    hashes: [u32; MAX_FRAME_DEPTH],
}

impl FramePath {
    pub const EMPTY: Self = Self {
        len: 0,
        _pad: [0; 3],
        kinds: [0; MAX_FRAME_DEPTH],
        hashes: [0; MAX_FRAME_DEPTH],
    };

    pub fn len(&self) -> usize {
        (self.len as usize).min(MAX_FRAME_DEPTH)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Append an entry. Returns `false` (unchanged) when full.
    pub fn push(&mut self, kind: FrameKind, name_hash: u32) -> bool {
        let len = self.len();
        if len >= MAX_FRAME_DEPTH {
            return false;
        }
        self.kinds[len] = kind as u8;
        self.hashes[len] = name_hash;
        self.len = (len + 1) as u8;
        true
    }

    /// Entry at `index`, if in range: `(raw kind, name_hash)`.
    pub fn entry(&self, index: usize) -> Option<(u8, u32)> {
        (index < self.len()).then(|| (self.kinds[index], self.hashes[index]))
    }

    /// The first `depth` entries as their own path.
    pub fn prefix(&self, depth: usize) -> Self {
        let mut out = *self;
        out.len = depth.min(self.len()) as u8;
        for i in out.len()..MAX_FRAME_DEPTH {
            out.kinds[i] = 0;
            out.hashes[i] = 0;
        }
        out
    }

    /// Order-sensitive hash of the first `depth` entries.
    ///
    /// This is the key blame entries are stored under; equal prefixes hash
    /// equal regardless of what lies deeper.
    pub fn prefix_hash(&self, depth: usize) -> u64 {
        let depth = depth.min(self.len());
        let mut hash = FNV1A_64_INIT;
        for i in 0..depth {
            hash = fnv1a_64_step(hash, &[self.kinds[i]]);
            hash = fnv1a_64_step(hash, &self.hashes[i].to_le_bytes());
        }
        hash
    }

    /// Hash of the whole path.
    pub fn full_hash(&self) -> u64 {
        self.prefix_hash(self.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_abc() -> FramePath {
        let mut p = FramePath::EMPTY;
        assert!(p.push(FrameKind::Boot, 0xa));
        assert!(p.push(FrameKind::ProjectLoad, 0xb));
        assert!(p.push(FrameKind::NodeRender, 0xc));
        p
    }

    #[test]
    fn push_and_entry_round_trip() {
        let p = path_abc();
        assert_eq!(p.len(), 3);
        assert_eq!(p.entry(0), Some((FrameKind::Boot as u8, 0xa)));
        assert_eq!(p.entry(2), Some((FrameKind::NodeRender as u8, 0xc)));
        assert_eq!(p.entry(3), None);
    }

    #[test]
    fn push_saturates_at_max_depth() {
        let mut p = FramePath::EMPTY;
        for i in 0..MAX_FRAME_DEPTH {
            assert!(p.push(FrameKind::NodeRender, i as u32));
        }
        assert!(!p.push(FrameKind::NodeRender, 999));
        assert_eq!(p.len(), MAX_FRAME_DEPTH);
    }

    #[test]
    fn equal_prefixes_hash_equal_regardless_of_suffix() {
        let p = path_abc();
        let mut q = path_abc();
        q.push(FrameKind::ShaderCompile, 0xd);
        assert_eq!(p.prefix_hash(2), q.prefix_hash(2));
        assert_ne!(p.full_hash(), q.full_hash());
        assert_eq!(p.prefix(2), q.prefix(2));
    }

    #[test]
    fn hash_is_order_and_kind_sensitive() {
        let mut p = FramePath::EMPTY;
        p.push(FrameKind::Boot, 0xa);
        p.push(FrameKind::ProjectLoad, 0xb);
        let mut q = FramePath::EMPTY;
        q.push(FrameKind::ProjectLoad, 0xb);
        q.push(FrameKind::Boot, 0xa);
        assert_ne!(p.full_hash(), q.full_hash());

        let mut r = FramePath::EMPTY;
        r.push(FrameKind::Boot, 0xa);
        r.push(FrameKind::NodeRender, 0xb); // same hash, different kind
        assert_ne!(p.full_hash(), r.full_hash());
    }

    #[test]
    fn empty_paths_hash_equal() {
        assert_eq!(
            FramePath::EMPTY.full_hash(),
            FramePath::EMPTY.prefix_hash(0)
        );
    }
}
