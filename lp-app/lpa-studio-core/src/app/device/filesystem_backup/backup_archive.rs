//! Build the ZIP: mount the image, mirror its paths, write the manifest.
//!
//! The layout is documented in this module's `README.md` and is a contract —
//! M7's restore and any future selective restore read it. Do not reshape it
//! casually.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;

use super::backup_image::{BackupFile, read_image_files};
use super::backup_manifest::{BACKUP_FORMAT_VERSION, BackupManifest};
// The device's identity stamp is INSIDE lpfs, so it is inside every backup.
// One definition of where it lives, shared with the pull path.
use crate::app::places::DEVICE_IDENTITY_PATH;

/// Where the device's own files live inside the archive.
///
/// Device paths are mirrored VERBATIM under this one root, so recovering a
/// path is a prefix strip rather than a reversal of some renaming scheme.
/// The prefix exists only so `manifest.json` cannot collide with a file the
/// device happened to keep at its filesystem root.
pub const ARCHIVE_FILES_ROOT: &str = "files";

/// The manifest's name at the archive root.
pub const ARCHIVE_MANIFEST_NAME: &str = "manifest.json";

/// A finished backup: the bytes, the name to offer them under, and the
/// manifest that went inside (kept so the UI can narrate without re-reading
/// the archive).
#[derive(Clone, Debug, PartialEq)]
pub struct BackupArchive {
    pub file_name: String,
    pub bytes: Vec<u8>,
    pub manifest: BackupManifest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackupError {
    /// The image did not mount, or a file in it could not be read. On a
    /// board being rescued this is a real possibility, and it must be said
    /// out loud rather than turned into an empty archive.
    Image(String),
    Zip(String),
    Manifest(String),
}

impl core::fmt::Display for BackupError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Image(m) => write!(f, "filesystem image: {m}"),
            Self::Zip(m) => write!(f, "zip: {m}"),
            Self::Manifest(m) => write!(f, "manifest: {m}"),
        }
    }
}

/// What the read told us about where the image came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupSource {
    /// The chip the bootloader named itself as.
    pub chip: Option<String>,
    pub partition_offset: u32,
    pub partition_length: u32,
    /// Human label for the file name — the device's name if Studio knows one.
    pub device_label: Option<String>,
}

/// Mount `image`, walk it, and emit the archive.
///
/// `now_secs` is the app's injected clock (core reads no clocks — see the
/// sans-IO ADR); it stamps the manifest and dates the file name.
pub fn build_backup_archive(
    image: &[u8],
    source: &BackupSource,
    now_secs: f64,
) -> Result<BackupArchive, BackupError> {
    let files =
        read_image_files(image).map_err(|error| BackupError::Image(format!("{error:?}")))?;

    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        captured_at_epoch_seconds: now_secs,
        device_uid: device_uid_from(&files),
        chip: source.chip.clone(),
        partition_offset: source.partition_offset,
        partition_length: source.partition_length,
        block_size: 4096,
        file_count: files.len() as u32,
        total_bytes: files.iter().map(|file| file.bytes.len() as u64).sum(),
    };
    let manifest_json = manifest
        .to_json()
        .map_err(|error| BackupError::Manifest(error.to_string()))?;

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        // Manifest first: a reader streaming the archive learns what it is
        // holding before it reaches a megabyte of device content.
        writer
            .start_file(ARCHIVE_MANIFEST_NAME, options)
            .map_err(|error| BackupError::Zip(error.to_string()))?;
        writer
            .write_all(manifest_json.as_bytes())
            .map_err(|error| BackupError::Zip(error.to_string()))?;
        for file in &files {
            writer
                .start_file(archive_entry_name(&file.path), options)
                .map_err(|error| BackupError::Zip(error.to_string()))?;
            writer
                .write_all(&file.bytes)
                .map_err(|error| BackupError::Zip(error.to_string()))?;
        }
        writer
            .finish()
            .map_err(|error| BackupError::Zip(error.to_string()))?;
    }

    Ok(BackupArchive {
        file_name: backup_file_name(source.device_label.as_deref(), now_secs),
        bytes: cursor.into_inner(),
        manifest,
    })
}

/// `/projects/demo/project.json` → `files/projects/demo/project.json`.
fn archive_entry_name(device_path: &str) -> String {
    format!(
        "{ARCHIVE_FILES_ROOT}/{}",
        device_path.trim_start_matches('/')
    )
}

/// The captured device's uid, read out of the identity stamp the image
/// carries. Absent when the board was never named, or when the stamp does
/// not parse — neither is worth failing a backup over.
fn device_uid_from(files: &[BackupFile]) -> Option<String> {
    let bytes = &files
        .iter()
        .find(|file| file.path == DEVICE_IDENTITY_PATH)?
        .bytes;
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()?
        .get("uid")?
        .as_str()
        .map(str::to_string)
}

/// `lightplayer-backup-porch-sign-2026-07-31.zip`.
///
/// Dated because a user rescuing a board takes more than one, and a browser
/// silently appending `(1)` is not a name anybody can read later.
fn backup_file_name(device_label: Option<&str>, now_secs: f64) -> String {
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

/// `YYYY-MM-DD` in UTC from epoch seconds, via Howard Hinnant's civil-date
/// algorithm. (`device_card.rs` carries a twin that also formats a time of
/// day; if a third appears, extract one.)
fn date_stamp(now_secs: f64) -> String {
    let days = (now_secs as i64).div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    /// The fixture-image gate: build a real littlefs image in memory, back it
    /// up, and read the archive back. No hardware, no device, no provider.
    #[test]
    fn a_fixture_image_round_trips_into_an_archive_with_verbatim_paths() {
        let image = fixture_image(&[
            (
                "/.lp/device.json",
                br#"{"uid":"dev_7pQr5St89uVwXy2C","name":"porch sign"}"#.to_vec(),
            ),
            (
                "/projects/porch/project.json",
                br#"{"kind":"Project","name":"porch"}"#.to_vec(),
            ),
            ("/projects/porch/shader.glsl", b"void main() {}".to_vec()),
            (
                "/lightplayer.json",
                br#"{"startupProject":"porch"}"#.to_vec(),
            ),
        ]);

        let archive = build_backup_archive(&image, &source(), NOW).expect("archive builds");

        let entries = archive_entries(&archive.bytes);
        let names: Vec<&str> = entries.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "manifest.json",
                "files/.lp/device.json",
                "files/lightplayer.json",
                "files/projects/porch/project.json",
                "files/projects/porch/shader.glsl",
            ],
            "device paths mirror verbatim under files/, sorted, manifest first"
        );
        let shader = entries
            .iter()
            .find(|(name, _)| name == "files/projects/porch/shader.glsl")
            .expect("the shader is in the archive");
        assert_eq!(shader.1, b"void main() {}");
    }

    /// The identity hazard M7 has to detect: the captured uid is recorded, so
    /// a restore can tell it is about to clone a device.
    #[test]
    fn the_manifest_records_the_captured_identity_and_partition() {
        let image = fixture_image(&[(
            "/.lp/device.json",
            br#"{"uid":"dev_7pQr5St89uVwXy2C","name":"porch sign"}"#.to_vec(),
        )]);

        let archive = build_backup_archive(&image, &source(), NOW).expect("archive builds");

        assert_eq!(archive.manifest.format_version, BACKUP_FORMAT_VERSION);
        assert_eq!(
            archive.manifest.device_uid.as_deref(),
            Some("dev_7pQr5St89uVwXy2C")
        );
        assert_eq!(archive.manifest.chip.as_deref(), Some("esp32c6"));
        assert_eq!(archive.manifest.partition_offset, 0x0031_0000);
        assert_eq!(archive.manifest.partition_length, 0x000F_0000);
        assert_eq!(archive.manifest.file_count, 1);

        // …and it is IN the archive, not just on the struct.
        let entries = archive_entries(&archive.bytes);
        let (_, manifest_bytes) = entries
            .iter()
            .find(|(name, _)| name == "manifest.json")
            .expect("manifest.json is at the archive root");
        let decoded: BackupManifest =
            serde_json::from_slice(manifest_bytes).expect("manifest parses");
        assert_eq!(decoded, archive.manifest);
    }

    /// A board that was never named has no identity stamp, and that must be
    /// an absent field rather than a failed backup.
    #[test]
    fn an_unstamped_device_backs_up_with_no_uid() {
        let image = fixture_image(&[("/projects/porch/project.json", b"{}".to_vec())]);

        let archive = build_backup_archive(&image, &source(), NOW).expect("archive builds");

        assert_eq!(archive.manifest.device_uid, None);
        assert_eq!(archive.manifest.file_count, 1);
    }

    /// Garbage in the partition means the device's filesystem is not
    /// readable. Saying so beats handing the user an empty archive and
    /// calling it a backup.
    #[test]
    fn an_unmountable_image_fails_rather_than_producing_an_empty_archive() {
        let image = vec![0xFFu8; 4096 * 240];

        let error = build_backup_archive(&image, &source(), NOW).unwrap_err();

        assert!(
            matches!(error, BackupError::Image(_)),
            "expected an image error, got {error}"
        );
    }

    #[test]
    fn the_file_name_carries_the_device_and_the_date() {
        let image = fixture_image(&[("/projects/porch/project.json", b"{}".to_vec())]);
        let mut source = source();
        source.device_label = Some("Porch Sign".to_string());

        let archive = build_backup_archive(&image, &source, NOW).expect("archive builds");

        assert_eq!(
            archive.file_name,
            "lightplayer-backup-porch-sign-2027-01-15.zip"
        );
    }

    #[test]
    fn an_unnamed_device_still_gets_a_readable_file_name() {
        assert_eq!(
            backup_file_name(None, NOW),
            "lightplayer-backup-device-2027-01-15.zip"
        );
        assert_eq!(
            backup_file_name(Some("  "), NOW),
            "lightplayer-backup-device-2027-01-15.zip"
        );
    }

    /// 2027-01-15T04:26:40Z — a date with a two-digit month and day so the
    /// zero-padding is exercised in the other direction by construction.
    const NOW: f64 = 1_800_000_000.0;

    fn source() -> BackupSource {
        BackupSource {
            chip: Some("esp32c6".to_string()),
            partition_offset: 0x0031_0000,
            partition_length: 0x000F_0000,
            device_label: None,
        }
    }

    /// Build a real C6-geometry littlefs image holding `files`.
    fn fixture_image(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
        use littlefs_rust::{Config, Filesystem, RamStorage};

        let block_count = 240;
        let mut config = Config::new(4096, block_count);
        config.cache_size = 512;
        config.lookahead_size = 64;
        let mut storage = RamStorage::new(4096, block_count);
        Filesystem::format(&mut storage, &config).expect("format");
        let fs = Filesystem::mount(storage, config)
            .map_err(|(error, _)| error)
            .expect("mount");
        for (path, bytes) in files {
            let mut prefix = String::new();
            let mut segments = path.trim_start_matches('/').split('/').peekable();
            while let Some(segment) = segments.next() {
                if segments.peek().is_none() {
                    break;
                }
                prefix.push('/');
                prefix.push_str(segment);
                let _ = fs.mkdir(&prefix);
            }
            fs.write_file(path, bytes).expect("seed the fixture image");
        }
        fs.unmount().expect("unmount").data().to_vec()
    }

    fn archive_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a zip archive");
        (0..archive.len())
            .map(|index| {
                let mut file = archive.by_index(index).expect("entry");
                let name = file.name().to_string();
                let mut content = Vec::new();
                file.read_to_end(&mut content).expect("entry bytes");
                (name, content)
            })
            .collect()
    }
}
