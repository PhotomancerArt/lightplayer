//! In-memory project file snapshots for diffing.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use lpfs::{LpFs, LpPath, LpPathBuf};

use super::DiffError;

/// All project files keyed by absolute path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectSnapshot {
    files: BTreeMap<String, Vec<u8>>,
}

impl ProjectSnapshot {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_fs(fs: &dyn LpFs) -> Result<Self, DiffError> {
        let paths = fs
            .list_dir(LpPath::new("/"), true)
            .map_err(|err| DiffError::Fs {
                message: alloc::format!("{err}"),
            })?;
        let mut files = BTreeMap::new();
        for path in paths {
            if fs.is_dir(path.as_path()).map_err(|err| DiffError::Fs {
                message: alloc::format!("{err}"),
            })? {
                continue;
            }
            let bytes = fs.read_file(path.as_path()).map_err(|err| DiffError::Fs {
                message: alloc::format!("{err}"),
            })?;
            files.insert(path.as_str().to_string(), bytes);
        }
        Ok(Self { files })
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn insert(&mut self, path: LpPathBuf, bytes: Vec<u8>) {
        self.files.insert(path.as_str().to_string(), bytes);
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.files.get(path).map(|bytes| bytes.as_slice())
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
    }

    pub fn copy_to_memory_fs(&self) -> lpfs::LpFsMemory {
        let mut fs = lpfs::LpFsMemory::new();
        for (path, bytes) in &self.files {
            fs.write_file_mut(LpPath::new(path), bytes).expect("write");
        }
        fs
    }
}
