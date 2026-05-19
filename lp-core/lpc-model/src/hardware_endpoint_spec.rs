use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    FromLpValue, LpType, LpValue, SlotMeta, SlotShapeId, SlotValue, SlotValueShape, ToLpValue,
    ValueEditorHint, ValueRootError,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct HardwareEndpointSpec(String);

impl HardwareEndpointSpec {
    pub fn parse(spec: impl Into<String>) -> Result<Self, HardwareEndpointSpecError> {
        let spec = spec.into();
        validate_spec(&spec)?;
        Ok(Self(spec))
    }

    pub fn from_static(spec: &'static str) -> Self {
        Self::parse(spec).expect("static hardware endpoint spec must be valid")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn capability(&self) -> &str {
        spec_parts(&self.0).0
    }

    pub fn driver(&self) -> &str {
        spec_parts(&self.0).1
    }

    pub fn config(&self) -> &str {
        spec_parts(&self.0).2
    }
}

impl Default for HardwareEndpointSpec {
    fn default() -> Self {
        Self::from_static("unset:unset:unset")
    }
}

impl fmt::Display for HardwareEndpointSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for HardwareEndpointSpec {
    type Err = HardwareEndpointSpecError;

    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        Self::parse(spec)
    }
}

impl Serialize for HardwareEndpointSpec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for HardwareEndpointSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let spec = String::deserialize(deserializer)?;
        Self::parse(spec).map_err(serde::de::Error::custom)
    }
}

impl ToLpValue for HardwareEndpointSpec {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.0.clone())
    }
}

impl FromLpValue for HardwareEndpointSpec {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => {
                Self::parse(value.clone()).map_err(|error| ValueRootError::new(error.to_string()))
            }
            other => Err(ValueRootError::new(format!(
                "expected HardwareEndpointSpec string, got {other:?}"
            ))),
        }
    }
}

impl SlotValue for HardwareEndpointSpec {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("HardwareEndpointSpec");

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
pub enum HardwareEndpointSpecError {
    WrongPartCount { spec: String },
    EmptyPart { spec: String },
    NonAscii { spec: String },
}

impl fmt::Display for HardwareEndpointSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongPartCount { spec } => write!(
                f,
                "hardware endpoint spec must be cap:driver:config, got `{spec}`"
            ),
            Self::EmptyPart { spec } => {
                write!(f, "hardware endpoint spec contains an empty part: `{spec}`")
            }
            Self::NonAscii { spec } => {
                write!(f, "hardware endpoint spec must be ASCII: `{spec}`")
            }
        }
    }
}

impl core::error::Error for HardwareEndpointSpecError {}

fn validate_spec(spec: &str) -> Result<(), HardwareEndpointSpecError> {
    if !spec.is_ascii() {
        return Err(HardwareEndpointSpecError::NonAscii {
            spec: spec.to_string(),
        });
    }
    let mut parts = spec.split(':');
    let Some(capability) = parts.next() else {
        return Err(HardwareEndpointSpecError::WrongPartCount {
            spec: spec.to_string(),
        });
    };
    let Some(driver) = parts.next() else {
        return Err(HardwareEndpointSpecError::WrongPartCount {
            spec: spec.to_string(),
        });
    };
    let Some(config) = parts.next() else {
        return Err(HardwareEndpointSpecError::WrongPartCount {
            spec: spec.to_string(),
        });
    };
    if parts.next().is_some() {
        return Err(HardwareEndpointSpecError::WrongPartCount {
            spec: spec.to_string(),
        });
    }
    if capability.is_empty() || driver.is_empty() || config.is_empty() {
        return Err(HardwareEndpointSpecError::EmptyPart {
            spec: spec.to_string(),
        });
    }
    Ok(())
}

fn spec_parts(spec: &str) -> (&str, &str, &str) {
    let mut parts = spec.split(':');
    (
        parts.next().expect("validated spec has capability"),
        parts.next().expect("validated spec has driver"),
        parts.next().expect("validated spec has config"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FromLpValue, ToLpValue};

    #[test]
    fn endpoint_spec_splits_three_parts() {
        let spec = HardwareEndpointSpec::parse("ws281x:rmt:D10").unwrap();

        assert_eq!(spec.capability(), "ws281x");
        assert_eq!(spec.driver(), "rmt");
        assert_eq!(spec.config(), "D10");
        assert_eq!(spec.as_str(), "ws281x:rmt:D10");
    }

    #[test]
    fn endpoint_spec_rejects_malformed_values() {
        for spec in ["ws281x:rmt", "ws281x:rmt:", ":rmt:D10", "ws281x:rmt:D10:x"] {
            assert!(
                HardwareEndpointSpec::parse(spec).is_err(),
                "{spec} should be rejected"
            );
        }
    }

    #[test]
    fn endpoint_spec_round_trips_through_lp_value() {
        let spec = HardwareEndpointSpec::parse("ws281x:rmt:D10").unwrap();

        assert_eq!(
            HardwareEndpointSpec::from_lp_value(&spec.to_lp_value()).unwrap(),
            spec
        );
    }
}
