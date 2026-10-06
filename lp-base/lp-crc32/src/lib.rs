//! **lp-crc32**: the one CRC-32 of the boot and update records.
//!
//! CRC-32/ISO-HDLC — the "IEEE" CRC of zlib, PNG and Ethernet: reflected,
//! polynomial `0xEDB8_8320`, initial value and final XOR `0xFFFF_FFFF`.
//! Check value: `crc32(b"123456789") == 0xCBF4_3926`.
//!
//! **Who uses it, and why it is one crate.** Every record of the split image
//! and the update protocol that hashes a build or checks itself uses this
//! CRC, and two of them are forever formats:
//!
//! - the boot record's build hash and checksum (`lp-bootctl`, the split
//!   image; it moves onto this crate after the split image merges);
//! - the update-progress record's build hash and checksum (`lpc-update`'s
//!   `transfer_record`);
//! - the board manifest's `refusedBuild`, which a host recomputes from a
//!   build id to know which build a board refused (`lpc-update`'s
//!   `build_hash`, `lpa-update`).
//!
//! A board and every future host must agree on these values bit for bit, so
//! the CRC is defined once, here, rather than copied beside each record
//! (`one-way-doors.md` §7 and §10).
//!
//! **Table-free and bitwise** on purpose: it runs over a few dozen bytes at a
//! time (a build id, a 52-byte record header), and on the ESP32-C6 flash is
//! the binding constraint — a 1 KiB lookup table would cost more than eight
//! shifts per byte ever will.
//!
//! `no_std`, no `alloc`, no dependencies.

#![no_std]

/// The reflected IEEE polynomial.
const POLY: u32 = 0xEDB8_8320;

/// A CRC-32 in progress: [`new`](Self::new), any number of
/// [`update`](Self::update)s, then [`finish`](Self::finish).
#[derive(Clone, Copy, Debug)]
pub struct Crc32(u32);

impl Crc32 {
    /// A fresh CRC (the `0xFFFF_FFFF` preset).
    #[must_use]
    pub const fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    /// Fold `bytes` in.
    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u32::from(byte);
            for _ in 0..8 {
                self.0 = if self.0 & 1 != 0 {
                    (self.0 >> 1) ^ POLY
                } else {
                    self.0 >> 1
                };
            }
        }
    }

    /// The CRC of everything folded in (the final XOR applied).
    #[must_use]
    pub const fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

/// The CRC-32 of one byte slice.
#[must_use]
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(bytes);
    crc.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CRC-32/ISO-HDLC's catalogued check value. The same vector
    /// `lp-bootctl`'s and `lp-recovery`'s private copies test on `main`
    /// (`crc32::tests::matches_the_known_check_vector`,
    /// `recovery_region::tests::crc32_matches_known_vector`).
    #[test]
    fn matches_the_known_check_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    /// `lp-bootctl`'s `empty_input_is_zero`.
    #[test]
    fn empty_input_is_zero() {
        assert_eq!(crc32(&[]), 0);
    }

    /// `lp-bootctl`'s `single_bit_changes_the_result`.
    #[test]
    fn single_bit_changes_the_result() {
        assert_ne!(crc32(&[0x00]), crc32(&[0x01]));
    }

    /// Values any zlib computes (`zlib.crc32` in Python, `crc32fast`), so a
    /// host in another language can check itself against this crate.
    #[test]
    fn matches_zlib_on_more_inputs() {
        assert_eq!(crc32(&[0x00]), 0xD202_EF8D);
        assert_eq!(crc32(b"a"), 0xE8B7_BE43);
        assert_eq!(crc32(b"abc"), 0x3524_41C2);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
        assert_eq!(crc32(&[0xFF; 4]), 0xFFFF_FFFF);
    }

    #[test]
    fn streaming_in_any_split_equals_one_shot() {
        let data: [u8; 97] = core::array::from_fn(|i| (i as u8).wrapping_mul(31) ^ 0x5a);
        let whole = crc32(&data);
        for cut in 0..data.len() {
            let mut crc = Crc32::new();
            crc.update(&data[..cut]);
            crc.update(&data[cut..]);
            assert_eq!(crc.finish(), whole, "split at {cut}");
        }
    }
}
