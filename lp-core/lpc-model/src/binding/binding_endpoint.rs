use super::{BusSlotRef, BusSlotRefError, NodeSlotRef, NodeSlotRefError};
use crate::{
    FieldSlot, FromLpValue, LpType, LpValue, SlotDataAccess, SlotMeta, SlotShape, SlotShapeId,
    SlotValue, SlotValueAccess, SlotValueShape, ToLpValue, ValueEditorHint, ValueRootError,
};
use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{MapAccess, Visitor},
    ser::SerializeMap,
};

/// Semantic endpoint for an authored slot binding.
///
/// TOML usually uses compact strings such as `bus#visual.out` and
/// `..shader#output`. Literal endpoints use an explicit `{ literal = ... }`
/// form because they are values, not address references.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum BindingEndpoint {
    Bus(BusSlotRef),
    Node(NodeSlotRef),
    Literal(LpValue),
}

impl BindingEndpoint {
    pub fn parse_ref(input: &str) -> Result<Self, BindingEndpointError> {
        if input.starts_with(BusSlotRef::PREFIX) {
            return BusSlotRef::parse(input)
                .map(Self::Bus)
                .map_err(BindingEndpointError::InvalidBus);
        }
        NodeSlotRef::parse(input)
            .map(Self::Node)
            .map_err(BindingEndpointError::InvalidNode)
    }

    pub fn is_literal(&self) -> bool {
        matches!(self, Self::Literal(_))
    }
}

impl SlotValueAccess for BindingEndpoint {
    fn changed_at(&self) -> crate::Revision {
        crate::current_revision()
    }

    fn value(&self) -> LpValue {
        self.to_lp_value()
    }
}

impl FieldSlot for BindingEndpoint {
    fn slot_field_shape() -> SlotShape {
        SlotShape::leaf(Self::value_shape())
    }

    fn slot_field_data(&self) -> SlotDataAccess<'_> {
        SlotDataAccess::Value(self)
    }
}

impl ToLpValue for BindingEndpoint {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.to_string())
    }
}

impl FromLpValue for BindingEndpoint {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::String(value) = value else {
            return Err(ValueRootError::new("expected binding endpoint string"));
        };
        Self::parse_ref(value).map_err(|error| ValueRootError::new(format!("{error}")))
    }
}

impl SlotValue for BindingEndpoint {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("BindingEndpoint");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::String,
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

impl fmt::Display for BindingEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bus(value) => write!(f, "{value}"),
            Self::Node(value) => write!(f, "{value}"),
            Self::Literal(value) => write!(f, "{value:?}"),
        }
    }
}

impl Serialize for BindingEndpoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Bus(value) => serializer.serialize_str(&value.to_string()),
            Self::Node(value) => serializer.serialize_str(&value.to_string()),
            Self::Literal(value) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("literal", value)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for BindingEndpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EndpointVisitor;

        impl<'de> Visitor<'de> for EndpointVisitor {
            type Value = BindingEndpoint;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a binding endpoint string or { literal = ... }")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                BindingEndpoint::parse_ref(value).map_err(E::custom)
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                self.visit_str(&value)
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut literal = None;
                while let Some(key) = access.next_key::<String>()? {
                    match key.as_str() {
                        "literal" => {
                            if literal.is_some() {
                                return Err(serde::de::Error::duplicate_field("literal"));
                            }
                            literal = Some(access.next_value::<LpValue>()?);
                        }
                        other => return Err(serde::de::Error::unknown_field(other, &["literal"])),
                    }
                }
                literal
                    .map(BindingEndpoint::Literal)
                    .ok_or_else(|| serde::de::Error::missing_field("literal"))
            }
        }

        deserializer.deserialize_any(EndpointVisitor)
    }
}

/// Error returned when parsing a compact binding endpoint ref.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingEndpointError {
    InvalidBus(BusSlotRefError),
    InvalidNode(NodeSlotRefError),
}

impl fmt::Display for BindingEndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBus(err) => write!(f, "{err}"),
            Self::InvalidNode(err) => write!(f, "{err}"),
        }
    }
}

impl core::error::Error for BindingEndpointError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bus_and_node_refs() {
        assert!(matches!(
            BindingEndpoint::parse_ref("bus#visual.out").unwrap(),
            BindingEndpoint::Bus(_)
        ));
        assert!(matches!(
            BindingEndpoint::parse_ref("..shader#output").unwrap(),
            BindingEndpoint::Node(_)
        ));
    }

    #[test]
    fn serializes_ref_endpoints_as_strings() {
        let endpoint = BindingEndpoint::parse_ref("bus#visual.out").unwrap();
        assert_eq!(
            serde_json::to_string(&endpoint).unwrap(),
            r#""bus#visual.out""#
        );
    }

    #[test]
    fn literal_endpoint_uses_explicit_literal_key() {
        let endpoint = BindingEndpoint::Literal(LpValue::F32(0.5));
        let json = serde_json::to_string(&endpoint).unwrap();
        assert_eq!(json, r#"{"literal":{"f32":0.5}}"#);
        let back: BindingEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, endpoint);
    }
}
