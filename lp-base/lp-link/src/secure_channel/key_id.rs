//! Which key a secure link's initiator holds, named in the clear in its first
//! SYN. Opaque to lp-link: lpc-access uses an access entry's 16-byte salt,
//! which is already that entry's public identity.

/// A key id: 16 bytes, sent in the clear in msg1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyId(pub [u8; 16]);

impl KeyId {
    /// The anonymous key's id: all zero. Its PSK is [`Psk::ANONYMOUS`](crate::secure_channel::Psk::ANONYMOUS).
    /// A session on it is encrypted but authenticates nobody.
    pub const ANONYMOUS: KeyId = KeyId([0; 16]);

    pub fn is_anonymous(&self) -> bool {
        *self == Self::ANONYMOUS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_all_zero_is_anonymous() {
        assert!(KeyId::ANONYMOUS.is_anonymous());
        let mut b = [0u8; 16];
        b[15] = 1;
        assert!(!KeyId(b).is_anonymous());
    }
}
