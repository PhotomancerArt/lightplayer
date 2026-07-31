//! The kind of lamp a fixture drives.
//!
//! A lamp type is a *name*; its electrical behaviour lives in
//! [`super::lamp_presets`]. Keeping the two apart means a project stores only
//! the name, so corrected numbers reach existing projects without touching
//! their files — and a project cannot author a bogus power model.

use alloc::string::ToString;
use serde::{Deserialize, Serialize};

use crate::{FromLpValue, LpValue, ToLpValue, ValueRootError};

/// A lamp part family with known power behaviour.
///
/// Deliberately short. Every entry here is one somebody has actually wired up;
/// see [`super::lamp_presets`] for how confident we are in its numbers.
/// Variant names are pinned explicitly rather than derived: serde's
/// `snake_case` rule only breaks on case changes, so `Ws281112v` would encode
/// as `ws281112v` while [`LampType::as_str`] says `ws2811_12v`, and the two
/// encodings of the same value would silently disagree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum LampType {
    /// 5V WS2812B / SK6812 and compatible per-pixel parts. The default.
    #[default]
    #[serde(rename = "ws2812b_5v")]
    Ws2812b5v,
    /// 12V WS2815 per-pixel parts with constant-current drivers.
    #[serde(rename = "ws2815_12v")]
    Ws281512v,
    /// 12V WS2811 strips: one addressable chip drives three LEDs in series.
    #[serde(rename = "ws2811_12v")]
    Ws281112v,
}

impl LampType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ws2812b5v => "ws2812b_5v",
            Self::Ws281512v => "ws2815_12v",
            Self::Ws281112v => "ws2811_12v",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ws2812b_5v" => Some(Self::Ws2812b5v),
            "ws2815_12v" => Some(Self::Ws281512v),
            "ws2811_12v" => Some(Self::Ws281112v),
            _ => None,
        }
    }

    /// Human-readable name for pickers and readouts.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Ws2812b5v => "WS2812B (5V)",
            Self::Ws281512v => "WS2815 (12V)",
            Self::Ws281112v => "WS2811 (12V)",
        }
    }

    /// Every lamp type, for building pickers.
    pub const ALL: &'static [Self] = &[Self::Ws2812b5v, Self::Ws281512v, Self::Ws281112v];
}

impl ToLpValue for LampType {
    fn to_lp_value(&self) -> LpValue {
        LpValue::String(self.as_str().to_string())
    }
}

impl FromLpValue for LampType {
    fn from_lp_value(value: &LpValue) -> Result<Self, ValueRootError> {
        match value {
            LpValue::String(value) => {
                Self::parse(value).ok_or_else(|| ValueRootError::new("expected lamp type"))
            }
            other => Err(ValueRootError::new(alloc::format!(
                "expected String, got {other:?}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for lamp in LampType::ALL {
            assert_eq!(
                LampType::parse(lamp.as_str()),
                Some(*lamp),
                "{} should round-trip",
                lamp.as_str()
            );
        }
    }

    #[test]
    fn unknown_name_is_rejected_not_defaulted() {
        assert_eq!(LampType::parse("ws2812b"), None);
        let err = LampType::from_lp_value(&LpValue::String("nonsense".to_string()));
        assert!(err.is_err(), "an unknown lamp must not silently default");
    }

    #[test]
    fn lp_value_round_trips() {
        let value = LampType::Ws281512v.to_lp_value();
        assert_eq!(value, LpValue::String("ws2815_12v".to_string()));
        assert_eq!(
            LampType::from_lp_value(&value).expect("decodes"),
            LampType::Ws281512v
        );
    }

    /// The serde form and `as_str` are two encodings of one value; if they
    /// drift, a lamp written by one path is unreadable by the other.
    #[test]
    fn serde_and_as_str_agree_for_every_variant() {
        for lamp in LampType::ALL {
            let json = serde_json::to_string(lamp).expect("encodes");
            assert_eq!(
                json,
                alloc::format!("\"{}\"", lamp.as_str()),
                "serde name must match as_str for {lamp:?}"
            );
            assert_eq!(
                serde_json::from_str::<LampType>(&json).expect("decodes"),
                *lamp
            );
        }
    }
}
