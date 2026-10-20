//! What a panel write holds: one value, or a set keyed by `u32`.

use alloc::vec::Vec;

use crate::LpValue;

/// The value a panel writer holds (panel.md P8).
///
/// Most controls hold one leaf [`LpValue`]. A **map** is a set of leaves keyed
/// by `u32` — a touch set (`touches`, [`crate::Touch`] by pointer id) — and
/// materializes on the bus as `SlotData::Map`, so a by-key or sentinel-map
/// reader sees it the way it sees any produced map.
///
/// A map rides **momentary** writes only (a `ttl_ms` is set): it is gesture
/// state, never persisted, and `.lp/panel.json` stays leaf-only. `LpValue`
/// gets no Map variant for the same reason: it is a persisted vocabulary.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelWriteValue {
    /// One leaf value.
    Value(LpValue),
    /// `key → leaf` entries, in the order written. Keys are unique.
    Map(Vec<(u32, LpValue)>),
}

impl PanelWriteValue {
    /// The leaf, when this is one.
    pub fn as_value(&self) -> Option<&LpValue> {
        match self {
            Self::Value(value) => Some(value),
            Self::Map(_) => None,
        }
    }

    /// Whether this is a map.
    pub fn is_map(&self) -> bool {
        matches!(self, Self::Map(_))
    }
}

impl From<LpValue> for PanelWriteValue {
    fn from(value: LpValue) -> Self {
        Self::Value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ToLpValue, Touch};
    use alloc::vec;

    #[test]
    fn both_forms_round_trip_through_json() {
        for value in [
            PanelWriteValue::Value(LpValue::F32(0.5)),
            PanelWriteValue::Map(vec![
                (1, Touch::new(1, [0.0, 0.5]).to_lp_value()),
                (2, Touch::new(2, [-1.0, 0.25]).to_lp_value()),
            ]),
            PanelWriteValue::Map(vec![]),
        ] {
            let json = serde_json::to_string(&value).unwrap();
            let back: PanelWriteValue = serde_json::from_str(&json).unwrap();
            assert_eq!(back, value);
        }
    }
}
