//! Golden-trace replay (multi-device roadmap M8): every committed capture
//! under `testdata/device-traces/` is replayed through the boot-line
//! classifier, pinning classification against REAL bytes from real boards
//! rather than synthetic fixtures.
//!
//! Traces are JSONL in the device-event-log contract (one JSON object per
//! line; see `lpa-studio-core::core::log::device_event_log`): `rx` records
//! carry raw serial lines, `state` records carry the transitions Studio
//! derived at capture time. The replay feeds the `rx` lines to a fresh
//! [`BootLineClassifier`] and asserts its no-firmware diagnosis agrees
//! with any no-firmware state the trace recorded.
//!
//! No traces committed yet ⇒ the test passes trivially (the M8 capture
//! sitting populates the directory). It fails loudly on a trace it cannot
//! parse — a malformed fixture is worse than a missing one.
//!
//! Compiles to nothing without the `device-session` feature (a solo
//! default-features `cargo test -p lpa-link`); the workspace test run
//! unifies the studio's features onto this crate and runs it.
#![cfg(feature = "device-session")]

use std::fs;
use std::path::PathBuf;

use lpa_link::device_session::{BootLineClassifier, NoFirmwareReason};

fn trace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/device-traces")
}

/// The no-firmware states the device-event-log's state labels use, mapped
/// to the classifier reason that must reproduce them.
fn no_firmware_reason_for_label(label: &str) -> Option<NoFirmwareReason> {
    match label {
        "blank-flash" => Some(NoFirmwareReason::BlankOrErasedFlash),
        "bootloader" => Some(NoFirmwareReason::RomDownloadMode),
        "foreign-firmware" => Some(NoFirmwareReason::SafeToReplaceFirmware),
        _ => None,
    }
}

#[test]
fn committed_traces_replay_through_the_classifier() {
    let dir = trace_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        // The directory ships with the repo; absence means a broken layout.
        panic!("trace directory missing: {}", dir.display());
    };
    let mut replayed = 0usize;
    for entry in entries {
        let path = entry.expect("readable dir entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let text = fs::read_to_string(&path).expect("readable trace");

        let mut classifier = BootLineClassifier::new();
        let mut recorded_no_firmware: Option<NoFirmwareReason> = None;
        for (line_number, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|error| {
                panic!("{name}:{}: malformed trace line: {error}", line_number + 1)
            });
            match record["kind"].as_str() {
                Some("rx") => {
                    let raw = record["line"].as_str().unwrap_or_else(|| {
                        panic!("{name}:{}: rx record without a line", line_number + 1)
                    });
                    classifier.observe_line(raw);
                }
                Some("state") => {
                    if let Some(reason) =
                        record["to"].as_str().and_then(no_firmware_reason_for_label)
                    {
                        recorded_no_firmware = Some(reason);
                    }
                }
                _ => {}
            }
        }

        if let Some(reason) = recorded_no_firmware {
            assert!(
                classifier.no_firmware_detected(),
                "{name}: the trace recorded a no-firmware state but the classifier, \
                 replaying the same raw lines, no longer detects one"
            );
            assert_eq!(
                classifier.no_firmware_reason(),
                reason,
                "{name}: classifier reason diverged from the captured state"
            );
        }
        replayed += 1;
    }
    // Informational only — zero traces is the pre-sitting state.
    eprintln!("replayed {replayed} golden trace(s) from {}", dir.display());
}
