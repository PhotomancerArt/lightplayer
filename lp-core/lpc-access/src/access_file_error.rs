//! Why an access file could not be read.

use alloc::string::String;
use core::fmt;

/// A persisted access file that failed to parse or validate.
///
/// Every variant is a refusal: a board that cannot read its access files
/// installs NO secrets from them (and a device store that fails to read is
/// treated as locked), so a damaged file can only ever take access away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessFileError {
    /// Not JSON of the expected shape.
    Malformed(String),
    /// A `version` this build does not speak.
    UnsupportedVersion(u32),
    /// A secret with `iterations == 0` (PBKDF2 requires at least one).
    ZeroIterations { label: String },
    /// More secrets than [`crate::MAX_SECRETS_PER_FILE`].
    TooManySecrets(usize),
}

impl fmt::Display for AccessFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "malformed access file: {message}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported access file version {version}")
            }
            Self::ZeroIterations { label } => {
                write!(f, "secret {label:?} has zero KDF iterations")
            }
            Self::TooManySecrets(count) => write!(
                f,
                "{count} secrets in one access file (at most {})",
                crate::MAX_SECRETS_PER_FILE
            ),
        }
    }
}
