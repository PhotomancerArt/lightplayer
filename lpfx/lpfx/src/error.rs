//! Errors from manifest parsing and validation.

use core::fmt;

/// Manifest parse or validation failure.
#[derive(Debug)]
pub enum FxError {
    /// TOML syntax or structure could not be deserialized.
    TomlParse(toml::de::Error),
    /// A required field was missing or empty.
    MissingField {
        section: &'static str,
        field: &'static str,
    },
    /// An input's `type` string was not recognized.
    InvalidType {
        input: alloc::string::String,
        found: alloc::string::String,
    },
    /// `default`, `min`, or `max` did not match the input's declared type.
    DefaultTypeMismatch {
        input: alloc::string::String,
        expected: alloc::string::String,
        found: alloc::string::String,
    },
    /// `ui` string form was not recognized (see [`crate::input::FxPresentation`]).
    InvalidUi {
        input: alloc::string::String,
        found: alloc::string::String,
    },
    /// Semantic validation failed (rule described in message).
    ValidationError(alloc::string::String),
}

impl fmt::Display for FxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TomlParse(e) => write!(f, "TOML parse error: {e}"),
            Self::MissingField { section, field } => {
                write!(f, "missing or empty field `{field}` in [{section}]")
            }
            Self::InvalidType { input, found } => {
                write!(f, "input `{input}`: unknown type `{found}`")
            }
            Self::DefaultTypeMismatch {
                input,
                expected,
                found,
            } => {
                write!(
                    f,
                    "input `{input}`: expected {expected} for default/min/max, found {found}"
                )
            }
            Self::InvalidUi { input, found } => {
                write!(f, "input `{input}`: unknown ui `{found}`")
            }
            Self::ValidationError(msg) => f.write_str(msg),
        }
    }
}

impl From<toml::de::Error> for FxError {
    fn from(e: toml::de::Error) -> Self {
        Self::TomlParse(e)
    }
}
