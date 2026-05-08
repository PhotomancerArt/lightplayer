use crate::{SlotPath, SlotPathError};
use alloc::string::{String, ToString};
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Parsed reference to a slot on the project bus.
///
/// The authored form uses the same owner/slot separator as node-slot refs:
///
/// ```text
/// bus#visual.out
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct BusSlotRef {
    slot: SlotPath,
}

impl BusSlotRef {
    pub const PREFIX: &'static str = "bus#";

    pub fn new(slot: SlotPath) -> Self {
        Self { slot }
    }

    pub fn parse(input: &str) -> Result<Self, BusSlotRefError> {
        let Some(slot) = input.strip_prefix(Self::PREFIX) else {
            return Err(BusSlotRefError::MissingPrefix);
        };
        if slot.is_empty() {
            return Err(BusSlotRefError::MissingSlot);
        }
        Ok(Self {
            slot: SlotPath::parse(slot).map_err(BusSlotRefError::InvalidSlot)?,
        })
    }

    pub fn slot(&self) -> &SlotPath {
        &self.slot
    }
}

impl fmt::Display for BusSlotRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", Self::PREFIX, self.slot)
    }
}

impl Serialize for BusSlotRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for BusSlotRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

/// Error returned when parsing a [`BusSlotRef`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusSlotRefError {
    MissingPrefix,
    MissingSlot,
    InvalidSlot(SlotPathError),
}

impl fmt::Display for BusSlotRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefix => f.write_str("bus slot ref must start with `bus#`"),
            Self::MissingSlot => f.write_str("bus slot ref is missing a slot path"),
            Self::InvalidSlot(err) => write!(f, "invalid bus slot path: {err}"),
        }
    }
}

impl core::error::Error for BusSlotRefError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn parses_and_round_trips_bus_slot_refs() {
        let parsed = BusSlotRef::parse("bus#visual.out").unwrap();
        assert_eq!(parsed.slot().to_string(), "visual.out");
        assert_eq!(parsed.to_string(), "bus#visual.out");

        let json = serde_json::to_string(&parsed).unwrap();
        assert_eq!(json, r#""bus#visual.out""#);
        let back: BusSlotRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, parsed);
    }

    #[test]
    fn rejects_missing_bus_prefix() {
        assert_eq!(
            BusSlotRef::parse("visual.out"),
            Err(BusSlotRefError::MissingPrefix)
        );
    }
}
