use crate::{LpType, SlotName, SlotNameError, SlotPolicy, SlotSemantics, SlotValueShape};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use super::{SlotMeta, stable_hash::fnv1a_32};

/// Static shape of a slot tree.
///
/// A slot shape defines the authored and synchronized structure of slot data.
/// `Value` leaves are produced and versioned as complete units; container
/// shapes provide named or keyed structure around those leaves.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
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
        #[serde(default, skip_serializing_if = "SlotEnumEncoding::is_default")]
        encoding: SlotEnumEncoding,
        variants: Vec<SlotVariantShape>,
    },
    Option {
        #[serde(default)]
        meta: SlotMeta,
        some: Box<SlotShape>,
    },
    Custom {
        #[serde(default)]
        meta: SlotMeta,
        codec: SlotShapeId,
        shape: Box<SlotShape>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        refs: Vec<SlotShapeId>,
    },
}

/// Compact registry identity for a slot shape node.
///
/// Static Rust-authored shapes should define this as a type-level constant,
/// usually with [`SlotShapeId::from_static_name`]. The registry rejects duplicate
/// ids at registration time, so static hash collisions fail during startup
/// shape registration instead of becoming ambiguous runtime lookups.
///
/// Name-based ids are 32-bit FNV-1a hashes. This is a small, stable,
/// non-cryptographic hash suitable for compact ids over trusted static shape
/// names. It is not collision-proof; the registry remains responsible for
/// detecting duplicate ids when shapes are registered.
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
        Self(fnv1a_32(input))
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

/// Authored syntax used when reading and writing an enum slot.
///
/// Encoding changes only the source/document representation of an enum. The
/// runtime data model is always one active variant plus that variant's payload.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SlotEnumEncoding {
    /// Store the active variant in a discriminator field, such as
    /// `kind = "Variant"`, with the payload flattened beside it.
    Tagged { field: SlotName },
    /// Store the active variant as the single property of the enum object.
    External,
}

impl SlotEnumEncoding {
    pub fn tagged_kind() -> Self {
        Self::Tagged {
            field: SlotName::parse("kind").expect("valid static slot name"),
        }
    }

    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

impl Default for SlotEnumEncoding {
    fn default() -> Self {
        Self::tagged_kind()
    }
}

impl SlotShape {
    /// Reference another registered shape.
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

    /// Collect registered shape ids referenced by this shape tree.
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
            Self::Custom {
                shape,
                refs: custom_refs,
                ..
            } => {
                shape.collect_referenced_shape_ids(refs);
                refs.extend(custom_refs.iter().copied());
            }
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
    #[serde(default, skip_serializing_if = "SlotSemantics::is_default")]
    pub semantics: SlotSemantics,
    #[serde(default, skip_serializing_if = "SlotPolicy::is_default")]
    pub policy: SlotPolicy,
    /// Declarative default binding endpoint (`bus:<channel>`) materialized at
    /// load when no authored binding names this slot — a produced slot's
    /// default publishes, a consumed slot's default sources (ADR
    /// 2026-07-09-declarative-default-bindings).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_bind: Option<String>,
}

impl SlotFieldShape {
    pub fn new(name: &str, shape: SlotShape) -> Result<Self, SlotNameError> {
        Self::with_semantics(name, shape, SlotSemantics::default())
    }

    pub fn with_policy(
        name: &str,
        shape: SlotShape,
        policy: SlotPolicy,
    ) -> Result<Self, SlotNameError> {
        Self::with_semantics_and_policy(name, shape, SlotSemantics::default(), policy)
    }

    pub fn with_semantics(
        name: &str,
        shape: SlotShape,
        semantics: SlotSemantics,
    ) -> Result<Self, SlotNameError> {
        Self::with_semantics_and_policy(name, shape, semantics, SlotPolicy::default())
    }

    pub fn with_semantics_and_policy(
        name: &str,
        shape: SlotShape,
        semantics: SlotSemantics,
        policy: SlotPolicy,
    ) -> Result<Self, SlotNameError> {
        Ok(Self {
            name: SlotName::parse(name)?,
            shape,
            semantics,
            policy,
            default_bind: None,
        })
    }
}

impl SlotSemantics {
    pub fn is_default(self: &Self) -> bool {
        *self == Self::default()
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
                        encoding: SlotEnumEncoding::default(),
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

    #[test]
    fn enum_encoding_defaults_to_tagged_kind() {
        let json = r#"{"enum":{"variants":[]}}"#;

        let shape: SlotShape = serde_json::from_str(json).unwrap();

        let SlotShape::Enum { encoding, .. } = shape else {
            panic!("expected enum shape");
        };
        assert_eq!(encoding, SlotEnumEncoding::tagged_kind());
    }
}
