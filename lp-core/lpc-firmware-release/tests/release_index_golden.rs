//! The release index's format-1 compatibility pin.
//!
//! `fixtures/release-index.v1.json` is what
//! `/api/v1/firmware/<target>/releases` answers at format 1. **A later
//! change that fails to read it, or writes it differently, is a format
//! break**: Studios in the field read this shape.
//! Never edit the fixture to make a test pass. Its values are two real
//! releases (`2026.10.06-19` at wire proto 39, `2026.10.06-11` at 38),
//! copied from their published `ota-manifest.json` and GitHub's release
//! list on 2026-10-07.

use lpc_firmware_release::{ReleaseIndex, ReleaseIndexError, ReleaseVersion};
use serde_json::{Value, json};

const GOLDEN: &[u8] = include_bytes!("fixtures/release-index.v1.json");

#[test]
fn the_golden_parses_validates_and_round_trips_byte_identically() {
    let index = ReleaseIndex::parse_valid(GOLDEN).unwrap();
    assert_eq!(index.format, 1);
    assert_eq!(index.target, "esp32c6-4mb");
    assert_eq!(index.releases.len(), 2);
    let newest = &index.releases[0];
    assert_eq!(newest.version, "2026.10.06-19");
    assert_eq!(newest.wire_proto, 39);
    assert_eq!((newest.requires.layout, newest.requires.loader), (1, 1));
    assert_eq!(newest.published_at.as_deref(), Some("2026-10-07T05:29:21Z"));
    assert_eq!(newest.build_id(), "2026.10.06-19+736d72856d24");
    let older = &index.releases[1];
    assert_eq!(older.wire_proto, 38);
    assert_eq!(older.published_at, None);
    assert!(
        ReleaseVersion::parse(&newest.version) > ReleaseVersion::parse(&older.version),
        "newest first"
    );
    assert_eq!(
        String::from_utf8(index.to_json_bytes()).unwrap(),
        String::from_utf8(GOLDEN.to_vec()).unwrap()
    );
}

#[test]
fn unknown_fields_are_ignored_at_every_level() {
    let mut value: Value = serde_json::from_slice(GOLDEN).unwrap();
    value["nextPage"] = json!("/api/v1/firmware/esp32c6-4mb/releases?before=2026.10.06-11");
    value["releases"][0]["capabilities"] = json!(["bluetooth-updates"]);
    value["releases"][1]["requires"]["radio"] = json!(2);
    let index = ReleaseIndex::parse_valid(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(index, ReleaseIndex::parse(GOLDEN).unwrap());
}

#[test]
fn another_format_is_refused() {
    let mut value: Value = serde_json::from_slice(GOLDEN).unwrap();
    value["format"] = json!(2);
    assert_eq!(
        ReleaseIndex::parse(&serde_json::to_vec(&value).unwrap()),
        Err(ReleaseIndexError::UnsupportedFormat(2))
    );
}
