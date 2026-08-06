//! Randomness, from the operating system's CSPRNG.

use lp_cloud_domain::{IdMint, SESSION_TOKEN_LEN};

/// The production [`IdMint`]: `getrandom`, which is the OS entropy source.
///
/// This is the counterpart the `MemIdMint` docs warn about — that one is a
/// counter, and a counting session token is a login bypass, because a
/// session token is a bearer credential that anybody may present.
///
/// A failure to draw entropy **panics**. There is no weaker fallback worth
/// having: a service that cannot get random bytes must not mint credentials
/// out of something else.
#[derive(Debug, Clone, Copy, Default)]
pub struct SecureMint;

impl IdMint for SecureMint {
    fn uid_bytes(&mut self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        fill(&mut bytes);
        bytes
    }

    fn session_token(&mut self) -> [u8; SESSION_TOKEN_LEN] {
        let mut token = [0u8; SESSION_TOKEN_LEN];
        fill(&mut token);
        token
    }
}

fn fill(bytes: &mut [u8]) {
    getrandom::fill(bytes).expect("lp-cloud-server: the OS refused to provide random bytes");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Not a randomness test — a wiring test. Two draws that match would
    /// mean the buffer never reached the CSPRNG at all.
    #[test]
    fn draws_differ() {
        let mut mint = SecureMint;
        assert_ne!(mint.uid_bytes(), mint.uid_bytes());
        assert_ne!(mint.session_token(), mint.session_token());
    }

    #[test]
    fn a_token_is_not_all_zeroes() {
        assert_ne!(SecureMint.session_token(), [0u8; SESSION_TOKEN_LEN]);
    }
}
