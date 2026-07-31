//! Small bitwise CRC-32 (IEEE / ISO-HDLC, reflected).
//!
//! No lookup table: this runs once per boot over 12 bytes. Eight iterations
//! per byte is nothing, and a 1 KB table would cost more flash than it saves
//! on a device where flash headroom is the binding constraint.
//!
//! Deliberately a local copy rather than a dependency: `lp-recovery` keeps
//! its equivalent private, and one shared 20-line helper is not worth a
//! crate edge between two `no_std` primitives that must not depend on each
//! other.

pub(crate) struct Crc32(u32);

impl Crc32 {
    pub(crate) fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u32::from(byte);
            for _ in 0..8 {
                self.0 = if self.0 & 1 != 0 {
                    (self.0 >> 1) ^ 0xEDB8_8320
                } else {
                    self.0 >> 1
                };
            }
        }
    }

    pub(crate) fn finish(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

/// CRC-32 of a single byte slice.
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(bytes);
    crc.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_known_check_vector() {
        // CRC-32/ISO-HDLC of "123456789" is 0xCBF43926.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn empty_input_is_zero() {
        assert_eq!(crc32(&[]), 0);
    }

    #[test]
    fn single_bit_changes_the_result() {
        assert_ne!(crc32(&[0x00]), crc32(&[0x01]));
    }
}
