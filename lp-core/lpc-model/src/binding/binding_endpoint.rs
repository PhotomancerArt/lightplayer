use super::{BusSlotRef, BusSlotRefError, NodeSlotRef, NodeSlotRefError};
use crate::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, StaticLpType,
    StaticSlotMeta, StaticSlotValueShape, StaticValueEditorHint, ToLpValue, ValueEditorHint,
    ValueRootError,
};
use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Authored reference to a binding endpoint.
///
/// The string form is the canonical slot value representation:
///
/// ```text
/// bus#visual.out
/// ..shader#output
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum BindingRef {
    Unset,
    Bus(BusSlotRef),
    Node(NodeSlotRef),
}

impl BindingRef {
    pub fn parse(input: &str) -> Result<Self, BindingRefError> {
        if input.is_empty() {
            return Ok(Self::Unset);
        }
        if input.starts_with(BusSlotRef::PREFIX) {
            return BusSlotRef::parse(input)
                .map(Self::Bus)
                .map_err(BindingRefError::InvalidBus);
        }
        NodeSlotRef::parse(input)
            .map(Self::Node)
            .map_err(BindingRefError::InvalidNode)
    }

    pub fn is_unset(&self) -> bool {
        matches!(self, Self::Unset)
    }
}

impl Default for BindingRef {
    fn default() -> Self {
        Self::Unset
    }
}

impl ToLpValue for BindingRef {
    fn to_lp_value(&self) -> LpValue {
        match self {
            Self::Unset => LpValue::Unset,
            Self::Bus(_) | Self::Node(_) => LpValue::String(self.to_string()),
        }
    }
}

impl FromLpValue for BindingRef {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        if matches!(value, LpValue::Unset) {
            return Ok(Self::Unset);
        }
        let LpValue::String(value) = value else {
            return Err(ValueRootError::new("expected binding ref string"));
        };
        if value.is_empty() {
            return Ok(Self::Unset);
        }
        Self::parse(value).map_err(|error| ValueRootError::new(format!("{error}")))
    }
}

impl SlotValue for BindingRef {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("BindingRef");
    const STATIC_VALUE_SHAPE_DESCRIPTOR: Option<StaticSlotValueShape> =
        Some(StaticSlotValueShape {
            id: Self::SHAPE_ID,
            ty: StaticLpType::String,
            meta: StaticSlotMeta::EMPTY,
            editor: StaticValueEditorHint::Path,
        });

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Path,
        }
    }
}

impl fmt::Display for BindingRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unset => f.write_str(""),
            Self::Bus(value) => write!(f, "{value}"),
            Self::Node(value) => write!(f, "{value}"),
        }
    }
}

impl Serialize for BindingRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if matches!(self, Self::Unset) {
            return serializer.serialize_str("");
        }
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for BindingRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        if input.is_empty() {
            return Ok(Self::Unset);
        }
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

/// Error returned when parsing a [`BindingRef`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingRefError {
    InvalidBus(BusSlotRefError),
    InvalidNode(NodeSlotRefError),
}

impl fmt::Display for BindingRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBus(err) => write!(f, "invalid bus binding ref: {err}"),
            Self::InvalidNode(err) => write!(f, "invalid node binding ref: {err}"),
        }
    }
}

impl core::error::Error for BindingRefError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn parses_refs() {
        assert_eq!(BindingRef::parse("").unwrap(), BindingRef::Unset);
        assert!(matches!(
            BindingRef::parse("bus#visual.out").unwrap(),
            BindingRef::Bus(_)
        ));
        assert!(matches!(
            BindingRef::parse("..shader#output").unwrap(),
            BindingRef::Node(_)
        ));
    }

    #[test]
    fn round_trips_through_lp_value_as_string() {
        let binding_ref = BindingRef::parse("bus#visual.out").unwrap();

        assert_eq!(
            BindingRef::from_lp_value(&binding_ref.to_lp_value()).unwrap(),
            binding_ref
        );
        assert_eq!(
            binding_ref.to_lp_value(),
            LpValue::String(binding_ref.to_string())
        );
        assert_eq!(
            BindingRef::from_lp_value(&LpValue::Unset).unwrap(),
            BindingRef::Unset
        );
    }

    #[test]
    fn serde_uses_string_form() {
        let binding_ref = BindingRef::parse("bus#visual.out").unwrap();

        let json = serde_json::to_string(&binding_ref).unwrap();

        assert_eq!(json, r#""bus#visual.out""#);
        assert_eq!(
            serde_json::from_str::<BindingRef>(&json).unwrap(),
            binding_ref
        );
    }
}
