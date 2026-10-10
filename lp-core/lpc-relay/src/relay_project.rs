//! What a board tells the hub about the project it plays, and the keyed
//! tags that stand in for the project's capabilities.
//!
//! ```text
//! tag_key     = project_tag_key(K)          = HMAC-SHA256(K, "lp-relay project/1")
//! uid_tag     = project_uid_tag(tag_key, uid)     = HMAC-SHA256(tag_key, "uid\0" ‖ uid)[..16]
//! content_tag = project_content_tag(tag_key, h32) = HMAC-SHA256(tag_key, "content\0" ‖ h32)[..16]
//! ```
//!
//! **Why tags.** A project's uid is a read capability (projects are
//! viewable by anyone holding the link; the uid is the protection), and a
//! package's content hash is one too (blobs and trees are served by hash).
//! The device leg is plain HTTP, so neither may cross it: whoever is on the
//! path would read them. The board sends the project's **name** in the
//! clear, like its own name, and its uid and content hash only as tags. The
//! cloud, which knows the account's key and the account's projects, can
//! compute the same tags and match them; an observer cannot reverse them.
//!
//! **Which key.** `K` is the account entry's key exactly as the board
//! stores it — the same `K` the relay proof is made from
//! ([`crate::relay_proof`]). The board keys the tags with the **first
//! verified account**: the lowest set bit of `Registered.accounts_ok`,
//! which the hub knows as `BoardAccounts.users[0]` (the accounts the board
//! proved, in hello order).
//!
//! **Domain separation.** The label keeps the tag key apart from the relay
//! proof key (`HMAC(K, "lp-relay auth/1")`) and from the lp-link PSK
//! (`HMAC(K, "lp-link psk/1")`), and the `"uid\0"` / `"content\0"`
//! prefixes keep the two tags apart from each other, so no tag can stand in
//! for a proof, a PSK, or the other tag.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_access::{HmacSha256, KEY_BYTES, hmac_sha256};

use crate::frame_reader::FrameReader;
use crate::relay_frame::RelayFrameError;
use crate::relay_hello::cut_utf8;
use crate::relay_limits::{MAX_PROJECT_NAME_BYTES, PROJECT_TAG_BYTES};

/// The label that domain-separates the project tag key.
pub const RELAY_PROJECT_LABEL: &[u8] = b"lp-relay project/1";

/// The project a board has loaded, as it tells the hub
/// ([`RelayFrame::Project`](crate::RelayFrame::Project), protocol 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayProject {
    /// The project's name for people (project.json's `name`, else its
    /// folder's), at most [`MAX_PROJECT_NAME_BYTES`] of UTF-8.
    pub name: String,
    /// [`project_uid_tag`] of the project's uid, when it has one.
    pub uid_tag: Option<[u8; PROJECT_TAG_BYTES]>,
    /// [`project_content_tag`] of the package hash, when the board computed
    /// one.
    pub content_tag: Option<[u8; PROJECT_TAG_BYTES]>,
}

impl RelayProject {
    /// The fields' bytes, after the frame's own presence byte: the name
    /// (`len u8`, UTF-8, cut to [`MAX_PROJECT_NAME_BYTES`] on a character
    /// boundary), then each tag (`0` / `1` + 16 bytes).
    pub(crate) fn put(&self, out: &mut Vec<u8>) {
        let name = cut_utf8(&self.name, MAX_PROJECT_NAME_BYTES);
        out.push(name.len() as u8);
        out.extend_from_slice(name.as_bytes());
        put_tag(out, self.uid_tag.as_ref());
        put_tag(out, self.content_tag.as_ref());
    }

    /// Read the fields [`Self::put`] writes.
    pub(crate) fn read(r: &mut FrameReader<'_>) -> Result<Self, RelayFrameError> {
        let name = String::from(r.short_str(MAX_PROJECT_NAME_BYTES)?);
        let uid_tag = read_tag(r)?;
        let content_tag = read_tag(r)?;
        Ok(Self {
            name,
            uid_tag,
            content_tag,
        })
    }
}

/// The tag key for an account entry whose key is `k`.
#[must_use]
pub fn project_tag_key(k: &[u8; KEY_BYTES]) -> [u8; 32] {
    hmac_sha256(k, RELAY_PROJECT_LABEL)
}

/// A project uid's tag under `tag_key` ([`project_tag_key`]).
#[must_use]
pub fn project_uid_tag(tag_key: &[u8; 32], uid: &str) -> [u8; PROJECT_TAG_BYTES] {
    tag(tag_key, b"uid\0", uid.as_bytes())
}

/// A package content hash's tag under `tag_key` ([`project_tag_key`]).
#[must_use]
pub fn project_content_tag(tag_key: &[u8; 32], content_hash: &[u8; 32]) -> [u8; PROJECT_TAG_BYTES] {
    tag(tag_key, b"content\0", content_hash)
}

fn tag(tag_key: &[u8; 32], prefix: &[u8], value: &[u8]) -> [u8; PROJECT_TAG_BYTES] {
    let mut mac = HmacSha256::new(tag_key);
    mac.update(prefix);
    mac.update(value);
    let full = mac.finalize();
    let mut out = [0u8; PROJECT_TAG_BYTES];
    out.copy_from_slice(&full[..PROJECT_TAG_BYTES]);
    out
}

fn put_tag(out: &mut Vec<u8>, tag: Option<&[u8; PROJECT_TAG_BYTES]>) {
    match tag {
        None => out.push(0),
        Some(tag) => {
            out.push(1);
            out.extend_from_slice(tag);
        }
    }
}

fn read_tag(r: &mut FrameReader<'_>) -> Result<Option<[u8; PROJECT_TAG_BYTES]>, RelayFrameError> {
    if r.flag()? {
        Ok(Some(r.array()?))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    /// Computed independently with RustCrypto's `hmac`: the oracle.
    #[test]
    fn the_tags_are_the_documented_hmac_chain() {
        let k = [0x42; 32];
        let uid = "prj7m3qk2x9z4w8v6t5r1n0p2a4c";
        let hash = [0x11; 32];

        let mut key = Hmac::<Sha256>::new_from_slice(&k).unwrap();
        key.update(b"lp-relay project/1");
        let key: [u8; 32] = key.finalize().into_bytes().into();

        let mut uid_mac = Hmac::<Sha256>::new_from_slice(&key).unwrap();
        uid_mac.update(b"uid\0");
        uid_mac.update(uid.as_bytes());
        let uid_full: [u8; 32] = uid_mac.finalize().into_bytes().into();

        let mut content_mac = Hmac::<Sha256>::new_from_slice(&key).unwrap();
        content_mac.update(b"content\0");
        content_mac.update(&hash);
        let content_full: [u8; 32] = content_mac.finalize().into_bytes().into();

        assert_eq!(project_tag_key(&k), key);
        assert_eq!(project_uid_tag(&key, uid), uid_full[..16]);
        assert_eq!(project_content_tag(&key, &hash), content_full[..16]);
    }

    #[test]
    fn the_tag_key_is_neither_the_proof_key_nor_the_link_psk() {
        let k = [7; 32];
        let tag_key = project_tag_key(&k);
        assert_ne!(tag_key, crate::relay_auth_key(&k));
        assert_ne!(tag_key, lpc_access::link_psk(&k));
        assert_ne!(tag_key, k);
    }

    #[test]
    fn a_tag_is_bound_to_its_key_and_its_kind() {
        let key = project_tag_key(&[1; 32]);
        let other = project_tag_key(&[2; 32]);
        let uid = "prj0000000000000000";
        assert_ne!(project_uid_tag(&key, uid), project_uid_tag(&other, uid));
        assert_ne!(
            project_uid_tag(&key, uid),
            project_uid_tag(&key, "prj0000000000000001")
        );
        // The same 32 bytes as a uid and as a hash tag differently.
        let bytes = [b'a'; 32];
        let as_text = core::str::from_utf8(&bytes).unwrap();
        assert_ne!(
            project_uid_tag(&key, as_text),
            project_content_tag(&key, &bytes)
        );
    }
}
