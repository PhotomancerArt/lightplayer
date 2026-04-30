use alloc::string::String;
use core::fmt;

/// A **human-readable label** and path segment: non-empty, ASCII
/// alphanumerics and `_`, and must not start with a digit. Used inside
/// [`NodePathSegment`] and struct field keys (see
/// `docs/plans-old/2026-04-22-lp-domain-m2-domain-skeleton/00-design.md` — `Name` grammar).
#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodeName(pub String);

impl NodeName {
    /// Parses and validates a [`NodeName`] from a string, enforcing the v0
    /// character rules above.
    pub fn parse(s: &str) -> Result<Self, NodeNameError> {
        if s.is_empty() {
            return Err(NodeNameError::Empty);
        }
        for c in s.chars() {
            if !(c.is_ascii_alphanumeric() || c == '_') {
                return Err(NodeNameError::InvalidChar(c));
            }
        }
        if let Some(first) = s.chars().next() {
            if first.is_ascii_digit() {
                return Err(NodeNameError::LeadingDigit);
            }
        }
        Ok(NodeName(String::from(s)))
    }
}
/// Parse failure for [`NodeName::parse`]: empty string, disallowed first character, or a character outside the allowed set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeNameError {
    /// Empty input.
    Empty,
    /// First character is ASCII digit (names must be identifiers, `m2` design: `[A-Za-z0-9_]+` with no leading digit).
    LeadingDigit,
    /// A character is not in `[A-Za-z0-9_]`.
    InvalidChar(char),
}

impl fmt::Display for NodeNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("name is empty"),
            Self::LeadingDigit => f.write_str("name must not start with a digit"),
            Self::InvalidChar(c) => write!(f, "invalid character in name: {c:?}"),
        }
    }
}

impl core::error::Error for NodeNameError {}

impl fmt::Display for NodeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::NodeName;

    #[test]
    fn name_parse_accepts_valid() {
        for s in ["foo", "foo_bar_42", "_x", "X1"] {
            NodeName::parse(s).unwrap_or_else(|e| panic!("rejected {s:?}: {e}"));
        }
    }

    #[test]
    fn name_parse_rejects_invalid() {
        for s in ["", "1foo", "foo-bar", "foo bar", "foo.bar"] {
            assert!(NodeName::parse(s).is_err(), "should have rejected {s:?}");
        }
    }
}
