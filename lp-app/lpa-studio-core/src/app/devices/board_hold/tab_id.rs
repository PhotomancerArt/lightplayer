//! [`TabId`]: which Studio tab of this browser said a hold note.

use core::fmt;

use serde::{Deserialize, Serialize};

/// One Studio tab's id on the hold channel.
///
/// Minted by the EDGE (random bytes from the browser), never by core: core
/// makes no randomness (sans-IO). Lives only as long as the tab's page;
/// never persisted, and it names nothing about the browser or the person.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TabId(pub String);

impl TabId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// A tab's id from random bytes the edge drew (the browser's
    /// `crypto.getRandomValues`; core makes no randomness): the bytes as
    /// lowercase hex.
    pub fn from_random_bytes(bytes: &[u8]) -> Self {
        Self(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_id_is_its_random_bytes_in_hex() {
        let id = TabId::from_random_bytes(&[0x00, 0x0f, 0xa0, 0xff]);
        assert_eq!(id.as_str(), "000fa0ff");
        assert_eq!(TabId::from_random_bytes(&[0xab; 16]).as_str().len(), 32);
    }

    #[test]
    fn a_tab_id_travels_as_a_plain_string() {
        let id = TabId::new("3f2a");
        assert_eq!(serde_json::to_string(&id).expect("serialize"), "\"3f2a\"");
        assert_eq!(id.to_string(), "3f2a");
    }
}
