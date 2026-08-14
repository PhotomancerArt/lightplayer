//! Building the [`SidecarMeta`] that rides along with every commit.
//!
//! The server never opens a project's tree to render a listing (D3), so
//! everything a listing or an OG card shows has to be computed here and
//! pushed. Two of the three fields come straight off the container manifest;
//! the third does not exist yet.
//!
//! # `preview_png` is `None`, deliberately
//!
//! There is no readable frame to capture at save time. The GPU tier
//! transfers its `OffscreenCanvas` into a worker (`preview_worker.rs`), so
//! the main thread cannot read it back, and reaching the CPU tier's canvas
//! for an arbitrary project — including the closed ones the sign-in sweep
//! publishes — would mean new worker-protocol surface. That was explicitly
//! out of this slice's timebox. See `docs/debt/sidecar-preview-capture.md`.

use lpa_studio_core::app::library::LibraryError;
use lpa_studio_core::app::library::package_manifest::read_manifest;
use lpc_cloud_api::SidecarMeta;
use lpc_cloud_api::share_link::slugify;
use lpfs::LpFs;

/// What a project calls itself and what format it is written in — the two
/// facts the cloud row is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIdentity {
    /// The display name, as the manifest has it.
    pub name: String,
    /// The container format version, or `0` when the manifest does not
    /// declare one. Zero is honest rather than flattering: a package with no
    /// `format` key is pre-floor and unloadable by this build, and claiming
    /// the current version would tell the service it can render something it
    /// cannot.
    pub format_version: u32,
}

impl ProjectIdentity {
    /// The cosmetic half of the project's share address (P1/D11).
    ///
    /// Empty for a name with nothing sluggable in it (emoji, CJK) — callers
    /// pass it through as-is, and `canonical_path` renders the bare-uid form.
    pub fn cloud_slug(&self) -> String {
        slugify(&self.name)
    }

    /// This project's display metadata for a publish or a push.
    pub fn sidecar(&self) -> SidecarMeta {
        SidecarMeta {
            name: self.name.clone(),
            format_version: self.format_version,
            // See the module docs: no readable frame source, by design.
            preview_png: None,
        }
    }
}

/// Read a project's identity out of its working copy's `project.json`.
///
/// `fallback_name` is the library's own label for the package (its slug),
/// used only when the manifest carries no name — which `install_package`
/// makes sure never happens for anything it created, but an imported
/// package is somebody else's bytes.
pub fn read_identity(
    package_fs: &dyn LpFs,
    fallback_name: &str,
) -> Result<ProjectIdentity, LibraryError> {
    let fields = read_manifest(package_fs)?;
    Ok(ProjectIdentity {
        name: fields
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| fallback_name.to_string()),
        format_version: fields.format.unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{AsLpPath, PROJECT_FORMAT_VERSION, ProjectManifest};
    use lpfs::LpFsMemory;

    const MANIFEST_PATH: &str = "/project.json";

    #[test]
    fn identity_comes_from_the_manifest() {
        let fs = package(r#"{"format": 5, "name": "Zook Dome"}"#);
        let identity = read_identity(&fs, "2026-08-07-1200-zook-dome").unwrap();
        assert_eq!(identity.name, "Zook Dome");
        assert_eq!(identity.format_version, 5);
    }

    /// The sidecar the service stores: real name, real format, no preview.
    #[test]
    fn the_sidecar_carries_name_and_format() {
        let fs = package(r#"{"format": 5, "name": "Zook Dome"}"#);
        let sidecar = read_identity(&fs, "fallback").unwrap().sidecar();
        assert_eq!(
            sidecar,
            SidecarMeta {
                name: "Zook Dome".to_string(),
                format_version: 5,
                preview_png: None,
            }
        );
    }

    /// The format the producer reports is the one the package declares, which
    /// for anything this build wrote is the constant `lpa-upgrade` migrates
    /// to — the same source of truth, read through the manifest rather than
    /// hard-coded here.
    #[test]
    fn a_freshly_written_manifest_reports_the_current_format() {
        let fs = LpFsMemory::new();
        let manifest = ProjectManifest::new_current("Fresh");
        fs.write_file(MANIFEST_PATH.as_path(), manifest.write_json().as_bytes())
            .unwrap();
        assert_eq!(
            read_identity(&fs, "fallback").unwrap().format_version,
            PROJECT_FORMAT_VERSION
        );
    }

    /// A manifest with no `format` is pre-floor; `0` says "unknown" rather
    /// than claiming a version the content is not written in.
    #[test]
    fn a_manifest_without_a_format_reports_zero() {
        let fs = package(r#"{"name": "Ancient"}"#);
        assert_eq!(read_identity(&fs, "fallback").unwrap().format_version, 0);
    }

    #[test]
    fn a_nameless_manifest_falls_back_to_the_library_label() {
        for json in [r#"{"format": 5}"#, r#"{"format": 5, "name": "  "}"#] {
            let fs = package(json);
            assert_eq!(read_identity(&fs, "my-package").unwrap().name, "my-package");
        }
    }

    #[test]
    fn the_cloud_slug_is_the_slugified_display_name() {
        let fs = package(r#"{"format": 5, "name": "Yona's \"radiance dome\" Doors"}"#);
        assert_eq!(
            read_identity(&fs, "fallback").unwrap().cloud_slug(),
            "yonas-radiance-dome-doors"
        );
    }

    /// A name with nothing sluggable yields the empty slug, which the URL
    /// grammar renders as the bare-uid form rather than an invented slug.
    #[test]
    fn an_unsluggable_name_yields_an_empty_slug() {
        let fs = package(r#"{"format": 5, "name": "光の橋"}"#);
        assert_eq!(read_identity(&fs, "fallback").unwrap().cloud_slug(), "");
    }

    #[test]
    fn a_package_without_a_manifest_has_no_identity() {
        assert!(read_identity(&LpFsMemory::new(), "fallback").is_err());
    }

    fn package(manifest_json: &str) -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file(MANIFEST_PATH.as_path(), manifest_json.as_bytes())
            .unwrap();
        fs
    }
}
