//! `project.json` container-manifest access: read fields, patch in place.
//!
//! Post-mitosis the manifest is the closed three-field container
//! ([`lpc_model::ProjectManifest`]: `format`/`uid`/`name`) — no node graph
//! to skirt, so patching is a plain read→modify→write of the canonical
//! form. The strict reader (unknown fields are an error) is what makes the
//! rewrite lossless by construction.

use lpc_history::{PrefixedUid, UidPrefix};
use lpc_model::{AsLpPath, ProjectKind, ProjectManifest};
use lpfs::LpFs;

use super::library_store::LibraryError;

pub const MANIFEST_PATH: &str = "/project.json";

/// The manifest fields the library cares about.
#[derive(Debug, Clone)]
pub struct ManifestFields {
    pub format: Option<u32>,
    pub uid: Option<String>,
    pub name: Option<String>,
    /// Advisory board target (gallery-rework vision D3); `None` for an
    /// untargeted project. Passed through read-only — the library never
    /// writes it (P02 scope: reading only; the generator writes it, P03).
    pub target: Option<String>,
    /// Authored project kind, resolved (`ProjectKind::General` when the
    /// manifest states no `kind`; module authoring unit, P1). Patched
    /// through [`set_kind_and_exports`].
    pub kind: ProjectKind,
    /// The kind's export list, flattened out for callers that just want
    /// the module folder names without matching on `kind`: `Pattern`'s or
    /// `Rig`'s own list, empty for `General`/`Show`.
    pub exports: Vec<String>,
}

/// Display label for a project's authored kind (`"General"` | `"Pattern"`
/// | `"Show"` | `"Rig"`) — shared by the project popup's settings rows and
/// the gallery card (P1 of the module authoring plan).
pub fn kind_label(kind: &ProjectKind) -> &'static str {
    match kind {
        ProjectKind::General => "General",
        ProjectKind::Pattern { .. } => "Pattern",
        ProjectKind::Show => "Show",
        ProjectKind::Rig { .. } => "Rig",
    }
}

pub fn read_manifest(fs: &dyn LpFs) -> Result<ManifestFields, LibraryError> {
    let manifest = read(fs)?;
    let kind = manifest.project_kind();
    let exports = match &kind {
        ProjectKind::Pattern { exports } | ProjectKind::Rig { exports } => exports.clone(),
        ProjectKind::General | ProjectKind::Show => Vec::new(),
    };
    Ok(ManifestFields {
        format: manifest.format,
        uid: manifest.uid,
        name: manifest.name,
        target: manifest.target,
        kind,
        exports,
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

/// Set the project kind and its export list together — the pair changes as
/// one unit (`ProjectKind::General` clears both the `kind` and `exports`
/// keys). Canonical read-modify-write like [`set_name`].
pub fn set_kind_and_exports(fs: &dyn LpFs, kind: ProjectKind) -> Result<(), LibraryError> {
    let mut manifest = read(fs)?;
    manifest.set_kind(kind);
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
  "format": 7,
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

    /// P02: `target` reads through the same seam as `uid`/`name`, and is
    /// `None` when the container omits it — the common, untargeted case.
    #[test]
    fn read_manifest_reads_target_when_present() {
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), MANIFEST).unwrap();
        assert_eq!(read_manifest(&fs).unwrap().target, None);

        let targeted: &[u8] = br#"{
  "format": 4,
  "name": "demo",
  "target": "espressif/esp32-c6-devkitc-1"
}
"#;
        fs.write_file(MANIFEST_PATH.as_path(), targeted).unwrap();
        assert_eq!(
            read_manifest(&fs).unwrap().target.as_deref(),
            Some("espressif/esp32-c6-devkitc-1")
        );
    }

    /// P1: `kind`/`exports` resolve through the same seam as `target`,
    /// defaulting to `General`/empty when the container states no `kind`.
    #[test]
    fn read_manifest_reads_kind_and_exports_when_present() {
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), MANIFEST).unwrap();
        let fields = read_manifest(&fs).unwrap();
        assert_eq!(fields.kind, ProjectKind::General);
        assert_eq!(fields.exports, Vec::<String>::new());

        let pattern: &[u8] = br#"{
  "format": 5,
  "name": "demo",
  "kind": "pattern",
  "exports": ["chase", "sparkle"]
}
"#;
        fs.write_file(MANIFEST_PATH.as_path(), pattern).unwrap();
        let fields = read_manifest(&fs).unwrap();
        assert_eq!(
            fields.kind,
            ProjectKind::Pattern {
                exports: vec!["chase".to_string(), "sparkle".to_string()]
            }
        );
        assert_eq!(
            fields.exports,
            vec!["chase".to_string(), "sparkle".to_string()]
        );
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

    /// P1: `set_kind_and_exports` is the canonical read-modify-write for
    /// the kind/exports pair — it must round-trip byte-identically through
    /// the model's writer, and switching back to `General` must clear both
    /// keys rather than leave a stale `exports: []` behind.
    #[test]
    fn set_kind_and_exports_stays_canonical_and_general_clears_both_keys() {
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), MANIFEST).unwrap();

        set_kind_and_exports(
            &fs,
            ProjectKind::Pattern {
                exports: vec!["chase".to_string()],
            },
        )
        .unwrap();
        let bytes = fs.read_file(MANIFEST_PATH.as_path()).unwrap();
        let text = core::str::from_utf8(&bytes).unwrap();
        let manifest = ProjectManifest::read_json(text)
            .unwrap_or_else(|e| panic!("manifest rejected after patching: {e}\n{text}"));
        assert_eq!(manifest.write_json(), text);
        assert_eq!(
            read_manifest(&fs).unwrap().kind,
            ProjectKind::Pattern {
                exports: vec!["chase".to_string()]
            }
        );

        set_kind_and_exports(&fs, ProjectKind::General).unwrap();
        let fields = read_manifest(&fs).unwrap();
        assert_eq!(fields.kind, ProjectKind::General);
        assert_eq!(fields.exports, Vec::<String>::new());
        let bytes = fs.read_file(MANIFEST_PATH.as_path()).unwrap();
        let text = core::str::from_utf8(&bytes).unwrap();
        assert!(
            !text.contains("kind") && !text.contains("exports"),
            "General must clear both keys: {text}"
        );
    }

    #[test]
    fn invalid_uid_is_rejected() {
        let fs = LpFsMemory::new();
        fs.write_file(
            MANIFEST_PATH.as_path(),
            br#"{
  "format": 7,
  "uid": "garbage"
}
"#,
        )
        .unwrap();
        assert!(ensure_uid(&fs, &[1u8; 16]).is_err());
    }
}
