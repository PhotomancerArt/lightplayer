//! The random bytes a LAN link's secure handshake draws (32 per handshake,
//! its ephemeral key): the operating system's.

/// Fill `buf` from the OS's random source.
///
/// # Panics
/// If the OS has none to give: a secure handshake cannot be made without
/// it, and there is nothing safe to fall back to.
pub fn os_entropy(buf: &mut [u8]) {
    if let Err(error) = getrandom::fill(buf) {
        panic!("the operating system gave no random bytes for a secure handshake: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_draws_differ() {
        let (mut a, mut b) = ([0u8; 32], [0u8; 32]);
        os_entropy(&mut a);
        os_entropy(&mut b);
        assert_ne!(a, b);
    }
}
