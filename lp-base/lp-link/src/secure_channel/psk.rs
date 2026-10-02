//! A pre-shared key: 32 secret bytes, wiped when dropped and never printed.

use core::fmt;

use zeroize::Zeroize;

/// A Noise PSK. lpc-access derives it from an access entry's key
/// (`link_psk(K) = HMAC-SHA256(K, "lp-link psk/1")`).
pub struct Psk([u8; 32]);

impl Psk {
    /// The anonymous key's PSK: all zero. It encrypts; it authenticates nobody.
    pub const ANONYMOUS: Psk = Psk([0; 32]);

    pub const fn new(bytes: [u8; 32]) -> Self {
        Psk(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Clone for Psk {
    fn clone(&self) -> Self {
        Psk(self.0)
    }
}

impl Drop for Psk {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Psk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Psk(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn debug_never_prints_the_bytes() {
        let p = Psk::new([0xAB; 32]);
        assert_eq!(format!("{p:?}"), "Psk(..)");
        assert_eq!(p.clone().as_bytes(), &[0xAB; 32]);
    }
}
