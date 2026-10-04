//! The archive itself: a tree and a manifest to ZIP bytes, and back.
//!
//! The layout is documented in this module's `README.md` and is a contract —
//! restores read it. Do not reshape it casually.

use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;

use super::backup_manifest::{BACKUP_FORMAT_VERSION, BackupManifest};
use crate::layout_migration::lpfs_tree::{LpfsNode, LpfsTree};

/// Where the device's own files live inside the archive.
///
/// Device paths are mirrored VERBATIM under this one root, so recovering a
/// path is a prefix strip rather than a reversal of some renaming scheme.
/// The prefix exists only so `manifest.json` cannot collide with a file the
/// device happened to keep at its filesystem root.
pub const ARCHIVE_FILES_ROOT: &str = "files";

/// The manifest's name at the archive root.
pub const ARCHIVE_MANIFEST_NAME: &str = "manifest.json";

/// Why an archive could not be written or read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArchiveError {
    Zip(String),
    Manifest(String),
    /// A `formatVersion` this build does not read (version and refuse).
    UnsupportedVersion(u32),
    /// No `manifest.json` at the root.
    MissingManifest,
    /// An entry outside `files/`, absolute, or climbing with `..`.
    UnsafePath(String),
}

impl core::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Zip(m) => write!(f, "zip: {m}"),
            Self::Manifest(m) => write!(f, "manifest: {m}"),
            Self::UnsupportedVersion(v) => write!(
                f,
                "backup format {v} is not one this build reads (it reads {BACKUP_FORMAT_VERSION})"
            ),
            Self::MissingManifest => f.write_str("not a LightPlayer backup: no manifest.json"),
            Self::UnsafePath(p) => write!(f, "refusing archive entry {p:?}"),
        }
    }
}

/// Write `tree` and `manifest` as archive bytes.
///
/// Byte-stable: the manifest first, then every entry in path order, with no
/// timestamps of their own (the `zip` crate's fixed default) — the same
/// board state produces the same archive twice.
pub fn write_archive(tree: &LpfsTree, manifest: &BackupManifest) -> Result<Vec<u8>, ArchiveError> {
    let zip_error = |error: zip::result::ZipError| ArchiveError::Zip(error.to_string());
    let io_error = |error: std::io::Error| ArchiveError::Zip(error.to_string());
    let manifest_json = manifest
        .to_json()
        .map_err(|error| ArchiveError::Manifest(error.to_string()))?;
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file(ARCHIVE_MANIFEST_NAME, options)
            .map_err(zip_error)?;
        writer
            .write_all(manifest_json.as_bytes())
            .map_err(io_error)?;
        for (path, node) in &tree.entries {
            let name = format!("{ARCHIVE_FILES_ROOT}/{}", path.trim_start_matches('/'));
            match node {
                LpfsNode::Dir => writer
                    .add_directory(format!("{name}/"), options)
                    .map_err(zip_error)?,
                LpfsNode::File(bytes) => {
                    writer.start_file(name, options).map_err(zip_error)?;
                    writer.write_all(bytes).map_err(io_error)?;
                }
            }
        }
        writer.finish().map_err(zip_error)?;
    }
    Ok(cursor.into_inner())
}

/// Read archive bytes back to their manifest and tree. Refuses any format
/// version but [`BACKUP_FORMAT_VERSION`] and any entry that would land
/// outside the device's filesystem.
pub fn read_archive(bytes: &[u8]) -> Result<(BackupManifest, LpfsTree), ArchiveError> {
    let zip_error = |error: zip::result::ZipError| ArchiveError::Zip(error.to_string());
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;

    let manifest: BackupManifest = {
        let mut entry = archive
            .by_name(ARCHIVE_MANIFEST_NAME)
            .map_err(|_| ArchiveError::MissingManifest)?;
        let mut text = Vec::new();
        entry
            .read_to_end(&mut text)
            .map_err(|error| ArchiveError::Zip(error.to_string()))?;
        // The version first, so a v1 or v3 archive is named as such rather
        // than as a missing field.
        let value: serde_json::Value = serde_json::from_slice(&text)
            .map_err(|error| ArchiveError::Manifest(error.to_string()))?;
        let version = value
            .get("formatVersion")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| ArchiveError::Manifest("no formatVersion".to_string()))?;
        if version != u64::from(BACKUP_FORMAT_VERSION) {
            return Err(ArchiveError::UnsupportedVersion(version as u32));
        }
        serde_json::from_value(value).map_err(|error| ArchiveError::Manifest(error.to_string()))?
    };

    let mut tree = LpfsTree::default();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(zip_error)?;
        let name = entry.name().to_string();
        if name == ARCHIVE_MANIFEST_NAME {
            continue;
        }
        let device_path = device_path_of(&name)?;
        if entry.is_dir() {
            tree.add_dir(&device_path);
        } else {
            let mut content = Vec::new();
            entry
                .read_to_end(&mut content)
                .map_err(|error| ArchiveError::Zip(error.to_string()))?;
            tree.entries.push((device_path, LpfsNode::File(content)));
        }
    }
    tree.entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((manifest, tree))
}

/// `files/projects/demo/project.json` → `/projects/demo/project.json`, or a
/// refusal for anything that would escape the device's filesystem.
fn device_path_of(name: &str) -> Result<String, ArchiveError> {
    let unsafe_path = || ArchiveError::UnsafePath(name.to_string());
    let rest = name
        .strip_prefix(ARCHIVE_FILES_ROOT)
        .and_then(|rest| rest.strip_prefix('/'))
        .ok_or_else(unsafe_path)?;
    let rest = rest.trim_end_matches('/');
    if rest.is_empty()
        || rest.starts_with('/')
        || rest.contains('\\')
        || rest
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(unsafe_path());
    }
    Ok(format!("/{rest}"))
}

/// `lightplayer-backup-porch-sign-2026-07-31-1415.zip`: the board and the
/// UTC minute it was taken, so two backups of one board never collide and a
/// browser's `(1)` suffix is never the only distinction.
pub fn backup_file_name(device_label: Option<&str>, now_secs: f64) -> String {
    let label = device_label
        .map(slugify_label)
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| "device".to_string());
    format!("lightplayer-backup-{label}-{}.zip", date_stamp(now_secs))
}

fn slugify_label(label: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
}

/// `YYYY-MM-DD-HHMM` in UTC from epoch seconds (Howard Hinnant's civil-date
/// algorithm).
fn date_stamp(now_secs: f64) -> String {
    let secs = now_secs as i64;
    let days = secs.div_euclid(86_400);
    let minute_of_day = secs.rem_euclid(86_400) / 60;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}-{:02}{:02}",
        minute_of_day / 60,
        minute_of_day % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_migration::device_backup_archive::backup_manifest::BackupPurpose;

    /// 2027-01-15T08:00:00Z.
    const NOW: f64 = 1_800_000_000.0;

    fn tree() -> LpfsTree {
        let mut tree = LpfsTree::from_files([
            (
                "/.lp/device.json".to_string(),
                br#"{"uid":"dev7pQr5St89uVwXy2C","name":"porch sign"}"#.to_vec(),
            ),
            ("/projects/porch/project.json".to_string(), b"{}".to_vec()),
            ("/projects/porch/empty.bin".to_string(), Vec::new()),
            ("/hardware.json".to_string(), vec![9u8; 5000]),
        ]);
        tree.add_dir("/projects/drafts");
        tree
    }

    fn manifest(version: u32) -> BackupManifest {
        let tree = tree();
        BackupManifest {
            format_version: version,
            captured_at_epoch_seconds: NOW,
            device_uid: tree.device_uid(),
            chip: Some("esp32c6".to_string()),
            base_mac: Some("10:bd:a3:b0:8e:30".to_string()),
            partition_offset: 0x31_0000,
            partition_length: 0xF_0000,
            target_partition_offset: Some(0x35_0000),
            target_partition_length: Some(0xB_0000),
            block_size: 4096,
            file_count: tree.file_count(),
            total_bytes: tree.total_bytes(),
            purpose: BackupPurpose::LayoutMigration,
        }
    }

    #[test]
    fn a_tree_round_trips_with_empty_dirs_and_empty_files() {
        let bytes = write_archive(&tree(), &manifest(2)).unwrap();
        let (back_manifest, back_tree) = read_archive(&bytes).unwrap();
        assert_eq!(back_manifest, manifest(2));
        assert_eq!(back_tree, tree());
    }

    #[test]
    fn the_same_board_makes_the_same_bytes_twice_with_the_manifest_first() {
        let a = write_archive(&tree(), &manifest(2)).unwrap();
        let b = write_archive(&tree(), &manifest(2)).unwrap();
        assert_eq!(a, b);
        let archive = zip::ZipArchive::new(Cursor::new(a.as_slice())).unwrap();
        assert_eq!(archive.name_for_index(0), Some(ARCHIVE_MANIFEST_NAME));
        assert_eq!(archive.name_for_index(1), Some("files/.lp/"));
    }

    #[test]
    fn version_one_and_version_three_are_refused_by_name() {
        for version in [1, 3] {
            let bytes = write_archive(&tree(), &manifest(version)).unwrap();
            assert_eq!(
                read_archive(&bytes).unwrap_err(),
                ArchiveError::UnsupportedVersion(version)
            );
        }
    }

    #[test]
    fn an_entry_that_escapes_the_filesystem_is_refused() {
        for bad in [
            "files/../etc/passwd",
            "files//abs",
            "other/x",
            "files/a/./b",
            "files",
        ] {
            assert!(
                matches!(device_path_of(bad), Err(ArchiveError::UnsafePath(_))),
                "{bad} should be refused"
            );
        }
        assert_eq!(device_path_of("files/a/b.json").unwrap(), "/a/b.json");
        assert_eq!(device_path_of("files/a/").unwrap(), "/a");

        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            let options = SimpleFileOptions::default();
            writer.start_file(ARCHIVE_MANIFEST_NAME, options).unwrap();
            writer
                .write_all(manifest(2).to_json().unwrap().as_bytes())
                .unwrap();
            writer.start_file("files/../escape", options).unwrap();
            writer.write_all(b"x").unwrap();
            writer.finish().unwrap();
        }
        assert!(matches!(
            read_archive(&cursor.into_inner()),
            Err(ArchiveError::UnsafePath(_))
        ));
    }

    #[test]
    fn the_file_name_carries_the_board_and_the_minute() {
        assert_eq!(
            backup_file_name(Some("Porch Sign"), NOW),
            "lightplayer-backup-porch-sign-2027-01-15-0800.zip"
        );
        assert_eq!(
            backup_file_name(None, NOW),
            "lightplayer-backup-device-2027-01-15-0800.zip"
        );
    }
}
