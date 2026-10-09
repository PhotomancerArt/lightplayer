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

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
