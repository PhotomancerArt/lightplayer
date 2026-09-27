//! Frame checksums: CRC-16/CCITT-FALSE and CRC-32C (Castagnoli), both with a
//! 16-entry ("nibble") table: 32 or 64 bytes of flash, two table lookups per
//! byte. Written from the polynomial definitions; the check values are the
//! standard `"123456789"` ones.
//!
//! The link *keys* the checksum with the session (see [`CrcKind::compute`]): a
//! frame left over from an earlier session fails the check exactly as a
//! corrupted frame does, so no stale frame can be taken for a current one.

/// Which checksum a link's frames carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrcKind {
    /// 2 bytes. Undetected random damage: about 1 in 65,536.
    Crc16,
    /// 4 bytes. Undetected random damage: about 1 in 4.3 billion.
    Crc32c,
}

impl CrcKind {
    /// Bytes the checksum adds to each frame.
    pub const fn len(self) -> usize {
        match self {
            CrcKind::Crc16 => 2,
            CrcKind::Crc32c => 4,
        }
    }

    /// The checksum of `data`, keyed with `key` (0 = unkeyed), in the low
    /// [`len`](Self::len) bytes.
    pub fn compute(self, key: u32, data: &[u8]) -> u32 {
        match self {
            CrcKind::Crc16 => {
                let fold = (key as u16) ^ ((key >> 16) as u16);
                crc16_ccitt(0xFFFF ^ fold, data) as u32
            }
            CrcKind::Crc32c => crc32c(key, data),
        }
    }
}

/// CRC-16/CCITT-FALSE (poly 0x1021, MSB first, no final xor) from `init`.
pub fn crc16_ccitt(init: u16, data: &[u8]) -> u16 {
    let mut crc = init;
    for &b in data {
        crc = (crc << 4) ^ CRC16_NIBBLES[(((crc >> 12) as u8 ^ (b >> 4)) & 0xF) as usize];
        crc = (crc << 4) ^ CRC16_NIBBLES[(((crc >> 12) as u8 ^ b) & 0xF) as usize];
    }
    crc
}

/// CRC-32C (poly 0x1EDC6F41, reflected 0x82F63B78, init and final xor all
/// ones), with `key` xored into the initial value.
pub fn crc32c(key: u32, data: &[u8]) -> u32 {
    let mut crc = !key;
    for &b in data {
        crc = (crc >> 4) ^ CRC32C_NIBBLES[((crc ^ b as u32) & 0xF) as usize];
        crc = (crc >> 4) ^ CRC32C_NIBBLES[((crc ^ (b >> 4) as u32) & 0xF) as usize];
    }
    !crc
}

static CRC16_NIBBLES: [u16; 16] = {
    let mut t = [0u16; 16];
    let mut i = 0;
    while i < 16 {
        let mut c = (i as u16) << 12;
        let mut k = 0;
        while k < 4 {
            c = if c & 0x8000 != 0 {
                (c << 1) ^ 0x1021
            } else {
                c << 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

static CRC32C_NIBBLES: [u32; 16] = {
    let mut t = [0u32; 16];
    let mut i = 0;
    while i < 16 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 4 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0x82F6_3B78
            } else {
                c >> 1
            };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_values() {
        assert_eq!(crc16_ccitt(0xFFFF, b"123456789"), 0x29B1);
        assert_eq!(crc32c(0, b"123456789"), 0xE306_9283);
    }

    #[test]
    fn key_changes_the_checksum() {
        for kind in [CrcKind::Crc16, CrcKind::Crc32c] {
            assert_ne!(
                kind.compute(0, b"frame"),
                kind.compute(0x1234_5678, b"frame")
            );
        }
    }
}
