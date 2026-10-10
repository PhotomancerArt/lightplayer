//! [`UnlockPassword`]: the password an Unlock press carries — and the one
//! place Studio holds it, for as long as the press takes to reach the
//! access controller.

use core::fmt;

/// The device password a person typed into the Unlock sheet (or into the
/// offer's form).
///
/// Studio does not keep it: the sheet's field holds it until the press,
/// this holds it from the press until the access controller has tried it,
/// and the controller keeps it only if the person said to remember it. Its
/// `Debug` never prints it, so an op formatted for the session recorder
/// (`?record=`), a log or a panic carries `<redacted>`.
#[derive(Clone, PartialEq, Eq)]
pub struct UnlockPassword(String);

impl UnlockPassword {
    /// This text, as typed (spaces count: a password may hold them).
    pub fn new(password: impl Into<String>) -> Self {
        Self(password.into())
    }

    /// The password itself, for the one caller that tries it on a link.
    pub(crate) fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Debug for UnlockPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_the_password() {
        let password = UnlockPassword::new("correct-horse-42");
        assert_eq!(format!("{password:?}"), "<redacted>");
        assert_eq!(format!("{password:#?}"), "<redacted>");
    }

    #[test]
    fn the_password_comes_back_as_typed() {
        let typed = " two words ";
        assert_eq!(UnlockPassword::new(typed).into_string(), typed);
    }
}
