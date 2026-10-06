//! The engine digest slot: the core's record of which engine is its own.
//!
//! A 40-byte record the **core** carries in its rodata:
//!
//! ```text
//!  0  magic    u32       ENGINE_DIGEST_MAGIC ("LPED")
//!  4  version  u16       ENGINE_DIGEST_VERSION (1)
//!  6  algo     u16       ENGINE_DIGEST_SHA256 (1)
//!  8  digest   [u8; 32]  SHA-256 of engine.bin exactly as flashed
//! ```
//!
//! The firmware links it unpatched ([`ENGINE_DIGEST_UNPATCHED`]: the digest
//! all zeros). The packager computes SHA-256 of `engine.bin` **exactly as
//! flashed** — the engine header patched (length, CRC) and committed — and
//! patches it into the core's ELF before the core's image is made, so the
//! image's checksum and appended hash cover it ([`patch`]).
//!
//! The core does not verify its engine against it at boot; it names its
//! first bytes in its boot line. An over-the-air update verifies a
//! downloaded engine against it, and a release publishes the same value.
//! This crate only defines the slot: the hash is computed by the packager.

/// `"LPED"`, little-endian.
pub const ENGINE_DIGEST_MAGIC: u32 = u32::from_le_bytes(*b"LPED");
pub const ENGINE_DIGEST_VERSION: u16 = 1;
/// The algorithm code for SHA-256.
pub const ENGINE_DIGEST_SHA256: u16 = 1;
/// Bytes in the slot.
pub const ENGINE_DIGEST_LEN: usize = 40;
/// Where the digest starts in the slot.
pub const ENGINE_DIGEST_OFFSET: usize = 8;

/// The slot as the firmware links it: magic, version, algorithm, zeros.
pub const ENGINE_DIGEST_UNPATCHED: [u8; ENGINE_DIGEST_LEN] = encode(&[0; 32]);

/// The slot holding `digest`.
pub const fn encode(digest: &[u8; 32]) -> [u8; ENGINE_DIGEST_LEN] {
    let mut out = [0u8; ENGINE_DIGEST_LEN];
    let magic = ENGINE_DIGEST_MAGIC.to_le_bytes();
    let version = ENGINE_DIGEST_VERSION.to_le_bytes();
    let algo = ENGINE_DIGEST_SHA256.to_le_bytes();
    let mut i = 0;
    while i < 4 {
        out[i] = magic[i];
        i += 1;
    }
    out[4] = version[0];
    out[5] = version[1];
    out[6] = algo[0];
    out[7] = algo[1];
    let mut i = 0;
    while i < 32 {
        out[ENGINE_DIGEST_OFFSET + i] = digest[i];
        i += 1;
    }
    out
}

/// The digest in a slot, or `None` when the bytes are not a v1 SHA-256
/// slot.
pub fn decode(slot: &[u8]) -> Option<[u8; 32]> {
    let s = slot.get(..ENGINE_DIGEST_LEN)?;
    if s[..ENGINE_DIGEST_OFFSET] != ENGINE_DIGEST_UNPATCHED[..ENGINE_DIGEST_OFFSET] {
        return None;
    }
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&s[ENGINE_DIGEST_OFFSET..]);
    Some(digest)
}

/// Why a slot cannot be patched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestPatchError {
    /// The bytes are not the unpatched slot.
    NotUnpatched,
}

/// The packager's step: write `digest` into an unpatched slot.
pub fn patch(slot: &mut [u8], digest: &[u8; 32]) -> Result<(), DigestPatchError> {
    let s = slot
        .get_mut(..ENGINE_DIGEST_LEN)
        .ok_or(DigestPatchError::NotUnpatched)?;
    if *s != ENGINE_DIGEST_UNPATCHED {
        return Err(DigestPatchError::NotUnpatched);
    }
    s.copy_from_slice(&encode(digest));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unpatched_slot_names_itself() {
        assert_eq!(&ENGINE_DIGEST_UNPATCHED[..4], b"LPED");
        assert_eq!(decode(&ENGINE_DIGEST_UNPATCHED), Some([0; 32]));
    }

    #[test]
    fn a_patch_round_trips_once() {
        let mut slot = ENGINE_DIGEST_UNPATCHED;
        let digest = [0xab; 32];
        patch(&mut slot, &digest).unwrap();
        assert_eq!(decode(&slot), Some(digest));
        assert_eq!(
            patch(&mut slot, &digest),
            Err(DigestPatchError::NotUnpatched)
        );
    }

    #[test]
    fn foreign_bytes_are_no_slot() {
        assert_eq!(decode(&[0xff; ENGINE_DIGEST_LEN]), None);
        assert_eq!(decode(&[0; 8]), None);
    }
}
