use super::{SlotMapKey, SlotName, SlotNameError};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Path through an owner's slot tree.
///
/// A slot path addresses independently versioned slot data. Dot segments
/// select record fields; bracket segments select map keys. Use
/// [`crate::ValuePath`] only for projection inside a leaf value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotPath(Vec<SlotPathSegment>);

/// One step through a slot tree.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum SlotPathSegment {
    /// Record field, enum variant, or option `some`.
    Field(SlotName),
    /// Stable map key.
    Key(SlotMapKey),
}

impl SlotPath {
    /// Root of a slot tree.
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// Parse a slot path such as `config.size` or `params["phase.offset"]`.
    pub fn parse(input: &str) -> Result<Self, SlotPathError> {
        if input.is_empty() {
            return Err(SlotPathError::EmptyPath);
        }

        let mut parser = SlotPathParser::new(input);
        let mut segments = Vec::new();
        loop {
            match parser.peek() {
                None => break,
                Some('.') => return Err(SlotPathError::EmptySegment),
                Some('[') => segments.push(SlotPathSegment::Key(parser.parse_key()?)),
                Some(_) => segments.push(SlotPathSegment::Field(parser.parse_field()?)),
            }

            match parser.peek() {
                None => break,
                Some('.') => {
                    parser.bump();
                    match parser.peek() {
                        None => return Err(SlotPathError::EmptySegment),
                        Some('.') => return Err(SlotPathError::EmptySegment),
                        Some('[') => return Err(SlotPathError::UnexpectedChar('[')),
                        Some(_) => {}
                    }
                }
                Some('[') => {}
                Some(c) => return Err(SlotPathError::UnexpectedChar(c)),
            }
        }

        Ok(Self(segments))
    }

    /// Build a path from already parsed segments.
    pub fn from_segments(segments: Vec<SlotPathSegment>) -> Self {
        Self(segments)
    }

    /// The path's segment list.
    pub fn segments(&self) -> &[SlotPathSegment] {
        &self.0
    }

    /// True when this path references the slot tree root.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// Return a new path with a record field child appended.
    pub fn child(&self, child: SlotName) -> Self {
        self.child_segment(SlotPathSegment::Field(child))
    }

    /// Return a new path with a map key child appended.
    pub fn child_key(&self, key: SlotMapKey) -> Self {
        self.child_segment(SlotPathSegment::Key(key))
    }

    /// Return a new path with `child` appended.
    pub fn child_segment(&self, child: SlotPathSegment) -> Self {
        let mut segments = self.0.clone();
        segments.push(child);
        Self(segments)
    }
}

impl fmt::Display for SlotPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, segment) in self.0.iter().enumerate() {
            match segment {
                SlotPathSegment::Field(name) => {
                    if index > 0 {
                        f.write_str(".")?;
                    }
                    f.write_str(name.as_str())?;
                }
                SlotPathSegment::Key(key) => write_key(f, key)?,
            }
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
    EmptyKey,
    InvalidSegment(SlotNameError),
    MissingBracket,
    UnterminatedString,
    UnexpectedChar(char),
}

impl fmt::Display for SlotPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPath => f.write_str("slot path is empty"),
            Self::EmptySegment => f.write_str("slot path contains an empty segment"),
            Self::EmptyKey => f.write_str("slot path contains an empty map key"),
            Self::InvalidSegment(err) => write!(f, "invalid slot path segment: {err}"),
            Self::MissingBracket => f.write_str("slot path map key is missing closing ']'"),
            Self::UnterminatedString => f.write_str("slot path quoted map key is unterminated"),
            Self::UnexpectedChar(c) => write!(f, "unexpected character in slot path: {c:?}"),
        }
    }
}

impl core::error::Error for SlotPathError {}

struct SlotPathParser<'a> {
    input: &'a str,
    index: usize,
}

impl<'a> SlotPathParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, index: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.index..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.index += c.len_utf8();
        Some(c)
    }

    fn parse_field(&mut self) -> Result<SlotName, SlotPathError> {
        let start = self.index;
        while let Some(c) = self.peek() {
            if c == '.' || c == '[' {
                break;
            }
            self.bump();
        }
        if self.index == start {
            return Err(SlotPathError::EmptySegment);
        }
        SlotName::parse(&self.input[start..self.index]).map_err(SlotPathError::InvalidSegment)
    }

    fn parse_key(&mut self) -> Result<SlotMapKey, SlotPathError> {
        debug_assert_eq!(self.peek(), Some('['));
        self.bump();
        match self.peek() {
            Some('"') => self.parse_quoted_key(),
            Some(']') => Err(SlotPathError::EmptyKey),
            None => Err(SlotPathError::MissingBracket),
            Some(_) => self.parse_bare_key(),
        }
    }

    fn parse_bare_key(&mut self) -> Result<SlotMapKey, SlotPathError> {
        let start = self.index;
        while let Some(c) = self.peek() {
            if c == ']' {
                break;
            }
            if c == '[' || c == '.' || c == '"' {
                return Err(SlotPathError::UnexpectedChar(c));
            }
            self.bump();
        }
        if self.index == start {
            return Err(SlotPathError::EmptyKey);
        }
        let raw = &self.input[start..self.index];
        if self.bump() != Some(']') {
            return Err(SlotPathError::MissingBracket);
        }
        Ok(parse_bare_key(raw))
    }

    fn parse_quoted_key(&mut self) -> Result<SlotMapKey, SlotPathError> {
        debug_assert_eq!(self.peek(), Some('"'));
        self.bump();
        let mut value = String::new();
        loop {
            match self.bump() {
                Some('"') => break,
                Some('\\') => match self.bump() {
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some(c) => return Err(SlotPathError::UnexpectedChar(c)),
                    None => return Err(SlotPathError::UnterminatedString),
                },
                Some(c) => value.push(c),
                None => return Err(SlotPathError::UnterminatedString),
            }
        }
        if self.bump() != Some(']') {
            return Err(SlotPathError::MissingBracket);
        }
        Ok(SlotMapKey::String(value))
    }
}

fn parse_bare_key(raw: &str) -> SlotMapKey {
    if let Ok(value) = raw.parse::<u32>() {
        SlotMapKey::U32(value)
    } else if let Ok(value) = raw.parse::<i32>() {
        SlotMapKey::I32(value)
    } else {
        SlotMapKey::String(raw.to_string())
    }
}

fn write_key(f: &mut fmt::Formatter<'_>, key: &SlotMapKey) -> fmt::Result {
    match key {
        SlotMapKey::String(value) if can_write_bare_string_key(value) => write!(f, "[{value}]"),
        SlotMapKey::String(value) => {
            f.write_str("[\"")?;
            for c in value.chars() {
                match c {
                    '"' => f.write_str("\\\"")?,
                    '\\' => f.write_str("\\\\")?,
                    '\n' => f.write_str("\\n")?,
                    '\t' => f.write_str("\\t")?,
                    c => f.write_str(&c.to_string())?,
                }
            }
            f.write_str("\"]")
        }
        SlotMapKey::I32(value) => write!(f, "[{value}]"),
        SlotMapKey::U32(value) => write!(f, "[{value}]"),
    }
}

fn can_write_bare_string_key(value: &str) -> bool {
    !value.is_empty()
        && value.parse::<u32>().is_err()
        && value.parse::<i32>().is_err()
        && !value
            .chars()
            .any(|c| c == '.' || c == '[' || c == ']' || c == '"' || c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn root_path_is_explicit() {
        let path = SlotPath::root();
        assert!(path.is_root());
        assert_eq!(path.to_string(), "");
        assert_eq!(SlotPath::parse(""), Err(SlotPathError::EmptyPath));
    }

    #[test]
    fn dotted_fields_round_trip() {
        let path = SlotPath::parse("config.size").unwrap();
        assert_eq!(path.segments().len(), 2);
        assert_eq!(path.to_string(), "config.size");
        assert!(matches!(
            &path.segments()[0],
            SlotPathSegment::Field(name) if name.as_str() == "config"
        ));
    }

    #[test]
    fn bracket_map_keys_round_trip() {
        let path = SlotPath::parse("params[phase].label").unwrap();
        assert_eq!(path.to_string(), "params[phase].label");
        assert!(matches!(
            &path.segments()[1],
            SlotPathSegment::Key(SlotMapKey::String(key)) if key == "phase"
        ));
    }

    #[test]
    fn dotted_string_map_keys_must_be_quoted() {
        let path = SlotPath::parse(r#"params["phase.offset"].label"#).unwrap();
        assert_eq!(path.to_string(), r#"params["phase.offset"].label"#);
        assert!(matches!(
            &path.segments()[1],
            SlotPathSegment::Key(SlotMapKey::String(key)) if key == "phase.offset"
        ));
        assert_eq!(
            SlotPath::parse("params[phase.offset]"),
            Err(SlotPathError::UnexpectedChar('.'))
        );
    }

    #[test]
    fn numeric_map_keys_round_trip() {
        assert_eq!(
            SlotPath::parse("touches[0]").unwrap().to_string(),
            "touches[0]"
        );
        assert!(matches!(
            &SlotPath::parse("offsets[-1]").unwrap().segments()[1],
            SlotPathSegment::Key(SlotMapKey::I32(-1))
        ));
    }

    #[test]
    fn numeric_string_keys_are_quoted_on_display() {
        let path = SlotPath::from_segments(vec![SlotPathSegment::Key(SlotMapKey::String(
            "0".to_string(),
        ))]);
        assert_eq!(path.to_string(), r#"["0"]"#);
    }

    #[test]
    fn rejects_empty_segments() {
        for input in [".config", "config.", "config..size"] {
            assert_eq!(SlotPath::parse(input), Err(SlotPathError::EmptySegment));
        }
    }

    #[test]
    fn serde_string_round_trip() {
        let path = SlotPath::parse(r#"state.outputs["main.texture"]"#).unwrap();
        let json = serde_json::to_string(&path).unwrap();
        assert_eq!(json, r#""state.outputs[\"main.texture\"]""#);
        let back: SlotPath = serde_json::from_str(&json).unwrap();
        assert_eq!(back, path);
    }
}
