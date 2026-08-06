//! A counting stand-in for randomness.

use lp_cloud_domain::{IdMint, SESSION_TOKEN_LEN};

/// A deterministic [`IdMint`]: a counter, not an rng.
///
/// Every call returns different bytes, and the same program run twice
/// returns the same bytes — which is what makes a minted `usr_` uid
/// assertable in a test.
///
/// **Never use this in production.** Session tokens are bearer credentials;
/// a counting one is a login bypass. The server edge injects a
/// cryptographically-secure mint.
#[derive(Debug, Clone, Default)]
pub struct MemIdMint {
    next: u128,
}

impl MemIdMint {
    /// A mint starting from zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// A mint starting from a chosen point, for tests that want two mints
    /// whose outputs cannot collide.
    pub fn starting_at(next: u128) -> Self {
        Self { next }
    }

    fn step(&mut self) -> [u8; 16] {
        let bytes = self.next.to_be_bytes();
        self.next = self.next.wrapping_add(1);
        bytes
    }
}

impl IdMint for MemIdMint {
    fn uid_bytes(&mut self) -> [u8; 16] {
        self.step()
    }

    fn session_token(&mut self) -> [u8; SESSION_TOKEN_LEN] {
        let mut token = [0u8; SESSION_TOKEN_LEN];
        token[..16].copy_from_slice(&self.step());
        token[16..].copy_from_slice(&self.step());
        token
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_draw_differs() {
        let mut mint = MemIdMint::new();
        let first = mint.uid_bytes();
        let second = mint.uid_bytes();
        assert_ne!(first, second);
        let token = mint.session_token();
        assert_ne!(token[..16], token[16..]);
    }

    #[test]
    fn the_same_seed_replays() {
        let mut a = MemIdMint::new();
        let mut b = MemIdMint::new();
        assert_eq!(a.uid_bytes(), b.uid_bytes());
        assert_ne!(MemIdMint::starting_at(9).uid_bytes(), a.uid_bytes());
    }
}
