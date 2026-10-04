//! The device backup archive (format 2): a board's whole filesystem as a ZIP.
//!
//! Normative layout: `README.md` beside this file. Shared by Studio (the
//! backup every layout migration stores in the browser first) and `lp-cli`
//! (`hardware lpfs save | migrate | restore`).

pub mod backup_archive;
pub mod backup_manifest;

pub use backup_archive::{
    ARCHIVE_FILES_ROOT, ARCHIVE_MANIFEST_NAME, ArchiveError, backup_file_name, read_archive,
    write_archive,
};
pub use backup_manifest::{BACKUP_FORMAT_VERSION, BackupManifest, BackupPurpose};
