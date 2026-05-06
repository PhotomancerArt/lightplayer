use crate::{ModelType, SlotName, SlotNameError};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::SlotMeta;

/// Registry identity for a complete slot shape tree.
///
/// Shape IDs are stable names owned by the producer of a slot tree. They let
/// runtime data refer to one registered shape without embedding the whole shape
/// alongside every update.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeId(String);

impl SlotShapeId {
    pub fn parse(input: &str) -> Result<Self, SlotShapeIdError> {
        if input.is_empty() {
            return Err(SlotShapeIdError::Empty);
        }
        Ok(Self(input.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SlotShapeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for SlotShapeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SlotShapeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

/// Error returned when parsing a [`SlotShapeId`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlotShapeIdError {
    Empty,
}

impl fmt::Display for SlotShapeIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("slot shape id is empty"),
        }
    }
}

impl core::error::Error for SlotShapeIdError {}

/// Static shape of a slot tree.
///
/// A slot shape defines the authored and synchronized structure of slot data.
/// `Value` leaves are produced and versioned as complete units; container
/// shapes provide named or keyed structure around those leaves.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SlotShape {
    Value {
        #[serde(default)]
        meta: SlotMeta,
        ty: ModelType,
    },
    Record {
        #[serde(default)]
        meta: SlotMeta,
        fields: Vec<SlotFieldShape>,
    },
    Map {
        #[serde(default)]
        meta: SlotMeta,
        key: SlotMapKeyShape,
        value: Box<SlotShape>,
    },
    Enum {
        #[serde(default)]
        meta: SlotMeta,
        variants: Vec<SlotVariantShape>,
    },
    Option {
        #[serde(default)]
        meta: SlotMeta,
        some: Box<SlotShape>,
    },
}

impl SlotShape {
    /// Convenience constructor for a value leaf with empty metadata.
    pub fn value(ty: ModelType) -> Self {
        Self::Value {
            meta: SlotMeta::empty(),
            ty,
        }
    }
}

/// Key domain for a [`SlotShape::Map`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SlotMapKeyShape {
    String,
    I32,
    U32,
}

/// One named field inside a [`SlotShape::Record`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotFieldShape {
    pub name: SlotName,
    pub shape: SlotShape,
}

impl SlotFieldShape {
    pub fn new(name: &str, shape: SlotShape) -> Result<Self, SlotNameError> {
        Ok(Self {
            name: SlotName::parse(name)?,
            shape,
        })
    }
}

/// One tagged variant inside a [`SlotShape::Enum`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotVariantShape {
    pub name: SlotName,
    pub shape: SlotShape,
}

impl SlotVariantShape {
    pub fn new(name: &str, shape: SlotShape) -> Result<Self, SlotNameError> {
        Ok(Self {
            name: SlotName::parse(name)?,
            shape,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn slot_shape_id_serializes_as_string() {
        let id = SlotShapeId::parse("fixture.config").unwrap();
        assert_eq!(id.to_string(), "fixture.config");

        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, r#""fixture.config""#);

        let back: SlotShapeId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn slot_shape_id_rejects_empty_text() {
        assert_eq!(SlotShapeId::parse("").unwrap_err(), SlotShapeIdError::Empty);
    }

    #[test]
    fn nested_shapes_round_trip() {
        let shape = SlotShape::Record {
            meta: SlotMeta::empty(),
            fields: vec![
                SlotFieldShape::new("size", SlotShape::value(ModelType::Vec2)).unwrap(),
                SlotFieldShape::new(
                    "mapping",
                    SlotShape::Enum {
                        meta: SlotMeta::empty(),
                        variants: vec![
                            SlotVariantShape::new(
                                "shapes",
                                SlotShape::Map {
                                    meta: SlotMeta::empty(),
                                    key: SlotMapKeyShape::String,
                                    value: Box::new(SlotShape::Option {
                                        meta: SlotMeta::empty(),
                                        some: Box::new(SlotShape::value(ModelType::Vec4)),
                                    }),
                                },
                            )
                            .unwrap(),
                        ],
                    },
                )
                .unwrap(),
            ],
        };

        let json = serde_json::to_string(&shape).unwrap();
        let back: SlotShape = serde_json::from_str(&json).unwrap();
        assert_eq!(back, shape);
    }
}
