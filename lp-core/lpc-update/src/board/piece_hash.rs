//! Hashing a piece before it commits (DM11): every piece is verified by
//! SHA-256 — the core against the offer's `core_sha256`, the engine against
//! the core's **digest slot** — before anything makes it bootable.

use sha2::{Digest, Sha256};

use crate::code_table::CHUNK;

use super::update_target::{FlashFault, UpdateTarget};

/// SHA-256 of `head` (if any, held in RAM) followed by the flash bytes
/// `[from, to)`: the target's own when it has one
/// ([`UpdateTarget::sha256_flash`]), else read through `buf` a sector at a
/// time and hashed here.
pub(crate) fn hash_flash<T: UpdateTarget>(
    target: &mut T,
    head: Option<&[u8]>,
    from: u32,
    to: u32,
    buf: &mut [u8],
) -> Result<[u8; 32], FlashFault> {
    if let Some(hashed) = target.sha256_flash(head, from, to) {
        return hashed;
    }
    let mut sha = Sha256::new();
    if let Some(head) = head {
        sha.update(head);
    }
    let mut at = from;
    while at < to {
        let n = (to - at).min(CHUNK).min(buf.len() as u32) as usize;
        target.read(at, &mut buf[..n])?;
        sha.update(&buf[..n]);
        at += n as u32;
    }
    Ok(sha.finalize().into())
}
