//! [`PasswordChange`]: the password a network is added with — and the one
//! place Studio holds a typed password, for as long as the press takes to
//! reach the board.

use core::fmt;

use lpc_wire::WifiPassword;

/// The password a `NetworkAdd` carries (adding a saved name again is how
/// its password is changed).
///
/// Studio never stores the password: it lives in the form until the press,
/// here until the request leaves, and nowhere after. `Debug` never prints
/// it, so an op formatted for the session recorder, a log or a panic
/// carries `Set(<redacted>)`.
#[derive(Clone, PartialEq, Eq)]
pub enum PasswordChange {
    /// An open network: no password (`""` on the wire).
    Open,
    /// This password.
    Set(String),
}

impl PasswordChange {
    /// What the `NetworkAdd` request carries: `""` for [`Self::Open`], the
    /// password for [`Self::Set`].
    pub fn to_wire(&self) -> WifiPassword {
        match self {
            Self::Open => WifiPassword::new(""),
            Self::Set(password) => WifiPassword::new(password.clone()),
        }
    }
}

impl fmt::Debug for PasswordChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
        assert_eq!(format!("{:?}", PasswordChange::Open), "Open");
    }

    #[test]
    fn the_wire_carries_empty_or_the_password() {
        assert_eq!(PasswordChange::Open.to_wire(), WifiPassword::new(""));
        assert_eq!(
            PasswordChange::Set("pw-12345".to_string()).to_wire(),
            WifiPassword::new("pw-12345")
        );
    }
}
