//! A key a secure link presents, and where a link's keys come from.
//!
//! A secure lp-link initiator names a key id in the clear and proves it
//! holds that key's PSK (Noise NNpsk0 inside the SYN). The id is an access
//! entry's salt and the PSK is `lpc_access::link_psk(K)` of the entry's key —
//! the same keys Studio unlocks a Bluetooth board with, so this crate never
//! derives one: the app hands them over through [`LinkKeys`].
//!
//! Plain bytes rather than lp-link's own `KeyId`/`Psk`, so this file builds
//! (and the app implements [`LinkKeys`]) without the secure channel's crypto
//! in the build; the provider converts at the edge.

/// Bytes in a key id (an access entry's salt).
pub const KEY_ID_BYTES: usize = 16;
/// Bytes in a link PSK.
pub const PSK_BYTES: usize = 32;

/// One key a secure link may present: the id the board looks it up by and
/// the PSK that proves it.
#[derive(Clone, PartialEq, Eq)]
pub struct LinkKey {
    pub key_id: [u8; KEY_ID_BYTES],
    pub psk: [u8; PSK_BYTES],
}

impl LinkKey {
    /// The anonymous key: all-zero id and PSK. A session on it is encrypted
    /// and authenticates nobody, so the board grants it what it is open to
    /// (play, edit, or nothing).
    pub const ANONYMOUS: LinkKey = LinkKey {
        key_id: [0; KEY_ID_BYTES],
        psk: [0; PSK_BYTES],
    };

    pub fn is_anonymous(&self) -> bool {
        self.key_id == [0; KEY_ID_BYTES]
    }
}

/// A PSK is login-equivalent: never in a log line. The id is the salt, which
/// the link names in the clear anyway; its first bytes are enough to tell
/// two keys apart in a journal.
impl core::fmt::Debug for LinkKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_anonymous() {
            return f.write_str("LinkKey(anonymous)");
        }
        write!(
            f,
            "LinkKey({:02x}{:02x}{:02x}{:02x}…, psk: <redacted>)",
            self.key_id[0], self.key_id[1], self.key_id[2], self.key_id[3]
        )
    }
}

/// Where a secure link's keys come from: Studio's access layer, which holds
/// this browser's keys, the account's, and the keys typed passwords derived.
///
/// Read by the provider on every new link and on every service pass (the
/// generation), so a key that arrives while a link is up — a password typed
/// for a locked board — reaches the board without anyone reconnecting by hand.
pub trait LinkKeys {
    /// The keys to present to the board at `address` (its socket URL), best
    /// first. The anonymous key is never among them: the walk tries it last
    /// on its own.
    fn keys_for(&self, address: &str) -> Vec<LinkKey>;

    /// A number that moves whenever [`Self::keys_for`] would answer
    /// differently for `address`.
    fn generation(&self, address: &str) -> u64;

    /// The board at `address` refused `key` as the WRONG key (it knows the
    /// id; the PSK did not match). Every such refusal is charged to the
    /// board's login backoff, so the key is not presented there again.
    fn refused_wrong(&self, address: &str, key: &LinkKey);
}

/// No keys at all: every link comes up anonymous, and an open board is all
/// it reaches. What a provider holds before the app hands it its keys.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoLinkKeys;

impl LinkKeys for NoLinkKeys {
    fn keys_for(&self, _address: &str) -> Vec<LinkKey> {
        Vec::new()
    }

    fn generation(&self, _address: &str) -> u64 {
        0
    }

    fn refused_wrong(&self, _address: &str, _key: &LinkKey) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_psk() {
        let key = LinkKey {
            key_id: [0xAB; KEY_ID_BYTES],
            psk: [0x5C; PSK_BYTES],
        };
        let shown = format!("{key:?}");
        assert!(shown.contains("abababab"), "{shown}");
        assert!(!shown.contains("5c"), "{shown}");
        assert_eq!(format!("{:?}", LinkKey::ANONYMOUS), "LinkKey(anonymous)");
    }

    #[test]
    fn only_the_all_zero_id_is_anonymous() {
        assert!(LinkKey::ANONYMOUS.is_anonymous());
        let mut key = LinkKey::ANONYMOUS;
        key.key_id[15] = 1;
        assert!(!key.is_anonymous());
    }
}
