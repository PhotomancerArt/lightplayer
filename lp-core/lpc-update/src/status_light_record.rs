//! **The update light's record**, `/.lp/status-light.json`, format 1 (DM22,
//! doors #11): the one strip the core may light while it has no engine.
//!
//! ```json
//! { "format": 1, "driver": "ws281x", "pin": 18, "count": 73, "colorOrder": "grb" }
//! ```
//!
//! **Written by the engine** when a project's first WS281x output opens, and
//! only when it differs from what is on disk; **read by the core** when it
//! runs core-only, to show what an update is doing: **dark yellow** while a
//! transfer is pending or running, **dark red** while the board waits for
//! its engine or the engine keeps crashing. It is the first piece of the
//! "LEDs indicate system events" plan; the core never parses the hardware
//! manifest for it.
//!
//! # A soft door
//!
//! The shape version is `format`, never `version` (which means the app
//! version, doors #1). A core may read what a newer or older engine wrote
//! (after an update or a rollback), so readers ignore unknown keys and stay
//! dark for an unknown `driver`, an unknown `colorOrder` or another
//! `format`: the worst case is a dark strip. A list of outputs can be added
//! later as a new key. Device-only: never part of a project, so
//! `lpa-upgrade` never sees it. Schema: `schemas/status-light.schema.json`.

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::board_manifest::BoardState;

/// Where the record lives on a device's filesystem.
pub const STATUS_LIGHT_PATH: &str = "/.lp/status-light.json";

/// The only `format` this crate reads and writes.
pub const STATUS_LIGHT_FORMAT: u32 = 1;

/// The one driver format 1 names.
pub const DRIVER_WS281X: &str = "ws281x";

/// Dark yellow, `(r, g, b)`: a transfer is pending or running. Dim on
/// purpose: a long strip lit solid must not draw much power.
pub const UPDATING_RGB: [u8; 3] = [24, 16, 0];

/// Dark red, `(r, g, b)`: the board waits for its engine, or its engine
/// keeps crashing.
pub const NEEDS_ENGINE_RGB: [u8; 3] = [24, 0, 0];

/// `/.lp/status-light.json`. See the module docs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema-gen",
    schemars(
        title = "status-light.json",
        description = "The update light's record, root `/.lp/status-light.json` on a device (lpc_update::StatusLightRecord): the strip the over-the-air core lights while it has no engine — dark yellow while updating, dark red while it waits for its engine. Written by the engine when a project's first WS281x output opens. Format 1; readers ignore unknown keys and stay dark for an unknown driver or colour order. Device-only: never part of a project, so lpa-upgrade never sees it."
    )
)]
#[serde(rename_all = "camelCase")]
pub struct StatusLightRecord {
    /// Shape version; always `1` ([`STATUS_LIGHT_FORMAT`]).
    pub format: u32,
    /// How the strip is driven: `"ws281x"` in format 1.
    pub driver: String,
    /// The GPIO the strip's data line is on.
    pub pin: u8,
    /// LEDs on the strip.
    pub count: u32,
    /// The byte order the strip takes a colour in (`"grb"`, `"rgb"`, …).
    pub color_order: String,
}

impl StatusLightRecord {
    /// A format-1 WS281x record.
    #[must_use]
    pub fn ws281x(pin: u8, count: u32, color_order: &str) -> Self {
        Self {
            format: STATUS_LIGHT_FORMAT,
            driver: DRIVER_WS281X.into(),
            pin,
            count,
            color_order: color_order.into(),
        }
    }

    /// The bytes the engine writes.
    #[must_use]
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// The record in `bytes`, if it is one this reader can light: format 1,
    /// the WS281x driver, a colour order it knows. Anything else is `None`
    /// (stay dark).
    #[must_use]
    pub fn read(bytes: &[u8]) -> Option<Self> {
        let record: Self = serde_json::from_slice(bytes).ok()?;
        (record.format == STATUS_LIGHT_FORMAT
            && record.driver == DRIVER_WS281X
            && record.count > 0
            && order_of(&record.color_order).is_some())
        .then_some(record)
    }

    /// `rgb` as the three bytes this strip takes, in its order.
    #[must_use]
    pub fn wire_bytes(&self, rgb: [u8; 3]) -> [u8; 3] {
        let order = order_of(&self.color_order).unwrap_or([0, 1, 2]);
        [rgb[order[0]], rgb[order[1]], rgb[order[2]]]
    }
}

/// The colour the light shows for a board state, or `None` (dark).
#[must_use]
pub fn light_for(state: BoardState) -> Option<[u8; 3]> {
    match state {
        BoardState::Updating => Some(UPDATING_RGB),
        BoardState::NeedsEngine | BoardState::EngineCrashing => Some(NEEDS_ENGINE_RGB),
        BoardState::Running | BoardState::OnTrial | BoardState::Unknown => None,
    }
}

/// For each wire byte, which of `r, g, b` it carries.
fn order_of(order: &str) -> Option<[usize; 3]> {
    Some(match order {
        "rgb" => [0, 1, 2],
        "rbg" => [0, 2, 1],
        "grb" => [1, 0, 2],
        "gbr" => [1, 2, 0],
        "brg" => [2, 0, 1],
        "bgr" => [2, 1, 0],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips_in_its_published_shape() {
        let r = StatusLightRecord::ws281x(18, 73, "grb");
        let json = r.to_json();
        assert_eq!(
            core::str::from_utf8(&json).unwrap(),
            r#"{"format":1,"driver":"ws281x","pin":18,"count":73,"colorOrder":"grb"}"#
        );
        assert_eq!(StatusLightRecord::read(&json), Some(r));
    }

    #[test]
    fn unknown_keys_are_ignored_and_unknown_drivers_stay_dark() {
        let newer =
            br#"{"format":1,"driver":"ws281x","pin":2,"count":5,"colorOrder":"rgb","outputs":[]}"#;
        assert_eq!(StatusLightRecord::read(newer).map(|r| r.pin), Some(2));
        for dark in [
            &br#"{"format":2,"driver":"ws281x","pin":2,"count":5,"colorOrder":"rgb"}"#[..],
            br#"{"format":1,"driver":"apa102","pin":2,"count":5,"colorOrder":"rgb"}"#,
            br#"{"format":1,"driver":"ws281x","pin":2,"count":5,"colorOrder":"rgbw"}"#,
            br#"{"format":1,"driver":"ws281x","pin":2,"count":0,"colorOrder":"rgb"}"#,
            br#"{"version":1}"#,
            b"not json",
        ] {
            assert_eq!(StatusLightRecord::read(dark), None);
        }
    }

    #[test]
    fn colours_follow_the_state_and_the_strips_order() {
        assert_eq!(light_for(BoardState::Updating), Some(UPDATING_RGB));
        assert_eq!(light_for(BoardState::NeedsEngine), Some(NEEDS_ENGINE_RGB));
        assert_eq!(
            light_for(BoardState::EngineCrashing),
            Some(NEEDS_ENGINE_RGB)
        );
        assert_eq!(light_for(BoardState::Running), None);
        let grb = StatusLightRecord::ws281x(1, 1, "grb");
        assert_eq!(grb.wire_bytes([24, 16, 0]), [16, 24, 0]);
        let bgr = StatusLightRecord::ws281x(1, 1, "bgr");
        assert_eq!(bgr.wire_bytes([1, 2, 3]), [3, 2, 1]);
    }
}
