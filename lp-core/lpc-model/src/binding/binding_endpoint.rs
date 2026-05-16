use super::{BusSlotRef, BusSlotRefError, NodeSlotRef, NodeSlotRefError};
use crate::{
    FieldSlot, FromLpValue, LpType, LpValue, ModelEnumVariant, SlotDataAccess, SlotMeta, SlotShape,
    SlotShapeId, SlotValue, SlotValueAccess, SlotValueShape, ToLpValue, ValueEditorHint,
    ValueRootError,
};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
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
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum BindingEndpoint {
    #[default]
    Unset,
    Bus(BusSlotRef),
    Node(NodeSlotRef),
    Literal(LpValue),
}

impl BindingEndpoint {
    pub fn parse_ref(input: &str) -> Result<Self, BindingEndpointError> {
        if input.is_empty() {
            return Ok(Self::Unset);
        }
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

    pub fn is_unset(&self) -> bool {
        matches!(self, Self::Unset)
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
        match self {
            Self::Unset => LpValue::Enum {
                variant: ENDPOINT_UNSET,
                payload: None,
            },
            Self::Bus(value) => LpValue::Enum {
                variant: ENDPOINT_BUS,
                payload: Some(Box::new(LpValue::String(value.to_string()))),
            },
            Self::Node(value) => LpValue::Enum {
                variant: ENDPOINT_NODE,
                payload: Some(Box::new(LpValue::String(value.to_string()))),
            },
            Self::Literal(value) => LpValue::Enum {
                variant: ENDPOINT_LITERAL,
                payload: Some(Box::new(value.clone())),
            },
        }
    }
}

impl FromLpValue for BindingEndpoint {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        let LpValue::Enum { variant, payload } = value else {
            return Err(ValueRootError::new("expected binding endpoint enum"));
        };
        match (*variant, payload.as_deref()) {
            (ENDPOINT_UNSET, None) => Ok(Self::Unset),
            (ENDPOINT_BUS, Some(LpValue::String(value))) => BusSlotRef::parse(value)
                .map(Self::Bus)
                .map_err(|error| ValueRootError::new(format!("{error}"))),
            (ENDPOINT_NODE, Some(LpValue::String(value))) => NodeSlotRef::parse(value)
                .map(Self::Node)
                .map_err(|error| ValueRootError::new(format!("{error}"))),
            (ENDPOINT_LITERAL, Some(value)) => Ok(Self::Literal(value.clone())),
            (ENDPOINT_UNSET, Some(_)) => Err(ValueRootError::new(
                "binding endpoint Unset variant does not accept a payload",
            )),
            (ENDPOINT_BUS | ENDPOINT_NODE, _) => Err(ValueRootError::new(
                "binding endpoint ref variants require a string payload",
            )),
            (ENDPOINT_LITERAL, None) => Err(ValueRootError::new(
                "binding endpoint Literal variant requires a payload",
            )),
            _ => Err(ValueRootError::new(format!(
                "unknown binding endpoint variant {variant}"
            ))),
        }
    }
}

impl SlotValue for BindingEndpoint {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("BindingEndpoint");

    fn value_shape() -> SlotValueShape {
        SlotValueShape {
            id: Self::SHAPE_ID,
            ty: LpType::Enum {
                name: Some(String::from("BindingEndpoint")),
                variants: vec![
                    ModelEnumVariant {
                        name: String::from("Unset"),
                        payload: None,
                    },
                    ModelEnumVariant {
                        name: String::from("Bus"),
                        payload: Some(LpType::String),
                    },
                    ModelEnumVariant {
                        name: String::from("Node"),
                        payload: Some(LpType::String),
                    },
                    ModelEnumVariant {
                        name: String::from("Literal"),
                        payload: Some(LpType::Any),
                    },
                ],
            },
            meta: SlotMeta::empty(),
            editor: ValueEditorHint::Plain,
        }
    }
}

const ENDPOINT_UNSET: u32 = 0;
const ENDPOINT_BUS: u32 = 1;
const ENDPOINT_NODE: u32 = 2;
const ENDPOINT_LITERAL: u32 = 3;

impl fmt::Display for BindingEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unset => Ok(()),
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
            Self::Unset => serializer.serialize_str(""),
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
                Ok(literal.map(BindingEndpoint::Literal).unwrap_or_default())
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
    fn empty_ref_is_unset_sentinel() {
        assert_eq!(BindingEndpoint::default(), BindingEndpoint::Unset);
        assert_eq!(
            BindingEndpoint::parse_ref("").unwrap(),
            BindingEndpoint::Unset
        );
        assert_eq!(
            BindingEndpoint::from_lp_value(&BindingEndpoint::Unset.to_lp_value()).unwrap(),
            BindingEndpoint::Unset
        );
    }

    #[test]
    fn endpoint_round_trips_through_enum_lp_value() {
        for endpoint in [
            BindingEndpoint::Unset,
            BindingEndpoint::parse_ref("bus#visual.out").unwrap(),
            BindingEndpoint::parse_ref("..shader#output").unwrap(),
            BindingEndpoint::Literal(LpValue::F32(0.5)),
        ] {
            let value = endpoint.to_lp_value();
            assert_eq!(BindingEndpoint::from_lp_value(&value).unwrap(), endpoint);
        }
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
