//! A board's password, as a host holds it for a locked board's LAN link.

use std::fmt;

/// The password a person gave for a locked board. Never printed: its
/// `Debug` says only that there is one, and it has no `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct BoardPassword(String);

impl BoardPassword {
    /// Hold `password` (one line of stdin, `LP_PASSWORD`; never argv).
    pub fn new(password: impl Into<String>) -> Self {
        Self(password.into())
    }

    /// The password's bytes, for the key derivation.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Whether it is the empty string.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for BoardPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BoardPassword(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_password() {
        let password = BoardPassword::new("hunter2");
        assert_eq!(format!("{password:?}"), "BoardPassword(..)");
        assert_eq!(password.as_bytes(), b"hunter2");
    }
}
