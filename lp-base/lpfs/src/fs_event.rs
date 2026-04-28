//! File change tracking types

use lp_model::LpPathBuf;

/// Filesystem version identifier - increments on each filesystem change
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct FsVersion(pub i64);

impl FsVersion {
    pub fn new(id: i64) -> Self {
        Self(id)
    }

    pub fn as_i64(self) -> i64 {
        self.0
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Default for FsVersion {
    fn default() -> Self {
        Self(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fs_version_creation() {
        let version = FsVersion::new(42);
        assert_eq!(version.as_i64(), 42);
    }

    #[test]
    fn test_fs_version_next() {
        let version = FsVersion::new(10);
        let next = version.next();
        assert_eq!(next.as_i64(), 11);
    }

    #[test]
    fn test_fs_version_default() {
        let version = FsVersion::default();
        assert_eq!(version.as_i64(), 0);
    }
}

/// Represents an event caused by a file or directory change
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsChange {
    /// Path affected by the change
    pub path: LpPathBuf,
    /// Type of change
    pub change_type: ChangeType,
}

/// Type of file change
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// File was created
    Create,
    /// File was modified
    Modify,
    /// File was deleted
    Delete,
}
