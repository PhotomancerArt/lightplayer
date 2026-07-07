//! `project.json` manifest access: read fields, patch in place.
//!
//! Patching never re-serializes the node graph: the manifest is read as a
//! `serde_json::Value`, the field is set, and the whole value is written
//! back — unknown keys and the `nodes` table pass through untouched.

use lpc_history::{PrefixedUid, UidPrefix};
use lpc_model::AsLpPath;
use lpfs::LpFs;

use super::library_store::LibraryError;

pub const MANIFEST_PATH: &str = "/project.json";

/// The manifest fields the library cares about (the rest passes through).
#[derive(Debug, Clone)]
pub struct ManifestFields {
    pub uid: Option<String>,
    pub name: Option<String>,
    pub kind: String,
}

pub fn read_manifest(fs: &dyn LpFs) -> Result<ManifestFields, LibraryError> {
    let value = read_value(fs)?;
    Ok(ManifestFields {
        uid: value
            .get("uid")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        name: value
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        kind: value
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("Project")
            .to_string(),
    })
}

/// Ensure the manifest carries a uid, minting one from `random` if absent.
/// Returns the (existing or minted) uid.
pub fn ensure_uid(fs: &dyn LpFs, random: &[u8; 16]) -> Result<PrefixedUid, LibraryError> {
    let mut value = read_value(fs)?;
    if let Some(existing) = value.get("uid").and_then(|v| v.as_str()) {
        return existing
            .parse()
            .map_err(|e| LibraryError::Manifest(format!("invalid uid {existing:?}: {e}")));
    }
    let uid = PrefixedUid::mint(UidPrefix::Project, random);
    set_field(
        &mut value,
        "uid",
        serde_json::Value::String(uid.to_string()),
    );
    write_value(fs, &value)?;
    Ok(uid)
}

pub fn set_name(fs: &dyn LpFs, name: &str) -> Result<(), LibraryError> {
    let mut value = read_value(fs)?;
    set_field(
        &mut value,
        "name",
        serde_json::Value::String(name.to_string()),
    );
    write_value(fs, &value)
}

fn set_field(value: &mut serde_json::Value, key: &str, field: serde_json::Value) {
    if let serde_json::Value::Object(map) = value {
        map.insert(key.to_string(), field);
    }
}

fn read_value(fs: &dyn LpFs) -> Result<serde_json::Value, LibraryError> {
    let bytes = fs
        .read_file(MANIFEST_PATH.as_path())
        .map_err(|e| LibraryError::Manifest(format!("read project.json: {e}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| LibraryError::Manifest(format!("parse project.json: {e}")))
}

fn write_value(fs: &dyn LpFs, value: &serde_json::Value) -> Result<(), LibraryError> {
    // The slot codec streams and requires the top-level `kind` to precede
    // the variant's fields, and serde_json's map is ordered alphabetically —
    // so emit a canonical key order (matching ProjectDef's declaration
    // order) instead of serializing the map directly.
    const CANONICAL_ORDER: [&str; 4] = ["kind", "format", "uid", "name"];
    let serde_json::Value::Object(map) = value else {
        return Err(LibraryError::Manifest(
            "project.json root must be an object".to_string(),
        ));
    };
    let mut ordered = Vec::new();
    for key in CANONICAL_ORDER {
        if let Some(v) = map.get(key) {
            ordered.push((key.to_string(), v.clone()));
        }
    }
    for (key, v) in map {
        if !CANONICAL_ORDER.contains(&key.as_str()) {
            ordered.push((key.clone(), v.clone()));
        }
    }
    let mut out = String::from("{\n");
    for (index, (key, v)) in ordered.iter().enumerate() {
        let rendered = serde_json::to_string_pretty(v)
            .map_err(|e| LibraryError::Manifest(format!("serialize project.json: {e}")))?;
        // indent nested lines to match to_string_pretty at depth 1
        let indented = rendered.replace('\n', "\n  ");
        out.push_str(&format!(
            "  {}: {indented}",
            serde_json::Value::String(key.clone())
        ));
        if index + 1 < ordered.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push('}');
    out.push('\n');
    fs.write_file(MANIFEST_PATH.as_path(), out.as_bytes())
        .map_err(|e| LibraryError::Manifest(format!("write project.json: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;

    const MANIFEST: &[u8] = br#"{
  "kind": "Project",
  "name": "fluid",
  "nodes": { "clock": { "ref": "./clock.json" }, "custom-key": 7 }
}"#;

    #[test]
    fn ensure_uid_mints_once_and_preserves_unknown_fields() {
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), MANIFEST).unwrap();

        let minted = ensure_uid(&fs, &[7u8; 16]).unwrap();
        let again = ensure_uid(&fs, &[9u8; 16]).unwrap();
        assert_eq!(minted, again, "second call must keep the existing uid");

        let value: serde_json::Value =
            serde_json::from_slice(&fs.read_file(MANIFEST_PATH.as_path()).unwrap()).unwrap();
        assert_eq!(value["uid"].as_str().unwrap(), minted.to_string());
        assert_eq!(value["nodes"]["custom-key"], 7);
        assert_eq!(value["nodes"]["clock"]["ref"], "./clock.json");
    }

    #[test]
    fn set_name_patches_only_the_name() {
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), MANIFEST).unwrap();
        set_name(&fs, "renamed").unwrap();
        let fields = read_manifest(&fs).unwrap();
        assert_eq!(fields.name.as_deref(), Some("renamed"));
        assert_eq!(fields.kind, "Project");
        let value: serde_json::Value =
            serde_json::from_slice(&fs.read_file(MANIFEST_PATH.as_path()).unwrap()).unwrap();
        assert_eq!(value["nodes"]["custom-key"], 7);
    }

    #[test]
    fn patched_manifest_parses_through_the_slot_codec() {
        // The whole point of canonical ordering + the ProjectDef uid slot:
        // a library-patched manifest must load on the runtime.
        let fs = LpFsMemory::new();
        fs.write_file(
            MANIFEST_PATH.as_path(),
            br#"{
  "kind": "Project",
  "format": 1,
  "name": "basic",
  "nodes": { "clock": { "ref": "./clock.json" } }
}"#,
        )
        .unwrap();
        ensure_uid(&fs, &[5u8; 16]).unwrap();
        set_name(&fs, "renamed").unwrap();
        let bytes = fs.read_file(MANIFEST_PATH.as_path()).unwrap();
        let text = core::str::from_utf8(&bytes).unwrap();
        assert!(
            text.trim_start().starts_with("{\n  \"kind\""),
            "kind must lead: {text}"
        );
        lpc_model::NodeDef::from_json_str(text)
            .unwrap_or_else(|e| panic!("codec rejected patched manifest: {e}\n{text}"));
    }

    #[test]
    fn invalid_uid_is_rejected() {
        let fs = LpFsMemory::new();
        fs.write_file(
            MANIFEST_PATH.as_path(),
            br#"{"kind":"Project","uid":"garbage"}"#,
        )
        .unwrap();
        assert!(ensure_uid(&fs, &[1u8; 16]).is_err());
    }
}
