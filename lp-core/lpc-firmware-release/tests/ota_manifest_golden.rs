//! The format-1 compatibility pin.
//!
//! `fixtures/ota-manifest.v1.json` is what a format-1 manifest looks like on
//! a release. **A later change that fails to read it is a format break**:
//! every release that carries one stays in the archive forever, and every
//! future Studio must read it. Never edit the fixture to make a test pass.

use lpc_firmware_release::{
    ENCODING_DEFLATE_DICT_V1, OTA_MANIFEST_FILE, OtaManifest, OtaManifestError, sha256_hex,
};
use serde_json::{Value, json};

const GOLDEN: &[u8] = include_bytes!("fixtures/ota-manifest.v1.json");

#[test]
fn the_golden_parses_validates_and_round_trips_byte_identically() {
    let manifest = OtaManifest::parse_valid(GOLDEN).unwrap();
    assert_eq!(manifest.target, "esp32c6-4mb");
    assert_eq!(manifest.chip, "esp32c6");
    assert_eq!(manifest.wire_proto, 36);
    assert_eq!((manifest.requires.layout, manifest.requires.loader), (1, 1));
    assert_eq!(manifest.build_id(), "2026.10.05-3+103285d5d05e");
    assert!(manifest.is_release());
    let e1 = manifest.encoding1().unwrap();
    assert_eq!(
        e1.engine.chunks[0], 0,
        "chunk 0 has no compressed form: send raw"
    );
    assert_eq!(
        String::from_utf8(manifest.to_json_bytes()).unwrap(),
        String::from_utf8(GOLDEN.to_vec()).unwrap()
    );
}

#[test]
fn unknown_fields_are_ignored_at_every_level() {
    let mut value: Value = serde_json::from_slice(GOLDEN).unwrap();
    value["releaseNotes"] = json!("new, optional");
    value["requires"]["radio"] = json!(2);
    value["core"]["compressedBy"] = json!("zlib-rs");
    value["encodings"][0]["level"] = json!(9);
    value["package"]["image"]["offset"] = json!(0);
    let manifest = OtaManifest::parse_valid(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(manifest, OtaManifest::parse(GOLDEN).unwrap());
}

#[test]
fn an_unknown_encoding_is_skipped_whatever_its_shape() {
    let mut value: Value = serde_json::from_slice(GOLDEN).unwrap();
    value["encodings"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "id": 7, "codec": { "future": [1, 2, 3] }, "core": "elsewhere" }));
    let bytes = serde_json::to_vec(&value).unwrap();
    let manifest = OtaManifest::parse_valid(&bytes).unwrap();
    assert_eq!(manifest.encodings.len(), 2);
    assert!(manifest.encoding(7).unwrap().as_encoding1().is_none());
    assert_eq!(
        manifest.encoding1(),
        OtaManifest::parse(GOLDEN).unwrap().encoding1()
    );
    assert_eq!(
        manifest.files().len(),
        6,
        "an unknown encoding's files are not allowed"
    );

    // An unknown encoding placed FIRST does not hide encoding 1 either.
    let mut value: Value = serde_json::from_slice(GOLDEN).unwrap();
    value["encodings"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({ "id": 2 }));
    let manifest = OtaManifest::parse_valid(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(manifest.encoding1().unwrap().id, ENCODING_DEFLATE_DICT_V1);
}

#[test]
fn another_format_is_refused() {
    let mut value: Value = serde_json::from_slice(GOLDEN).unwrap();
    value["format"] = json!(2);
    assert_eq!(
        OtaManifest::parse(&serde_json::to_vec(&value).unwrap()),
        Err(OtaManifestError::UnsupportedFormat(2))
    );
}

#[test]
fn every_named_file_is_in_the_allowlist_and_the_manifest_is_not() {
    let manifest = OtaManifest::parse(GOLDEN).unwrap();
    let names: Vec<&str> = manifest.files().iter().map(|f| f.file).collect();
    assert_eq!(
        names,
        [
            "core.bin",
            "engine.bin",
            "core.z",
            "engine.z",
            "package.json",
            "fw-esp32c6-merged.bin"
        ]
    );
    assert!(manifest.file(OTA_MANIFEST_FILE).is_none());
    // The fixture's hashes are SHA-256 of `lightplayer fixture <name>`: real
    // digests, so the pin also pins the hex spelling.
    assert_eq!(
        sha256_hex(b"lightplayer fixture core.bin"),
        manifest.core.sha256
    );
}
