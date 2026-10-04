//! [`PasswordChange`]: what a Wi‑Fi change does to the saved password —
//! and the one place Studio holds a typed password, for as long as the
//! press takes to reach the board.

use core::fmt;

use lpc_wire::WifiPassword;

/// What a set does to the board's saved Wi‑Fi password.
///
/// Studio never stores the password (plan Q8): it lives in the form until
/// the press, here until the request leaves, and nowhere after. `Debug`
/// never prints it, so an op formatted for the session recorder, a log or a
/// panic carries `Set(<redacted>)`.
#[derive(Clone, PartialEq, Eq)]
pub enum PasswordChange {
    /// Leave the saved password as it is (same network, field left blank).
    Keep,
    /// An open network: no password (`""` on the wire).
    Open,
    /// This password.
    Set(String),
}

impl PasswordChange {
    /// What the `NetworkSet` request carries: nothing for [`Self::Keep`],
    /// `""` for [`Self::Open`], the password for [`Self::Set`].
    pub fn to_wire(&self) -> Option<WifiPassword> {
        match self {
            Self::Keep => None,
            Self::Open => Some(WifiPassword::new("")),
            Self::Set(password) => Some(WifiPassword::new(password.clone())),
        }
    }
}

impl fmt::Debug for PasswordChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Keep => f.write_str("Keep"),
            Self::Open => f.write_str("Open"),
            Self::Set(_) => f.write_str("Set(<redacted>)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_password() {
        let set = PasswordChange::Set("correct-horse-42".to_string());
        assert_eq!(format!("{set:?}"), "Set(<redacted>)");
        assert_eq!(format!("{:?}", PasswordChange::Keep), "Keep");
    }

    #[test]
    fn the_wire_carries_nothing_empty_or_the_password() {
        assert_eq!(PasswordChange::Keep.to_wire(), None);
        assert_eq!(PasswordChange::Open.to_wire(), Some(WifiPassword::new("")));
        assert_eq!(
            PasswordChange::Set("pw-12345".to_string()).to_wire(),
            Some(WifiPassword::new("pw-12345"))
        );
    }
}
