//! File change tracking types

use crate::LpPathBuf;

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

/// Kind of filesystem event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsEventKind {
    /// File was created
    Create,
    /// File was modified
    Modify,
    /// File was deleted
    Delete,
}

/// Represents an event caused by a file or directory change
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsEvent {
    /// Path affected by the change
    pub path: LpPathBuf,
    /// Kind of change
    pub kind: FsEventKind,
}

#[deprecated(note = "renamed to FsEventKind")]
pub type ChangeType = FsEventKind;

#[deprecated(note = "renamed to FsEvent")]
pub type FsChange = FsEvent;
