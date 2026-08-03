//! `project.json` container-manifest access: read fields, patch in place.
//!
//! Post-mitosis the manifest is the closed three-field container
//! ([`lpc_model::ProjectManifest`]: `format`/`uid`/`name`) — no node graph
//! to skirt, so patching is a plain read→modify→write of the canonical
//! form. The strict reader (unknown fields are an error) is what makes the
//! rewrite lossless by construction.

use lpc_history::{PrefixedUid, UidPrefix};
use lpc_model::{AsLpPath, ProjectManifest};
use lpfs::LpFs;

use super::library_store::LibraryError;

pub const MANIFEST_PATH: &str = "/project.json";

/// The manifest fields the library cares about.
#[derive(Debug, Clone)]
pub struct ManifestFields {
    pub format: Option<u32>,
    pub uid: Option<String>,
    pub name: Option<String>,
}

pub fn read_manifest(fs: &dyn LpFs) -> Result<ManifestFields, LibraryError> {
    let manifest = read(fs)?;
    Ok(ManifestFields {
        format: manifest.format,
        uid: manifest.uid,
        name: manifest.name,
    })
}

/// Ensure the manifest carries a uid, minting one from `random` if absent.
/// Returns the (existing or minted) uid.
pub fn ensure_uid(fs: &dyn LpFs, random: &[u8; 16]) -> Result<PrefixedUid, LibraryError> {
    let mut manifest = read(fs)?;
    if let Some(existing) = &manifest.uid {
        return existing
            .parse()
            .map_err(|e| LibraryError::Manifest(format!("invalid uid {existing:?}: {e}")));
    }
    let uid = PrefixedUid::mint(UidPrefix::Project, random);
    manifest.uid = Some(uid.to_string());
    write(fs, &manifest)?;
    Ok(uid)
}

pub fn set_name(fs: &dyn LpFs, name: &str) -> Result<(), LibraryError> {
    let mut manifest = read(fs)?;
    manifest.name = Some(name.to_string());
    write(fs, &manifest)
}

fn read(fs: &dyn LpFs) -> Result<ProjectManifest, LibraryError> {
    let bytes = fs
        .read_file(MANIFEST_PATH.as_path())
        .map_err(|e| LibraryError::Manifest(format!("read project.json: {e}")))?;
    let text = core::str::from_utf8(&bytes)
        .map_err(|_| LibraryError::Manifest("project.json is not UTF-8".to_string()))?;
    ProjectManifest::read_json(text)
        .map_err(|e| LibraryError::Manifest(format!("parse project.json: {e}")))
}

fn write(fs: &dyn LpFs, manifest: &ProjectManifest) -> Result<(), LibraryError> {
    fs.write_file(MANIFEST_PATH.as_path(), manifest.write_json().as_bytes())
        .map_err(|e| LibraryError::Manifest(format!("write project.json: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;

    const MANIFEST: &[u8] = br#"{
  "format": 3,
  "name": "demo"
}
"#;

    #[test]
    fn read_manifest_reads_uid_and_name() {
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), MANIFEST).unwrap();
        let fields = read_manifest(&fs).unwrap();
        assert_eq!(fields.uid, None);
        assert_eq!(fields.name.as_deref(), Some("demo"));
    }

    #[test]
    fn ensure_uid_mints_once_and_is_stable() {
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), MANIFEST).unwrap();
        let minted = ensure_uid(&fs, &[7u8; 16]).unwrap();
        let again = ensure_uid(&fs, &[9u8; 16]).unwrap();
        assert_eq!(minted, again, "second call must keep the existing uid");
        let fields = read_manifest(&fs).unwrap();
        assert_eq!(fields.uid.as_deref(), Some(minted.to_string().as_str()));
    }

    #[test]
    fn set_name_patches_only_the_name() {
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), MANIFEST).unwrap();
        let minted = ensure_uid(&fs, &[7u8; 16]).unwrap();
        set_name(&fs, "renamed").unwrap();
        let fields = read_manifest(&fs).unwrap();
        assert_eq!(fields.name.as_deref(), Some("renamed"));
        assert_eq!(fields.uid.as_deref(), Some(minted.to_string().as_str()));
    }

    #[test]
    fn patched_manifest_stays_canonical() {
        // The whole point of the closed manifest: a library-patched
        // project.json must round-trip byte-identically through the model's
        // canonical writer, so library patches never churn project diffs.
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), MANIFEST).unwrap();
        ensure_uid(&fs, &[5u8; 16]).unwrap();
        set_name(&fs, "renamed").unwrap();
        let bytes = fs.read_file(MANIFEST_PATH.as_path()).unwrap();
        let text = core::str::from_utf8(&bytes).unwrap();
        let manifest = lpc_model::ProjectManifest::read_json(text)
            .unwrap_or_else(|e| panic!("manifest rejected after patching: {e}\n{text}"));
        assert_eq!(manifest.write_json(), text);
        assert_eq!(manifest.format, Some(lpc_model::PROJECT_FORMAT_VERSION));
    }

    #[test]
    fn invalid_uid_is_rejected() {
        let fs = LpFsMemory::new();
        fs.write_file(
            MANIFEST_PATH.as_path(),
            br#"{
  "format": 3,
  "uid": "garbage"
}
"#,
        )
        .unwrap();
        assert!(ensure_uid(&fs, &[1u8; 16]).is_err());
    }
}
