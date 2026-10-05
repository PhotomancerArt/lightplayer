//! **The two hash rules** (`one-way-doors.md` §13). Forever: the values they
//! define are compared across boards, hosts, releases and caches.
//!
//! - **Engine SHA-256** = SHA-256 of `engine.bin` exactly as flashed (its
//!   header patched and committed). That one value is the core's digest
//!   slot, `ota-manifest.json`'s `engine.sha256`, the board manifest's
//!   `engineSha256`, Studio's engine-cache key and the store's blob key. A
//!   board accepts an engine (any engine install, a heal included) only if
//!   the bytes it wrote hash to its **digest slot** — never to what an offer
//!   claims.
//! - **Core SHA-256** = SHA-256 of `core.bin`, which equals the flash bytes
//!   `[core_off, core_off + core_len)`; the boot record's `core_len` is
//!   `core.bin`'s exact length. No slot can hold the hash of the image it sits
//!   in, so the core computes it at runtime (DM24, once, cached) and reports
//!   it as `coreSha256`; it equals `ota-manifest.json`'s `core.sha256`.

use sha2::{Digest, Sha256};

/// The engine rule: SHA-256 of `engine.bin` as flashed.
#[must_use]
pub fn engine_sha256(engine_bin: &[u8]) -> [u8; 32] {
    Sha256::digest(engine_bin).into()
}

/// The core rule: SHA-256 of `core.bin` (= the flash bytes
/// `[core_off, core_off + core_len)`).
#[must_use]
pub fn core_sha256(core_bin: &[u8]) -> [u8; 32] {
    Sha256::digest(core_bin).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_rules_are_plain_sha256() {
        // FIPS 180-2's "abc" vector.
        let abc = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ];
        assert_eq!(engine_sha256(b"abc"), abc);
        assert_eq!(core_sha256(b"abc"), abc);
    }
}
