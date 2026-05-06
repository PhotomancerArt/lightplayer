use alloc::string::{String, ToString};
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Name of one segment inside a slot owner's namespace.
///
/// Slot names are joined into [`crate::SlotPath`] values. They are separate
/// from [`crate::ValuePath`], which selects nested data inside a leaf value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotName(String);

impl SlotName {
    pub fn parse(input: &str) -> Result<Self, SlotNameError> {
        if input.is_empty() {
            return Err(SlotNameError::Empty);
        }
        if input.contains('#') {
            return Err(SlotNameError::InvalidChar('#'));
        }
        if input.contains('.') {
            return Err(SlotNameError::InvalidChar('.'));
        }
        Ok(Self(input.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SlotName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for SlotName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SlotName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

/// Error returned when parsing a [`SlotName`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotNameError {
    Empty,
    InvalidChar(char),
}

impl fmt::Display for SlotNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("slot name is empty"),
            Self::InvalidChar(c) => write!(f, "invalid character in slot name: {c:?}"),
        }
    }
}

impl core::error::Error for SlotNameError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn slot_name_accepts_segment_names() {
        let name = SlotName::parse("width").unwrap();
        assert_eq!(name.as_str(), "width");
        assert_eq!(name.to_string(), "width");
    }

    #[test]
    fn slot_name_rejects_empty_and_path_separators() {
        for input in ["", "output#image", "config.width"] {
            assert!(SlotName::parse(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn slot_name_deserializes_from_string() {
        let name: SlotName = serde_json::from_str(r#""param""#).unwrap();
        assert_eq!(name.as_str(), "param");
    }
}
