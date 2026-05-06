use super::{SlotName, SlotNameError};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Path through an owner's slot tree.
///
/// A slot path addresses independently versioned slot data. Use
/// [`crate::ValuePath`] only for projection inside a leaf value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotPath(Vec<SlotName>);

impl SlotPath {
    /// Root of a slot tree.
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// Parse a dotted slot path such as `config.size`.
    pub fn parse(input: &str) -> Result<Self, SlotPathError> {
        if input.is_empty() {
            return Err(SlotPathError::EmptyPath);
        }
        let mut segments = Vec::new();
        for segment in input.split('.') {
            if segment.is_empty() {
                return Err(SlotPathError::EmptySegment);
            }
            segments.push(SlotName::parse(segment).map_err(SlotPathError::InvalidSegment)?);
        }
        Ok(Self(segments))
    }

    /// Build a path from already parsed names.
    pub fn from_segments(segments: Vec<SlotName>) -> Self {
        Self(segments)
    }

    /// The path's segment list.
    pub fn segments(&self) -> &[SlotName] {
        &self.0
    }

    /// True when this path references the slot tree root.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// Return a new path with `child` appended.
    pub fn child(&self, child: SlotName) -> Self {
        let mut segments = self.0.clone();
        segments.push(child);
        Self(segments)
    }
}

impl fmt::Display for SlotPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, segment) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(".")?;
            }
            f.write_str(segment.as_str())?;
        }
        Ok(())
    }
}

impl Serialize for SlotPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SlotPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        if input.is_empty() {
            Ok(Self::root())
        } else {
            Self::parse(&input).map_err(serde::de::Error::custom)
        }
    }
}

/// Error returned when parsing a [`SlotPath`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotPathError {
    EmptyPath,
    EmptySegment,
    InvalidSegment(SlotNameError),
}

impl fmt::Display for SlotPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPath => f.write_str("slot path is empty"),
            Self::EmptySegment => f.write_str("slot path contains an empty segment"),
            Self::InvalidSegment(err) => write!(f, "invalid slot path segment: {err}"),
        }
    }
}

impl core::error::Error for SlotPathError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_path_is_explicit() {
        let path = SlotPath::root();
        assert!(path.is_root());
        assert_eq!(path.to_string(), "");
        assert_eq!(SlotPath::parse(""), Err(SlotPathError::EmptyPath));
    }

    #[test]
    fn dotted_path_round_trips() {
        let path = SlotPath::parse("config.size").unwrap();
        assert_eq!(path.segments().len(), 2);
        assert_eq!(path.to_string(), "config.size");
    }

    #[test]
    fn rejects_empty_segments() {
        for input in [".config", "config.", "config..size"] {
            assert_eq!(SlotPath::parse(input), Err(SlotPathError::EmptySegment));
        }
    }

    #[test]
    fn serde_string_round_trip() {
        let path = SlotPath::parse("state.output").unwrap();
        let json = serde_json::to_string(&path).unwrap();
        assert_eq!(json, r#""state.output""#);
        let back: SlotPath = serde_json::from_str(&json).unwrap();
        assert_eq!(back, path);
    }
}
