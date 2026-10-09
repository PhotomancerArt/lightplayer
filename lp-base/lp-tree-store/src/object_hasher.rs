//! The hasher seam: every record id is SHA-256, computed by
//! whoever mounts the store. The firmware passes the C6's hardware SHA; host
//! and tests pass [`SoftSha256`] (feature `soft-sha`, the `sha2` crate).

/// SHA-256 over the concatenation of `parts`.
pub trait ObjectHasher {
    fn sha256(&mut self, parts: &[&[u8]]) -> [u8; 32];
}

impl<H: ObjectHasher + ?Sized> ObjectHasher for &mut H {
    fn sha256(&mut self, parts: &[&[u8]]) -> [u8; 32] {
        (**self).sha256(parts)
    }
}

/// Software SHA-256 (`sha2`).
#[cfg(feature = "soft-sha")]
#[derive(Clone, Copy, Debug, Default)]
pub struct SoftSha256;

#[cfg(feature = "soft-sha")]
impl ObjectHasher for SoftSha256 {
    fn sha256(&mut self, parts: &[&[u8]]) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for p in parts {
            h.update(p);
        }
        h.finalize().into()
    }
}
