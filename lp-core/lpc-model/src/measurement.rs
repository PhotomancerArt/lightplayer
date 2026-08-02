//! Measured hardware envelopes: what a (build × board) pair was *observed* to
//! survive, with the provenance needed to judge whether the observation still
//! applies.
//!
//! A [`MeasurementRecord`] is machine-written — `lp-cli hardware bench` runs
//! the workload on real hardware and writes the record; nothing in this repo
//! types one by hand. The record is deliberately a *measurement*, not a
//! promise: it names the metric (`id@version`), the raw boundary the run
//! found, the safety margin applied to it, and the firmware/date the run
//! observed. A studio-side advisory reads it; the device never enforces it.
//!
//! Both the raw boundary and the margined limit are stored so the derivation
//! stays auditable: `limit_leds == floor(raw_boundary_leds * margin)`, checked
//! by whoever loads the record. See `measurements/README.md` for the store
//! layout and staleness posture.

use alloc::string::{String, ToString};

use serde::{Deserialize, Serialize};

/// Metric id: the largest LED count a build survives on a board without
/// running out of memory.
///
/// Metric identity is `id@version` — the id names *what* is measured, the
/// version pins the workload and procedure that measured it. Changing the
/// workload, the OOM criterion, or the ramp procedure bumps
/// [`MEASUREMENT_METRIC_VERSION`]; records from an older version stay readable
/// but are not comparable.
pub const MEASUREMENT_METRIC_LEDS_MAX_SAFE: &str = "leds.max-safe";

/// Version of the metric definitions in this module.
pub const MEASUREMENT_METRIC_VERSION: u32 = 1;

/// Safety factor applied to the measured boundary to get the advertised limit.
/// The boundary is where a board died; the limit is where it is asked to live.
pub const MEASUREMENT_DEFAULT_MARGIN: f32 = 0.8;

/// One measured envelope for one (build × board) pair.
///
/// Serialized camelCase as `measurements/<build-id>/<board-id>.json`, with
/// slashes in the board id written as dashes. Plain serde on purpose: this is
/// a host-side tooling shape, not a wire or authored-artifact shape, so it
/// carries no `schemars` derive (schema generation is for the formats the
/// studio edits).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeasurementRecord {
    /// Metric id, e.g. [`MEASUREMENT_METRIC_LEDS_MAX_SAFE`].
    pub metric: String,
    /// Metric version; with `metric`, the full metric identity.
    pub metric_version: u32,
    /// Firmware build def id (`lp-fw/builds/<id>.json`), e.g. `esp32c6-4mb`.
    pub build_id: String,
    /// Board manifest id, e.g. `seeed/xiao-esp32-c6`.
    pub board_id: String,
    /// Largest LED count that survived the run; the boundary the bisect found.
    pub raw_boundary_leds: u32,
    /// The advertised limit: `floor(raw_boundary_leds * margin)`.
    pub limit_leds: u32,
    /// Safety factor between boundary and limit (see
    /// [`MEASUREMENT_DEFAULT_MARGIN`]).
    pub margin: f32,
    /// ISO date (`YYYY-MM-DD`) the run happened.
    pub measured_on: String,
    /// Commit of the firmware under test, from its embedded manifest core.
    pub fw_commit: String,
    /// Whether that firmware was built from a dirty tree.
    pub fw_dirty: bool,
    /// Version of the bench harness that produced the record. Bumped when the
    /// harness changes in a way that could move the boundary without the
    /// metric definition changing.
    pub harness_version: u32,
    /// Free-form operator note (desk conditions, anomalies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl MeasurementRecord {
    /// Build a `leds.max-safe` record, deriving [`Self::limit_leds`] from the
    /// boundary and margin. The derivation lives here so no caller can type a
    /// limit that disagrees with the boundary it claims to come from.
    #[allow(clippy::too_many_arguments, reason = "a record is its provenance")]
    pub fn new_leds_max_safe(
        build_id: impl Into<String>,
        board_id: impl Into<String>,
        raw_boundary_leds: u32,
        margin: f32,
        measured_on: impl Into<String>,
        fw_commit: impl Into<String>,
        fw_dirty: bool,
        harness_version: u32,
    ) -> Self {
        Self {
            metric: MEASUREMENT_METRIC_LEDS_MAX_SAFE.to_string(),
            metric_version: MEASUREMENT_METRIC_VERSION,
            build_id: build_id.into(),
            board_id: board_id.into(),
            raw_boundary_leds,
            limit_leds: derive_limit_leds(raw_boundary_leds, margin),
            margin,
            measured_on: measured_on.into(),
            fw_commit: fw_commit.into(),
            fw_dirty,
            harness_version,
            notes: None,
        }
    }

    /// Whether this record measures `metric` at exactly `version`. Records of
    /// another metric version are readable but not comparable.
    pub fn is_metric(&self, metric: &str, version: u32) -> bool {
        self.metric == metric && self.metric_version == version
    }

    /// Whether [`Self::limit_leds`] agrees with the stored boundary and
    /// margin. A record that fails this was hand-edited.
    pub fn limit_is_derived(&self) -> bool {
        self.limit_leds == derive_limit_leds(self.raw_boundary_leds, self.margin)
    }
}

/// `floor(raw_boundary_leds * margin)`, saturating at zero for a nonsense
/// margin. Truncation *is* floor here: both operands are non-negative.
pub fn derive_limit_leds(raw_boundary_leds: u32, margin: f32) -> u32 {
    let scaled = f64::from(raw_boundary_leds) * f64::from(margin);
    if scaled <= 0.0 {
        return 0;
    }
    scaled as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record survives the JSON round trip it is stored as, with the
    /// camelCase field names the store files carry.
    #[test]
    fn record_round_trips_through_camel_case_json() {
        let record = MeasurementRecord::new_leds_max_safe(
            "esp32c6-4mb",
            "seeed/xiao-esp32-c6",
            250,
            MEASUREMENT_DEFAULT_MARGIN,
            "2026-08-02",
            "5466346",
            false,
            1,
        );

        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"rawBoundaryLeds\":250"), "{json}");
        assert!(json.contains("\"limitLeds\":200"), "{json}");
        assert!(
            json.contains("\"boardId\":\"seeed/xiao-esp32-c6\""),
            "{json}"
        );
        // `notes` is absent rather than null when unset.
        assert!(!json.contains("notes"), "{json}");

        let parsed: MeasurementRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, record);
        assert!(parsed.is_metric(MEASUREMENT_METRIC_LEDS_MAX_SAFE, 1));
    }

    /// The constructor derives the limit; it is never an independent input.
    #[test]
    fn limit_is_floor_of_boundary_times_margin() {
        assert_eq!(derive_limit_leds(250, 0.8), 200);
        assert_eq!(derive_limit_leds(255, 0.8), 204);
        // 0.8 is not representable in binary: 249 * 0.8 is 199.2, and the
        // stored margin being a hair above 0.8 must not round the floor up.
        assert_eq!(derive_limit_leds(249, 0.8), 199);
        assert_eq!(derive_limit_leds(1, 0.8), 0);
        assert_eq!(derive_limit_leds(0, 0.8), 0);
        assert_eq!(derive_limit_leds(100, 1.0), 100);
        assert_eq!(derive_limit_leds(100, -1.0), 0);

        let record = MeasurementRecord::new_leds_max_safe(
            "esp32v3-4mb",
            "domraem/dom-z-102",
            330,
            MEASUREMENT_DEFAULT_MARGIN,
            "2026-08-02",
            "unknown",
            true,
            1,
        );
        assert_eq!(record.limit_leds, 264);
        assert!(record.limit_is_derived());
    }

    /// A hand-edited limit is detectable — the whole reason both numbers are
    /// stored.
    #[test]
    fn hand_edited_limit_fails_the_derivation_check() {
        let mut record = MeasurementRecord::new_leds_max_safe(
            "esp32s3-8mb",
            "seeed/xiao-esp32-s3-plus",
            400,
            MEASUREMENT_DEFAULT_MARGIN,
            "2026-08-02",
            "5466346",
            false,
            1,
        );
        assert!(record.limit_is_derived());

        record.limit_leds = 400;
        assert!(!record.limit_is_derived());
    }

    /// Unknown fields are refused: a record shape from a newer tool is an
    /// error, not a partial decode (alpha posture, as with build defs).
    #[test]
    fn unknown_fields_are_refused() {
        let json = r#"{
            "metric": "leds.max-safe",
            "metricVersion": 1,
            "buildId": "esp32c6-4mb",
            "boardId": "seeed/xiao-esp32-c6",
            "rawBoundaryLeds": 250,
            "limitLeds": 200,
            "margin": 0.8,
            "measuredOn": "2026-08-02",
            "fwCommit": "5466346",
            "fwDirty": false,
            "harnessVersion": 1,
            "fps": 42
        }"#;
        assert!(serde_json::from_str::<MeasurementRecord>(json).is_err());
    }
}
