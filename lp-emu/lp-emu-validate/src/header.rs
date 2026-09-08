//! The transcript contract's header.
//!
//! Two halves, because two different things know two different facts:
//!
//! * the **in-band header** is one line the payload prints on the device. It
//!   carries only what the firmware knows about itself — payload, chip, commit,
//!   feature set. `fw-checks` emits it `no_std` with no dependency beyond `log`.
//! * the **sidecar** (`<transcript>.meta.json`) is the full header, written by
//!   the runner. It adds what only the host knows: which configuration ran it,
//!   the board id and MAC, tool versions, the capture method, and the trust
//!   table saying what this configuration is believed for.
//!
//! The sidecar is authoritative, and it is required. The in-band line is
//! optional — the two transcripts this system was built on predate it — but
//! when both exist they must agree, and `Transcript::load` refuses them if they
//! do not. That is the whole point: a transcript whose provenance is a guess is
//! worse than no transcript.
//!
//! **Never edit a transcript.** A mismatch is a regression or a re-capture, not
//! a fixture to refresh (the FP replay rule,
//! `lp-emu/lp-xt-emu/tests/fp_silicon_replay.rs`).

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::configuration::{Configuration, TrustTable};

/// The current header schema. Bump it when a field's meaning changes; adding
/// an optional field does not need a bump.
pub const HEADER_SCHEMA: u32 = 1;

/// The prefix `fw-checks` prints the in-band header line behind.
///
/// Deliberately distinct from `[fw-check-json] ` so a header is never mistaken
/// for a record by an older parser.
pub const HEADER_PREFIX: &str = "[fw-checks-header] ";

/// The full header: the sidecar's contents.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TranscriptHeader {
    pub schema: u32,
    /// Payload name, as in the registry (`shader-compile-stress`).
    pub payload: String,
    pub chip: String,
    /// The configuration name (PD4): `silicon:esp32c6`.
    pub configuration: String,
    /// Capture date, `YYYY-MM-DD`.
    pub date: String,
    /// Short git commit of the firmware image, as `LP_BUILD_COMMIT` reports it.
    pub firmware_commit: String,
    /// The cargo features the image was built with.
    pub firmware_features: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware_dirty: Option<bool>,
    /// The sha256 of the image that produced this transcript, lower-case hex.
    ///
    /// Additive and optional: the two transcripts this system was built on
    /// predate it, and an added optional field needs no schema bump (see
    /// [`HEADER_SCHEMA`]).
    ///
    /// The commit and the feature set say which SOURCE ran; this says which
    /// BYTES did, and until L4 those were not the same question. M5 P1's
    /// digest (PR #569) found three CI runs of one pinned firmware commit
    /// producing three different ELFs — a wall-clock stamp in the ESP-IDF
    /// application descriptor, absolute paths in `.debug_str`, and a
    /// linker-script patch that lost a race on a cold build tree.
    /// `scripts/emu/build-reference-image.sh` removes all three and its
    /// `--verify` proves it, so a recorded sha is a fact another host can
    /// reproduce rather than a serial number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware_sha256: Option<String>,
    /// Silicon revision as the chip reports it (`v0.2`), or as the
    /// configuration synthesises it (esp-emu's eFuse says `v0.3` on a `v0.2`
    /// board — that disagreement is a fact worth carrying).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silicon_rev: Option<String>,
    /// The board the chip is on (`seeed/xiao-esp32-c6`).
    ///
    /// **Metadata, never part of the configuration key.** Identity is the chip
    /// (Yona, G2 2026-09-06): this is chip simulation, not board simulation,
    /// and the board is not something the runner can determine
    /// programmatically. It is a fact about one capture, recorded here beside
    /// `mac` and `silicon_rev`, and useful exactly when a specific bench setup
    /// turns out to matter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<String>,
    /// The chip's MAC, which is the only thing that distinguishes two boards
    /// of the same model on one desk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    /// Tool name -> version. espflash, esp-emu, the host toolchain.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<String, String>,
    /// Where the bytes came from, verbatim enough to find them again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// How they were captured — the method, in one paragraph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<String>,
    /// Anything else a reader needs before trusting a number here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// What this configuration is trusted for, per field class.
    #[serde(default)]
    pub trust: TrustTable,
}

impl TranscriptHeader {
    pub fn configuration(&self) -> Result<Configuration> {
        Configuration::parse(&self.configuration)
    }

    /// The committed filename stem: `<configuration>-<date>-<short-commit>`.
    pub fn file_stem(&self) -> Result<String> {
        let config = self.configuration()?;
        let short = short_commit(&self.firmware_commit);
        Ok(format!("{}-{}-{}", config.slug(), self.date, short))
    }

    /// The committed path, relative to the transcripts root:
    /// `<chip>/<payload>/<stem>.txt`.
    pub fn relative_path(&self) -> Result<String> {
        Ok(format!(
            "{}/{}/{}.txt",
            self.chip,
            self.payload,
            self.file_stem()?
        ))
    }

    pub fn from_json(text: &str) -> Result<Self> {
        let header: Self = serde_json::from_str(text).context("parsing transcript header JSON")?;
        header.validate()?;
        Ok(header)
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != HEADER_SCHEMA {
            bail!(
                "transcript header schema {} is not {HEADER_SCHEMA}; this build cannot read it",
                self.schema
            );
        }
        for (name, value) in [
            ("payload", &self.payload),
            ("chip", &self.chip),
            ("configuration", &self.configuration),
            ("date", &self.date),
            ("firmware_commit", &self.firmware_commit),
        ] {
            if value.trim().is_empty() {
                bail!("transcript header field `{name}` is empty");
            }
        }
        if self.firmware_features.is_empty() {
            bail!("transcript header field `firmware_features` is empty");
        }
        self.configuration()?;
        if !is_iso_date(&self.date) {
            bail!(
                "transcript header `date` must be YYYY-MM-DD, got `{}`",
                self.date
            );
        }
        Ok(())
    }

    /// Does the device's own account of itself match the sidecar's?
    pub fn agrees_with_inband(&self, inband: &InbandHeader) -> Result<()> {
        let mut wrong = Vec::new();
        if inband.payload != self.payload {
            wrong.push(format!(
                "payload: in-band `{}` vs sidecar `{}`",
                inband.payload, self.payload
            ));
        }
        if inband.chip != self.chip {
            wrong.push(format!(
                "chip: in-band `{}` vs sidecar `{}`",
                inband.chip, self.chip
            ));
        }
        if !commits_agree(&inband.firmware_commit, &self.firmware_commit) {
            wrong.push(format!(
                "firmware_commit: in-band `{}` vs sidecar `{}`",
                inband.firmware_commit, self.firmware_commit
            ));
        }
        let mut inband_features: Vec<&str> = inband.firmware_features.split(',').collect();
        let mut sidecar_features: Vec<&str> =
            self.firmware_features.iter().map(String::as_str).collect();
        inband_features.sort_unstable();
        sidecar_features.sort_unstable();
        if inband_features != sidecar_features {
            wrong.push(format!(
                "firmware_features: in-band `{}` vs sidecar `{}`",
                inband_features.join(","),
                sidecar_features.join(",")
            ));
        }
        if wrong.is_empty() {
            Ok(())
        } else {
            bail!(
                "the transcript's in-band header disagrees with its sidecar:\n  {}\n\
                 Neither may be edited to make them agree — re-capture instead.",
                wrong.join("\n  ")
            )
        }
    }
}

/// The half the device prints, `no_std`, on one line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InbandHeader {
    pub schema: u32,
    pub payload: String,
    pub chip: String,
    pub firmware_commit: String,
    /// Comma-separated, as the firmware's `LP_BUILD_FEATURES` reports it.
    pub firmware_features: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub firmware_dirty: Option<bool>,
}

impl InbandHeader {
    /// Parse the header out of any line that carries the prefix.
    pub fn from_line(line: &str) -> Result<Option<Self>> {
        let Some(idx) = line.find(HEADER_PREFIX) else {
            return Ok(None);
        };
        let json = line[idx + HEADER_PREFIX.len()..].trim();
        let header: Self = serde_json::from_str(json)
            .with_context(|| format!("parsing in-band transcript header: {json}"))?;
        if header.schema != HEADER_SCHEMA {
            bail!(
                "in-band transcript header schema {} is not {HEADER_SCHEMA}",
                header.schema
            );
        }
        Ok(Some(header))
    }

    /// The exact line `fw-checks` prints. Kept here so the host has a spec to
    /// test the device's emitter against.
    pub fn to_line(&self) -> String {
        format!(
            "{HEADER_PREFIX}{}",
            serde_json::to_string(self).expect("in-band header serialises")
        )
    }
}

/// `d6cfaa2051ae` -> `d6cfaa205`. Nine characters is what the plan's filenames
/// use and what a human can retype.
pub fn short_commit(commit: &str) -> &str {
    let n = commit.len().min(9);
    &commit[..n]
}

/// Two commit strings agree if one is a prefix of the other — the firmware
/// embeds 12 characters and filenames carry 9.
fn commits_agree(a: &str, b: &str) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> TranscriptHeader {
        TranscriptHeader {
            schema: HEADER_SCHEMA,
            payload: "shader-compile-stress".into(),
            chip: "esp32c6".into(),
            configuration: "silicon:esp32c6".into(),
            date: "2026-09-06".into(),
            firmware_commit: "d6cfaa2051ae".into(),
            firmware_features: vec![
                "esp32c6".into(),
                "spike_uart0_link".into(),
                "test_shader_compile_incremental".into(),
            ],
            firmware_dirty: Some(false),
            firmware_sha256: None,
            silicon_rev: Some("v0.2".into()),
            board: Some("seeed/xiao-esp32-c6".into()),
            mac: Some("a0:f2:62:87:b4:8c".into()),
            tools: BTreeMap::new(),
            source: None,
            capture: None,
            note: None,
            trust: TrustTable::default(),
        }
    }

    #[test]
    fn file_stem_and_path_follow_the_contract() {
        let h = header();
        assert_eq!(
            h.file_stem().unwrap(),
            "silicon-esp32c6-2026-09-06-d6cfaa205"
        );
        assert_eq!(
            h.relative_path().unwrap(),
            "esp32c6/shader-compile-stress/silicon-esp32c6-2026-09-06-d6cfaa205.txt"
        );
    }

    #[test]
    fn round_trips_through_json() {
        let h = header();
        let back = TranscriptHeader::from_json(&h.to_json().unwrap()).unwrap();
        assert_eq!(back.payload, h.payload);
        assert_eq!(back.mac, h.mac);
        assert_eq!(back.firmware_features, h.firmware_features);
    }

    /// `firmware_sha256` is additive in both directions: a sidecar written
    /// before it existed still loads, and one that carries it keeps it
    /// through a round trip. That is the whole M2 contract for adding a
    /// field — no schema bump, no committed transcript touched.
    #[test]
    fn the_image_sha_is_an_additive_field() {
        let mut h = header();
        h.firmware_sha256 = None;
        let without = h.to_json().unwrap();
        assert!(
            !without.contains("firmware_sha256"),
            "an absent sha is not written at all: {without}"
        );

        h.firmware_sha256 =
            Some("61027da9eabbf137a2f3ed5846350293f2bc79fea91fd9ee5592fa9f4aa16ba8".to_string());
        let with = h.to_json().unwrap();
        assert_eq!(
            TranscriptHeader::from_json(&with)
                .unwrap()
                .firmware_sha256
                .as_deref(),
            Some("61027da9eabbf137a2f3ed5846350293f2bc79fea91fd9ee5592fa9f4aa16ba8")
        );
        // And the pre-L4 sidecars, which have no such key.
        assert_eq!(
            TranscriptHeader::from_json(&without)
                .unwrap()
                .firmware_sha256,
            None
        );
    }

    #[test]
    fn rejects_a_bad_date_and_an_empty_field() {
        let mut h = header();
        h.date = "2026-9-6".into();
        assert!(h.validate().is_err());

        let mut h = header();
        h.chip = String::new();
        assert!(h.validate().is_err());
    }

    #[test]
    fn rejects_a_future_schema() {
        let mut h = header();
        h.schema = HEADER_SCHEMA + 1;
        assert!(h.validate().is_err());
    }

    #[test]
    fn inband_round_trips_through_its_line() {
        let ib = InbandHeader {
            schema: HEADER_SCHEMA,
            payload: "shader-compile-stress".into(),
            chip: "esp32c6".into(),
            firmware_commit: "d6cfaa2051ae".into(),
            firmware_features: "esp32c6,spike_uart0_link,test_shader_compile_incremental".into(),
            firmware_dirty: Some(false),
        };
        let line = ib.to_line();
        assert!(line.starts_with(HEADER_PREFIX));
        let parsed = InbandHeader::from_line(&line).unwrap().unwrap();
        assert_eq!(parsed, ib);
    }

    #[test]
    fn inband_is_found_behind_a_log_prefix() {
        let ib = InbandHeader {
            schema: HEADER_SCHEMA,
            payload: "gpio-calibrate".into(),
            chip: "esp32c6".into(),
            firmware_commit: "abc123456789".into(),
            firmware_features: "esp32c6,test_gpio_calibrate".into(),
            firmware_dirty: None,
        };
        let line = format!("[INFO] fw_checks: {}", ib.to_line());
        assert_eq!(
            InbandHeader::from_line(&line).unwrap().unwrap().payload,
            "gpio-calibrate"
        );
    }

    #[test]
    fn a_line_without_the_prefix_is_not_a_header() {
        assert!(
            InbandHeader::from_line("I (23) boot: hello")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn agreement_tolerates_commit_length_but_not_content() {
        let h = header();
        let mut ib = InbandHeader {
            schema: HEADER_SCHEMA,
            payload: h.payload.clone(),
            chip: h.chip.clone(),
            firmware_commit: "d6cfaa205".into(),
            firmware_features: "test_shader_compile_incremental,esp32c6,spike_uart0_link".into(),
            firmware_dirty: Some(false),
        };
        h.agrees_with_inband(&ib).unwrap();

        ib.firmware_commit = "deadbeef0000".into();
        let err = h.agrees_with_inband(&ib).unwrap_err().to_string();
        assert!(err.contains("firmware_commit"), "{err}");
        assert!(err.contains("re-capture"), "{err}");
    }

    #[test]
    fn agreement_notices_a_missing_feature() {
        let h = header();
        let ib = InbandHeader {
            schema: HEADER_SCHEMA,
            payload: h.payload.clone(),
            chip: h.chip.clone(),
            firmware_commit: h.firmware_commit.clone(),
            firmware_features: "test_shader_compile_incremental,esp32c6".into(),
            firmware_dirty: Some(false),
        };
        let err = h.agrees_with_inband(&ib).unwrap_err().to_string();
        assert!(err.contains("firmware_features"), "{err}");
    }
}
