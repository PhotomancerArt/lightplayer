//! The authored output name (D39): how patch entries and the studio refer
//! to an output.
//!
//! "In the real world I do use numbers… 'Box 5' which corresponds to
//! 10.0.0.105" — the name is a short human label, never hardware identity:
//! renaming an output moves no wires, and no endpoint/IP/pin is derived
//! from it. Auto-assignment ([`next_output_name`]) hands out numeric
//! defaults ("1", "2", …) the first time a patch needs to name an output;
//! users edit them into "Box 5"-style labels. Project-uniqueness among
//! outputs is a resolve/validation-layer check, not enforced here — like
//! [`crate::NodeName`], the type validates shape locally.

use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, StaticLpType,
    StaticSlotValueShape, ToLpValue, ValueEditorHint, ValueRootError,
};

/// Longest output name, in characters (ASCII, so bytes = chars).
pub const OUTPUT_NAME_MAX_LEN: usize = 24;

/// A validated output display name: trimmed (no leading/trailing
/// whitespace), non-empty, at most [`OUTPUT_NAME_MAX_LEN`] printable ASCII
/// characters — interior spaces allowed ("Box 5").
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct OutputName(String);

impl OutputName {
    pub fn parse(name: impl Into<String>) -> Result<Self, OutputNameError> {
        let name = name.into();
        if name.is_empty() {
            return Err(OutputNameError::Empty);
        }
        if name.len() > OUTPUT_NAME_MAX_LEN {
            return Err(OutputNameError::TooLong { name });
        }
        if name.starts_with(' ') || name.ends_with(' ') {
            return Err(OutputNameError::Untrimmed { name });
        }
        if let Some(bad) = name.chars().find(|c| !c.is_ascii_graphic() && *c != ' ') {
            return Err(OutputNameError::InvalidChar { name, char: bad });
        }
        Ok(Self(name))
    }

    pub fn from_static(name: &'static str) -> Self {
        Self::parse(name).expect("static output name must be valid")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The next free numeric default: "1", "2", … skipping names already in
/// use. Pure — the UI invokes it when a verb first needs to name an
/// unnamed output; nothing auto-writes.
pub fn next_output_name<'a>(existing: impl Iterator<Item = &'a OutputName> + Clone) -> OutputName {
    for candidate in 1u32.. {
        let text = candidate.to_string();
        if !existing.clone().any(|name| name.as_str() == text) {
            return OutputName(text);
        }
    }
    unreachable!("u32 numeric space exceeds any project's output count")
}

/// The slot machinery's ensure-present seed: the first numeric default.
/// Real auto-assignment goes through [`next_output_name`], which skips
/// names already in use.
impl Default for OutputName {
    fn default() -> Self {
        Self(String::from("1"))
    }
}

impl fmt::Display for OutputName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for OutputName {
    type Err = OutputNameError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::parse(name)
    }
}

impl Serialize for OutputName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for OutputName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::parse(name).map_err(serde::de::Error::custom)
    }
}

impl ToLpValue for OutputName {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.0.clone())
    }
}

impl FromLpValue for OutputName {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => {
                Self::parse(value.clone()).map_err(|error| ValueRootError::new(error.to_string()))
            }
            other => Err(ValueRootError::new(format!(
                "expected OutputName string, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for OutputName {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("OutputName");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> = Some(
        StaticSlotValueShape::new(Self::SHAPE_ID, StaticLpType::String),
    );

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputNameError {
    Empty,
    TooLong { name: String },
    Untrimmed { name: String },
    InvalidChar { name: String, char: char },
}

impl fmt::Display for OutputNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("output name is empty"),
            Self::TooLong { name } => write!(
                f,
                "output name {name:?} is longer than {OUTPUT_NAME_MAX_LEN} characters"
            ),
            Self::Untrimmed { name } => {
                write!(f, "output name {name:?} must not start or end with a space")
            }
            Self::InvalidChar { name, char } => write!(
                f,
                "output name {name:?} contains {char:?}; printable ASCII only"
            ),
        }
    }
}

impl core::error::Error for OutputNameError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn accepts_real_world_names_and_refuses_the_rest() {
        for name in ["1", "Box 5", "west-arch", "A_2 (spare)"] {
            OutputName::parse(name).unwrap_or_else(|e| panic!("rejected {name:?}: {e}"));
        }
        for name in ["", " Box 5", "Box 5 ", "Box\t5", "Boîte 5"] {
            assert!(OutputName::parse(name).is_err(), "should reject {name:?}");
        }
        assert!(OutputName::parse("a".repeat(OUTPUT_NAME_MAX_LEN)).is_ok());
        assert!(OutputName::parse("a".repeat(OUTPUT_NAME_MAX_LEN + 1)).is_err());
    }

    #[test]
    fn next_output_name_hands_out_the_first_free_number() {
        let existing: Vec<OutputName> = vec![];
        assert_eq!(next_output_name(existing.iter()).as_str(), "1");

        let existing = vec![
            OutputName::from_static("1"),
            OutputName::from_static("Box 2"),
            OutputName::from_static("3"),
        ];
        assert_eq!(next_output_name(existing.iter()).as_str(), "2");
    }

    #[test]
    fn deserialization_validates() {
        assert!(serde_json::from_str::<OutputName>(r#""Box 5""#).is_ok());
        assert!(serde_json::from_str::<OutputName>(r#"" x""#).is_err());
        assert_eq!(
            serde_json::to_string(&OutputName::from_static("Box 5")).unwrap(),
            r#""Box 5""#
        );
    }

    #[test]
    fn round_trips_through_lp_value() {
        let name = OutputName::from_static("Box 5");
        assert_eq!(
            OutputName::from_lp_value(&name.to_lp_value()).unwrap(),
            name
        );
    }
}
