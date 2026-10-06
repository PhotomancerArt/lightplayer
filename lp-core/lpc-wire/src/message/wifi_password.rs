//! A Wi-Fi password on its way to a board, which never prints.

use alloc::string::String;
use core::fmt;
use serde::{Deserialize, Serialize};

/// The password inside [`crate::ClientRequest::NetworkAdd`].
///
/// Serialized as the bare string (`#[serde(transparent)]`); its `Debug` is
/// written by hand and prints `WifiPassword(<redacted>)`, so a request
/// formatted with `{:?}` — in a log, a panic, a journal — never carries it.
/// No reply carries a password at all: the board answers whether one is
/// set.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WifiPassword(String);

impl WifiPassword {
    /// Wrap a password (`""` for an open network).
    #[must_use]
    pub fn new(password: impl Into<String>) -> Self {
        Self(password.into())
    }

    /// The password itself, for the one place that stores it (the board's
    /// network store) or validates it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// The password itself, by value.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

/// Whether raw request text (a `M!` line, a link payload, the start of a
/// read buffer) may hold a Wi-Fi password: it names `networkAdd`. A
/// transport that previews or echoes received text in a log checks this
/// first and logs only the length — the parsed request's `Debug` is safe,
/// the raw bytes are not.
#[must_use]
pub fn may_carry_secret(text: &str) -> bool {
    text.contains("\"networkAdd\"")
}

impl fmt::Debug for WifiPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WifiPassword(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn serializes_as_the_bare_string_and_never_prints() {
        let password = WifiPassword::new("correct-horse-42");
        assert_eq!(
            crate::json::to_string(&password).unwrap(),
            "\"correct-horse-42\""
        );
        let shown = format!("{password:?} {password:#?}");
        assert!(!shown.contains("correct-horse-42"), "{shown}");
        let back: WifiPassword = crate::json::from_str("\"correct-horse-42\"").unwrap();
        assert_eq!(back.expose(), "correct-horse-42");
    }
}
