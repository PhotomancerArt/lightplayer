//! The provenance sidecar `/.lp/meta.json`.
//!
//! Lives inside the package (so it travels on export/pull) but under the
//! reserved `/.lp/` namespace, which the lph1 hash spec excludes — metadata
//! churn never changes a package's content hash.

use lpc_model::AsLpPath;
use lpfs::LpFs;
use serde::{Deserialize, Serialize};

use super::library_store::LibraryError;

pub const META_PATH: &str = "/.lp/meta.json";

/// Where a package came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PackageProvenance {
    /// Created from scratch in this library.
    Created,
    /// Seeded from a bundled source (e.g. `examples/basic`).
    SeededFrom { source: String },
    /// Imported from a zip archive; the archive's own uid, if it had one.
    ImportedZip { original_uid: Option<String> },
    /// Forked from another project's version.
    ForkedFrom {
        parent_project: String,
        parent_version: String,
    },
    /// Adopted from a device carrying a project this library did not know
    /// (connect-as-pull, D8/D11).
    PulledFromDevice {
        device_uid: String,
        device_name: String,
    },
}

/// The sidecar contents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageMeta {
    pub provenance: PackageProvenance,
    /// f64 epoch seconds, caller-supplied.
    pub created_at: f64,
}

pub fn read_meta(fs: &dyn LpFs) -> Result<Option<PackageMeta>, LibraryError> {
    if !fs
        .file_exists(META_PATH.as_path())
        .map_err(|e| LibraryError::Meta(format!("{e}")))?
    {
        return Ok(None);
    }
    let bytes = fs
        .read_file(META_PATH.as_path())
        .map_err(|e| LibraryError::Meta(format!("read meta: {e}")))?;
    // Lenient parse: provenance is a best-effort sidecar, and browser
    // storage can leave it torn or zero-filled (an iOS Safari tab killed
    // mid-flush). Treating damage as "absent" keeps it from bricking
    // project open — the origin event falls back to `Created`.
    match serde_json::from_slice(&bytes) {
        Ok(meta) => Ok(Some(meta)),
        Err(e) => {
            log::warn!("package meta unreadable (treating as absent): parse meta: {e}");
            Ok(None)
        }
    }
}

pub fn write_meta(fs: &dyn LpFs, meta: &PackageMeta) -> Result<(), LibraryError> {
    let bytes = serde_json::to_vec_pretty(meta)
        .map_err(|e| LibraryError::Meta(format!("serialize meta: {e}")))?;
    fs.write_file(META_PATH.as_path(), &bytes)
        .map_err(|e| LibraryError::Meta(format!("write meta: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfs::LpFsMemory;

    #[test]
    fn round_trips_and_is_absent_by_default() {
        let fs = LpFsMemory::new();
        assert!(read_meta(&fs).unwrap().is_none());
        let meta = PackageMeta {
            provenance: PackageProvenance::SeededFrom {
                source: "examples/basic".to_string(),
            },
            created_at: 1700000000.5,
        };
        write_meta(&fs, &meta).unwrap();
        assert_eq!(read_meta(&fs).unwrap().unwrap(), meta);
    }

    #[test]
    fn damaged_meta_reads_as_absent() {
        use lpc_model::AsLpPath;
        let fs = LpFsMemory::new();
        // Zero-filled sidecar: the signature of a browser tab killed
        // mid-flush (size persisted, content lost).
        fs.write_file(META_PATH.as_path(), &[0u8; 64]).unwrap();
        assert!(read_meta(&fs).unwrap().is_none());
        // Empty husk (created, never committed).
        fs.write_file(META_PATH.as_path(), &[]).unwrap();
        assert!(read_meta(&fs).unwrap().is_none());
    }
}
