use crate::{LpType, SlotName, SlotNameError, SlotValueShape};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use super::SlotMeta;


/// Static shape of a slot tree.
///
/// A slot shape defines the authored and synchronized structure of slot data.
/// `Value` leaves are produced and versioned as complete units; container
/// shapes provide named or keyed structure around those leaves.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SlotShape {
    Ref {
        id: SlotShapeId,
    },
    Unit {
        #[serde(default)]
        meta: SlotMeta,
    },
    Value {
        shape: SlotValueShape,
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

/// Compact registry identity for a slot shape node.
///
/// Static Rust-authored shapes should define this as a type-level constant,
/// usually with [`SlotShapeId::from_static_name`]. The registry rejects duplicate
/// ids at registration time, so static hash collisions fail during startup
/// shape registration instead of becoming ambiguous runtime lookups.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotShapeId(u32);

impl SlotShapeId {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn from_static_name(input: &str) -> Self {
        Self(fnv1a32(input))
    }

    pub fn from_name(input: &str) -> Result<Self, SlotShapeIdError> {
        Self::parse(input)
    }

    pub fn parse(input: &str) -> Result<Self, SlotShapeIdError> {
        if input.is_empty() {
            return Err(SlotShapeIdError::Empty);
        }
        Ok(Self::from_static_name(input))
    }

    pub fn raw(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SlotShapeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08x}", self.0)
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

const fn fnv1a32(input: &str) -> u32 {
    const OFFSET: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;

    let bytes = input.as_bytes();
    let mut hash = OFFSET;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u32;
        hash = hash.wrapping_mul(PRIME);
        index += 1;
    }
    hash
}

impl SlotShape {
    /// Reference another registered root shape.
    pub fn reference(id: SlotShapeId) -> Self {
        Self::Ref { id }
    }

    /// Convenience constructor for a payload-free unit slot with empty metadata.
    pub fn unit() -> Self {
        Self::Unit {
            meta: SlotMeta::empty(),
        }
    }

    /// Convenience constructor for a value leaf with empty metadata.
    pub fn value(ty: LpType) -> Self {
        Self::Value {
            shape: SlotValueShape::raw(ty),
        }
    }

    /// Convenience constructor for a semantic value leaf descriptor.
    pub fn leaf(shape: SlotValueShape) -> Self {
        Self::Value { shape }
    }

    /// Collect root shape ids referenced by this shape tree.
    ///
    /// The returned ids are not de-duplicated. Callers that care about unique
    /// ids can collect into a set; preserving traversal order keeps this helper
    /// simple and predictable for generated bootstrap code.
    pub fn referenced_shape_ids(&self) -> Vec<SlotShapeId> {
        let mut refs = Vec::new();
        self.collect_referenced_shape_ids(&mut refs);
        refs
    }

    fn collect_referenced_shape_ids(&self, refs: &mut Vec<SlotShapeId>) {
        match self {
            Self::Ref { id } => refs.push(*id),
            Self::Unit { .. } | Self::Value { .. } => {}
            Self::Record { fields, .. } => {
                for field in fields {
                    field.shape.collect_referenced_shape_ids(refs);
                }
            }
            Self::Map { value, .. } => value.collect_referenced_shape_ids(refs),
            Self::Enum { variants, .. } => {
                for variant in variants {
                    variant.shape.collect_referenced_shape_ids(refs);
                }
            }
            Self::Option { some, .. } => some.collect_referenced_shape_ids(refs),
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
    use alloc::format;
    use alloc::vec;

    #[test]
    fn slot_shape_id_serializes_as_compact_integer() {
        let id = SlotShapeId::parse("fixture.config").unwrap();
        assert_eq!(id, SlotShapeId::from_static_name("fixture.config"));
        assert_ne!(id.raw(), 0);

        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("{}", id.raw()));

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
                SlotFieldShape::new("size", SlotShape::value(LpType::Vec2)).unwrap(),
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
                                        some: Box::new(SlotShape::value(LpType::Vec4)),
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
