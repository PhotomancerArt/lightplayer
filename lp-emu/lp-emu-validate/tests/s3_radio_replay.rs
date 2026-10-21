//! The S3's radio image, as a replay of committed transcripts (S3 radio M1,
//! AC2) — the first S3 silicon capture in the tree.
//!
//! One bench sitting (FS3 `fixture-s3`, D8:3B:DA:47:29:70, 2026-10-21) and its
//! emulated twin, both of `fw-esp32s3 --features radio_boot_probe` at the
//! same commit, recorded through the front door:
//!
//! ```bash
//! lp-cli validate record s3-radio --config silicon:esp32s3 \
//!   --port <resolved by MAC> --date 2026-10-21 --commit e5d8eeebe7f4
//! lp-cli validate record s3-radio --config lp-emu:esp32s3:t1 \
//!   --link-host lp-link --date 2026-10-21 --commit e5d8eeebe7f4
//! ```
//!
//! What the pair holds, and the reason it means anything:
//!
//! - **the same bytes** — both sidecars state one `firmware_sha256`;
//! - **the radio's whole heap ledger, equal to the byte** — what
//!   `esp_radio::wifi::new`, the station, `BleConnector::new` and the HCI
//!   reset each took (`radio-stage`, twelve memory values). The blobs
//!   allocate by configuration, not by what the air says, so a machine whose
//!   radio windows answered a poll wrongly enough to send the blobs down
//!   another path would show here;
//! - **the BTDM controller's answer, equal** — its Command Complete for the
//!   Reset (`radio-hci-event`): the controller's ROM routines on real
//!   registers and on modelled ones say the same thing.
//!
//! What it does NOT hold: anything about the air (silicon's scan found three
//! access points, ours none — not compared, and not comparable), timing, or a
//! pin (no pin capture on either side). **Never edit a transcript**: a failure
//! here is a regression or a re-capture under a new stem.

use std::path::PathBuf;

use lp_emu_validate::grade::FieldClass;
use lp_emu_validate::replay::{ReplayOptions, replay};
use lp_emu_validate::transcript::{Transcript, sidecar_path};
use lp_emu_validate::{TranscriptHeader, find_payload};

const SILICON: &str = "silicon-esp32s3-2026-10-21-e5d8eeebe.txt";
const OURS: &str = "lp-emu-esp32s3-t1-2026-10-21-e5d8eeebe.txt";

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../transcripts/esp32s3/radio-boot")
}

fn load(name: &str) -> Transcript {
    let path = dir().join(name);
    Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()))
}

#[test]
fn the_pair_ran_one_image() {
    let (silicon, ours) = (load(SILICON), load(OURS));
    assert_eq!(silicon.header.configuration, "silicon:esp32s3");
    assert_eq!(ours.header.configuration, "lp-emu:esp32s3:t1");
    assert_eq!(silicon.header.payload, "radio-boot");
    let a = silicon
        .header
        .firmware_sha256
        .as_deref()
        .expect("silicon's image hash");
    let b = ours
        .header
        .firmware_sha256
        .as_deref()
        .expect("our image hash");
    assert_eq!(a, b, "two builds, not two machines");
    assert!(find_payload("radio-boot").is_ok());
}

#[test]
fn the_radio_heap_ledger_and_the_controllers_answer_match_silicon() {
    let report =
        replay(&load(OURS), &load(SILICON), ReplayOptions::default()).expect("the replay runs");
    assert!(report.is_ok(), "{}", report.render());
    assert_eq!(
        report.compared(FieldClass::Memory),
        12,
        "six stages, free and used: {}",
        report.render()
    );
    assert_eq!(report.differences_in(FieldClass::Memory).count(), 0);
    assert_eq!(
        report.compared(FieldClass::Structural),
        1,
        "{}",
        report.render()
    );
    assert_eq!(report.differences_in(FieldClass::Structural).count(), 0);
}

/// The negative control: one byte of the BLE controller's allocation moved
/// in OUR transcript's body — reloaded in memory, the file untouched — must
/// be a memory difference the replay reports, or the comparison above
/// proves nothing.
#[test]
fn a_moved_figure_is_a_memory_difference() {
    let path = dir().join(OURS);
    let body = std::fs::read_to_string(&path).unwrap();
    let header =
        TranscriptHeader::from_json(&std::fs::read_to_string(sidecar_path(&path)).unwrap())
            .unwrap();
    let liar = body.replace(
        "stage=ble_new heap_free=155200 heap_used=82380",
        "stage=ble_new heap_free=155196 heap_used=82384",
    );
    assert_ne!(liar, body, "the figure to move is in the transcript");
    let liar = Transcript::from_parts(header, &liar).unwrap();
    let report = replay(&liar, &load(SILICON), ReplayOptions::default()).expect("the replay runs");
    assert!(!report.is_ok(), "{}", report.render());
    assert_eq!(
        report.differences_in(FieldClass::Memory).count(),
        2,
        "{}",
        report.render()
    );
}
