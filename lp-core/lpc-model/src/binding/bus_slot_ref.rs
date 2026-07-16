use crate::ChannelName;
use alloc::string::{String, ToString};
use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Parsed reference to a channel on the project bus.
///
/// URI-style authored form: the `bus:` scheme followed by the channel name
/// (`purpose[.in|.out][/instance]` — see [`ChannelName`] for the naming
/// convention):
///
/// ```text
/// bus:visual.out
/// bus:time
/// bus:visual.out/left
/// ```
///
/// `#` is reserved in bus refs for a future "field within the channel's
/// value" fragment (`bus:pose#head.x`) and is rejected today.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct BusSlotRef {
    channel: ChannelName,
}

impl BusSlotRef {
    pub const PREFIX: &'static str = "bus:";

    pub fn new(channel: ChannelName) -> Self {
        Self { channel }
    }

    pub fn parse(input: &str) -> Result<Self, BusSlotRefError> {
        let Some(channel) = input.strip_prefix(Self::PREFIX) else {
            return Err(BusSlotRefError::MissingPrefix);
        };
        if channel.is_empty() {
            return Err(BusSlotRefError::MissingChannel);
        }
        if channel.contains('#') {
            return Err(BusSlotRefError::ReservedFragment);
        }
        Ok(Self {
            channel: ChannelName(String::from(channel)),
        })
    }

    pub fn channel(&self) -> &ChannelName {
        &self.channel
    }
}

impl fmt::Display for BusSlotRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", Self::PREFIX, self.channel)
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
    MissingChannel,
    ReservedFragment,
}

impl fmt::Display for BusSlotRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefix => f.write_str("bus ref must start with `bus:`"),
            Self::MissingChannel => f.write_str("bus ref is missing a channel name"),
            Self::ReservedFragment => {
                f.write_str("`#` is reserved in bus refs (future field-within-channel syntax)")
            }
        }
    }
}

impl core::error::Error for BusSlotRefError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn parses_and_round_trips_bus_refs() {
        let parsed = BusSlotRef::parse("bus:visual.out").unwrap();
        assert_eq!(parsed.channel().to_string(), "visual.out");
        assert_eq!(parsed.to_string(), "bus:visual.out");

        let json = serde_json::to_string(&parsed).unwrap();
        assert_eq!(json, r#""bus:visual.out""#);
        let back: BusSlotRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back, parsed);
    }

    #[test]
    fn parses_instance_channels() {
        let parsed = BusSlotRef::parse("bus:visual.out/left").unwrap();
        assert_eq!(parsed.channel().to_string(), "visual.out/left");
        assert_eq!(parsed.to_string(), "bus:visual.out/left");
    }

    #[test]
    fn rejects_missing_prefix_and_channel() {
        assert_eq!(
            BusSlotRef::parse("bus#visual.out"),
            Err(BusSlotRefError::MissingPrefix)
        );
        assert_eq!(
            BusSlotRef::parse("visual.out"),
            Err(BusSlotRefError::MissingPrefix)
        );
        assert_eq!(
            BusSlotRef::parse("bus:"),
            Err(BusSlotRefError::MissingChannel)
        );
    }

    #[test]
    fn rejects_reserved_fragment() {
        assert_eq!(
            BusSlotRef::parse("bus:pose#head.x"),
            Err(BusSlotRefError::ReservedFragment)
        );
    }
}
