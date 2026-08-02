//! Turn a raw `lpfs` image read off a device into a ZIP the user can keep.
//!
//! See `README.md` next to this file for the archive layout and the manifest
//! field list — that is the normative description, because the layout is a
//! contract M7 (restore) and any future selective restore have to honor.
//!
//! The split here follows the two halves of the job:
//!
//! - [`backup_image`] mounts the littlefs image and walks it. This is the
//!   half that can fail on a damaged device, and it fails LOUDLY rather than
//!   producing a half-archive.
//! - [`backup_archive`] turns the walk into ZIP bytes and writes the
//!   [`BackupManifest`] beside them.
//!
//! All of it runs in wasm: Studio is Rust, `littlefs-rust` is a pure-Rust
//! port, and the `zip` crate this crate already uses for package export
//! deflates fine there. **No filesystem parsing happens in JS** — JS only ever
//! sees bytes.

mod backup_archive;
mod backup_image;
mod backup_manifest;
mod ui_device_backup;

pub use backup_archive::{
    ARCHIVE_FILES_ROOT, ARCHIVE_MANIFEST_NAME, BackupArchive, BackupError, BackupSource,
    build_backup_archive,
};
pub use backup_image::{BackupFile, read_image_files};
pub use backup_manifest::{BACKUP_FORMAT_VERSION, BackupManifest};
pub use ui_device_backup::UiDeviceBackup;
