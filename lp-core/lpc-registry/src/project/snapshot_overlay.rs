//! Snapshot-to-overlay helper for test/bootstrap workflows.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpc_model::{ArtifactLocation, AssetOverlay, ProjectOverlay};
use lpfs::{LpFs, LpPath, LpPathBuf};

/// Raw project files keyed by absolute project path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectSnapshot {
    files: BTreeMap<String, Vec<u8>>,
}

impl ProjectSnapshot {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_fs(fs: &dyn LpFs) -> Result<Self, SnapshotError> {
        let paths = fs
            .list_dir(LpPath::new("/"), true)
            .map_err(snapshot_fs_error)?;
        let mut files = BTreeMap::new();
        for path in paths {
            if fs.is_dir(path.as_path()).map_err(snapshot_fs_error)? {
                continue;
            }
            let bytes = fs.read_file(path.as_path()).map_err(snapshot_fs_error)?;
            files.insert(path.as_str().to_string(), bytes);
        }
        Ok(Self { files })
    }

    pub fn insert(&mut self, path: LpPathBuf, bytes: Vec<u8>) {
        self.files.insert(path.as_str().to_string(), bytes);
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn copy_to_memory_fs(&self) -> lpfs::LpFsMemory {
        let mut fs = lpfs::LpFsMemory::new();
        for (path, bytes) in &self.files {
            fs.write_file_mut(LpPath::new(path), bytes).expect("write");
        }
        fs
    }
}

/// Build an artifact-body overlay that transforms `base` files into `target`.
pub fn derive_overlay_between_snapshots(
    base: &ProjectSnapshot,
    target: &ProjectSnapshot,
) -> ProjectOverlay {
    let mut overlay = ProjectOverlay::new();

    for (path, bytes) in target.iter() {
        if base.get(path) != Some(bytes) {
            overlay.set_artifact_body(
                ArtifactLocation::file(path),
                AssetOverlay::ReplaceBody(bytes.to_vec()),
            );
        }
    }
    for (path, _) in base.iter() {
        if target.get(path).is_none() {
            overlay.set_artifact_body(ArtifactLocation::file(path), AssetOverlay::Delete);
        }
    }

    overlay
}

/// Snapshot helper failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotError {
    Fs { message: String },
}

fn snapshot_fs_error(err: lpfs::FsError) -> SnapshotError {
    SnapshotError::Fs {
        message: err.to_string(),
    }
}
