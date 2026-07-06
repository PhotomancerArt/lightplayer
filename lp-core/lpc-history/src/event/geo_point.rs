//! Optional location attached to push events.

use alloc::string::String;
use serde::{Deserialize, Serialize};

/// A location, recorded on push events when the user opts in.
///
/// Model only: nothing in this crate produces one. The label is
/// user-editable free text (client-only product — no geocoding service).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
    pub label: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn serde_round_trip() {
        let point = GeoPoint {
            lat: 45.559,
            lon: -122.645,
            label: Some("near Alberta St".to_string()),
        };
        let json = serde_json::to_string(&point).unwrap();
        let back: GeoPoint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, point);
    }
}
