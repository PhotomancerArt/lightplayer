use super::{BusSlotRef, BusSlotRefError, NodeSlotRef, NodeSlotRefError};
use crate::LpValue;
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
                            literal = Some(access.next_value::<AuthoredLiteral>()?.0);
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

struct AuthoredLiteral(LpValue);

impl<'de> Deserialize<'de> for AuthoredLiteral {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(AuthoredLiteralVisitor)
    }
}

struct AuthoredLiteralVisitor;

impl<'de> Visitor<'de> for AuthoredLiteralVisitor {
    type Value = AuthoredLiteral;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a scalar literal or tagged scalar LpValue")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(AuthoredLiteral(LpValue::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let value = i32::try_from(value)
            .map_err(|_| E::custom(format!("literal integer {value} does not fit i32")))?;
        Ok(AuthoredLiteral(LpValue::I32(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let value = u32::try_from(value)
            .map_err(|_| E::custom(format!("literal integer {value} does not fit u32")))?;
        Ok(AuthoredLiteral(LpValue::U32(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(AuthoredLiteral(LpValue::F32(value as f32)))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(AuthoredLiteral(LpValue::String(String::from(value))))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(AuthoredLiteral(LpValue::String(value)))
    }

    fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let Some(key) = access.next_key::<String>()? else {
            return Err(serde::de::Error::custom("literal map cannot be empty"));
        };
        let value = match key.as_str() {
            "string" => LpValue::String(access.next_value()?),
            "i32" => LpValue::I32(access.next_value()?),
            "u32" => LpValue::U32(access.next_value()?),
            "f32" => LpValue::F32(access.next_value()?),
            "bool" => LpValue::Bool(access.next_value()?),
            other => {
                return Err(serde::de::Error::unknown_field(
                    other,
                    &["string", "i32", "u32", "f32", "bool"],
                ));
            }
        };

        if access.next_key::<String>()?.is_some() {
            return Err(serde::de::Error::custom(
                "tagged literal map must have exactly one field",
            ));
        }

        Ok(AuthoredLiteral(value))
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

    #[test]
    fn literal_endpoint_loads_scalar_toml_without_full_lp_value_shape() {
        #[derive(serde::Deserialize)]
        struct Wrapper {
            source: BindingEndpoint,
        }

        let back: Wrapper = toml::from_str(
            r#"
[source]
literal = 0.5
"#,
        )
        .unwrap();

        assert_eq!(back.source, BindingEndpoint::Literal(LpValue::F32(0.5)));
    }
}
