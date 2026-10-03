//! [`OfferArgError`]: why a press's values were refused, in words the app
//! agent reads back and acts on.

use core::fmt;

/// Why [`crate::UiOffer::press`] refused a press. Each variant names the
/// parameter and what was wrong with it, plainly: the agent reads the
/// sentence and tries again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OfferArgError {
    /// The press carried a parameter the offer does not take.
    Unknown {
        name: String,
        /// The parameters the offer does take (empty for a parameterless
        /// one).
        known: Vec<String>,
    },
    /// A required parameter had no value and no default.
    Missing { name: String, label: String },
    /// A choice's value is not one of its options.
    NotAnOption {
        name: String,
        value: String,
        options: Vec<String>,
    },
    /// A choice's value is an option that cannot be picked right now.
    OptionDisabled {
        name: String,
        value: String,
        reason: String,
    },
    /// Text longer than the parameter allows.
    TooLong { name: String, max_len: usize },
    /// A toggle's value is not `true` or `false`.
    NotAToggle { name: String, value: String },
    /// A value was given for a parameter that does not apply to what the
    /// rest of the press picked (a push's `name` names a new project, never
    /// an example).
    Inapplicable { name: String, reason: String },
    /// The offer cannot be pressed right now at all, whatever it is given.
    Unavailable { reason: String },
}

impl fmt::Display for OfferArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown { name, known } if known.is_empty() => {
                write!(f, "this offer takes no parameters, so `{name}` is not one")
            }
            Self::Unknown { name, known } => write!(
                f,
                "this offer has no parameter `{name}`; it takes {}",
                known.join(", ")
            ),
            Self::Missing { name, label } => {
                write!(f, "`{name}` is required: choose a {label}")
            }
            Self::NotAnOption {
                name,
                value,
                options,
            } => write!(
                f,
                "`{name}` must be one of {}; `{value}` is not one of them",
                options.join(", ")
            ),
            Self::OptionDisabled {
                name,
                value,
                reason,
            } => write!(f, "`{name}` cannot be `{value}` right now: {reason}"),
            Self::TooLong { name, max_len } => {
                write!(f, "`{name}` is longer than {max_len} characters")
            }
            Self::NotAToggle { name, value } => {
                write!(f, "`{name}` takes true or false, not `{value}`")
            }
            Self::Inapplicable { name, reason } => {
                write!(f, "`{name}` does not apply here: {reason}")
            }
            Self::Unavailable { reason } => write!(f, "this cannot be done right now: {reason}"),
        }
    }
}

impl std::error::Error for OfferArgError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_refusal_names_the_parameter_and_the_problem() {
        let cases = [
            (
                OfferArgError::Unknown {
                    name: "colour".to_string(),
                    known: vec!["board".to_string(), "name".to_string()],
                },
                "this offer has no parameter `colour`; it takes board, name",
            ),
            (
                OfferArgError::Unknown {
                    name: "colour".to_string(),
                    known: Vec::new(),
                },
                "this offer takes no parameters, so `colour` is not one",
            ),
            (
                OfferArgError::Missing {
                    name: "board".to_string(),
                    label: "board".to_string(),
                },
                "`board` is required: choose a board",
            ),
            (
                OfferArgError::NotAnOption {
                    name: "board".to_string(),
                    value: "esp8266".to_string(),
                    options: vec!["xiao".to_string(), "devkit".to_string()],
                },
                "`board` must be one of xiao, devkit; `esp8266` is not one of them",
            ),
            (
                OfferArgError::OptionDisabled {
                    name: "board".to_string(),
                    value: "devkit".to_string(),
                    reason: "no build served".to_string(),
                },
                "`board` cannot be `devkit` right now: no build served",
            ),
            (
                OfferArgError::TooLong {
                    name: "name".to_string(),
                    max_len: 32,
                },
                "`name` is longer than 32 characters",
            ),
            (
                OfferArgError::NotAToggle {
                    name: "enabled".to_string(),
                    value: "yes".to_string(),
                },
                "`enabled` takes true or false, not `yes`",
            ),
            (
                OfferArgError::Inapplicable {
                    name: "name".to_string(),
                    reason: "it names a new project".to_string(),
                },
                "`name` does not apply here: it names a new project",
            ),
            (
                OfferArgError::Unavailable {
                    reason: "Firmware updates need USB.".to_string(),
                },
                "this cannot be done right now: Firmware updates need USB.",
            ),
        ];
        for (error, text) in cases {
            assert_eq!(error.to_string(), text);
        }
    }
}
