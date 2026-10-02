//! The device's answer to a secure link's key lookup: which installed
//! entries a key id names, as PSKs to try, best tier first.
//!
//! The key id is the entry's 16-byte salt, which the client sends in the
//! clear (it is already each entry's public identity: `AccessRemove` names
//! it, a login challenge offers it). One holder uses one salt everywhere, so
//! a salt is normally one entry; the device store and a loaded project's
//! sidecar may both hold it (perhaps at different tiers), so every match is
//! returned, edit first, and the secure link takes the first whose PSK
//! verifies.
//!
//! An empty answer is an unknown key: no secret was tested, and the caller
//! does not charge it to the login backoff. The anonymous key id (all zero)
//! is the caller's to handle; no stored entry may have it
//! ([`crate::DeviceAccessFile::upsert_secret`] refuses one).

use alloc::vec::Vec;

use crate::link_psk::link_psk;
use crate::secret_entry::{SALT_BYTES, SecretEntry};
use crate::tier::Tier;

/// One entry a key id names: the PSK its handshake must match, and the
/// tier a match grants.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyCandidate {
    pub psk: [u8; 32],
    pub tier: Tier,
}

impl core::fmt::Debug for KeyCandidate {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KeyCandidate")
            .field("tier", &self.tier)
            .finish_non_exhaustive()
    }
}

/// Every installed entry with `salt`, edit before play (stable within a
/// tier).
#[must_use]
pub fn key_candidates(installed: &[SecretEntry], salt: &[u8; SALT_BYTES]) -> Vec<KeyCandidate> {
    let mut found: Vec<KeyCandidate> = installed
        .iter()
        .filter(|entry| &entry.salt == salt)
        .map(|entry| KeyCandidate {
            psk: link_psk(&entry.k),
            tier: entry.tier,
        })
        .collect();
    found.sort_by_key(|c| core::cmp::Reverse(c.tier));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret_kind::SecretKind;

    fn entry(tier: Tier, salt: u8, k: u8) -> SecretEntry {
        SecretEntry {
            label: alloc::format!("{tier:?}-{k}"),
            kind: SecretKind::Browser,
            tier,
            salt: [salt; SALT_BYTES],
            iterations: 1,
            k: [k; 32],
            added_at: None,
        }
    }

    #[test]
    fn every_match_comes_back_edit_first() {
        let installed = [
            entry(Tier::Play, 1, 10),
            entry(Tier::Edit, 2, 20),
            entry(Tier::Edit, 1, 11),
            entry(Tier::Play, 1, 12),
        ];
        let found = key_candidates(&installed, &[1; SALT_BYTES]);
        let tiers: Vec<Tier> = found.iter().map(|c| c.tier).collect();
        assert_eq!(tiers, [Tier::Edit, Tier::Play, Tier::Play]);
        assert_eq!(found[0].psk, link_psk(&[11; 32]));
        assert_eq!(found[1].psk, link_psk(&[10; 32]), "stable within a tier");
        assert_eq!(found[2].psk, link_psk(&[12; 32]));
    }

    #[test]
    fn an_unknown_salt_finds_nothing() {
        assert!(key_candidates(&[entry(Tier::Edit, 1, 1)], &[9; SALT_BYTES]).is_empty());
        assert!(key_candidates(&[], &[0; SALT_BYTES]).is_empty());
    }
}
