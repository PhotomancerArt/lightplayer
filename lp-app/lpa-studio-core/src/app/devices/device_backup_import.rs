//! Restoring a board from a backup ZIP this browser never stored itself
//! (the C6 repartition's Decision 11 — plan P01, `lp2025/2026-10-05-1903-
//! wifi-link-c6`): a user who kept the file Studio offered as a download
//! can put it back even when this browser's own copy of it is gone. Past
//! that interruption, the backup is the only copy of the board's files.
//!
//! The file itself is not a fillable offer parameter: there is no offer
//! param kind for raw bytes (`OfferParamKind` is `Choice` / `Text` /
//! `Toggle`), and a browser only opens a file picker from a real click. So
//! [`DeviceRestoreFromFileOp`] — published at `devices/<board>/restore-
//! from-file` — carries no bytes at all; it exists so the app agent can SEE
//! that a restore from an outside file is possible here
//! (`needs_user_activation` hands it to the user as a card, same as
//! `DeviceBackupOp`'s download). The web shell intercepts the click and
//! opens a file picker itself, exactly as the project library's zip import
//! does; once a person has actually chosen a file, it dispatches
//! [`DeviceRestoreFromFileDataOp`] — a second, unpublished op nothing but a
//! real file read can produce.
//!
//! [`check_backup_file`] is the one question a mismatched backup ever asks
//! ("ease over ceremony": no second confirmation stacks on top of it).
//! It is exported so the web shell can ask it, in a native confirm, BEFORE
//! dispatching — the same `window.confirm` idiom `unsaved_gate.rs` already
//! uses for a quick sanity check ahead of a destructive action. The dispatch
//! handler below does not re-ask: it trusts the shell asked, and proceeds.

use core::any::Any;

use lpa_devices::DeviceId;
use lpa_link::layout_migration::device_backup_archive::read_archive;
use lpa_link::normalize_base_mac;

use crate::{ActionClass, ActionConfirmation, ActionMeta, ActionPriority, ControllerOp};

/// Bytes riding a restore-from-file action. `Debug` prints the count, not
/// the archive — it carries the board's secrets (its access keys, its
/// Wi-Fi password), like any backup archive.
#[derive(Clone, Eq, PartialEq)]
pub struct BackupFileBytes(pub Vec<u8>);

impl core::fmt::Debug for BackupFileBytes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "BackupFileBytes({} bytes)", self.0.len())
    }
}

/// The offer: discoverable, never pressable by an app agent. See the module
/// doc for why it carries no bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceRestoreFromFileOp {
    pub device: DeviceId,
}

impl DeviceRestoreFromFileOp {
    /// Routed by `StudioController` directly, like the roster's own ops.
    pub const NODE_ID: &'static str = "studio|device-restore-from-file";

    /// This offer as a dispatchable [`UiAction`](crate::UiAction). Pressing
    /// it for real means opening a file picker, which only the web shell's
    /// own click handler can do — see the module doc.
    pub fn action_for(device: DeviceId) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), Self { device })
    }
}

impl ControllerOp for DeviceRestoreFromFileOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Restore from a backup file…",
            "Pick a backup .zip from your computer and replace this board's files with it.",
            ActionPriority::Secondary,
        )
        .with_icon("upload")
        // A file picker wants a real click, same reasoning as the download.
        .needs_user_activation()
        .lasting(ActionConfirmation::new(
            "Replace this board's files with a backup file?",
            "Studio checks the file, then replaces whatever is on this board now with what it \
             holds. The file stays on your computer.",
            "restore",
        ))
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Passive {
            deadline: crate::PASSIVE_REFRESH_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

/// The real import: what only a file read can produce. Not published
/// anywhere (no offer-param kind carries raw bytes) — the web shell builds
/// and dispatches it directly, the same way the project library dispatches
/// its own zip import (`HomeOp::ImportZip`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceRestoreFromFileDataOp {
    pub device: DeviceId,
    pub file_name: String,
    pub bytes: BackupFileBytes,
}

impl DeviceRestoreFromFileDataOp {
    pub const NODE_ID: &'static str = "studio|device-restore-from-file-data";

    fn action_for(device: DeviceId, file_name: String, bytes: Vec<u8>) -> crate::UiAction {
        crate::UiAction::from_op(
            crate::ControllerId::new(Self::NODE_ID),
            Self {
                device,
                file_name,
                bytes: BackupFileBytes(bytes),
            },
        )
    }
}

/// Build the dispatchable action for a file Studio's web shell just read.
///
/// Named apart from the usual `<Op>::action_for` so a web call site reads
/// as a plain function call rather than the pattern
/// `scripts/check-web-actions.py` exists to catch: every OTHER action the
/// web builds for itself is one that COULD be published and simply has
/// not moved there yet, which is what the ratchet is pressure for. This
/// one structurally cannot be (the module doc explains why), the same
/// shape as the project library's own `HomeOp::ImportZip` (dispatched
/// through `package_card::home_action`, not through a counted
/// constructor) — so core still owns the construction, under a name the
/// ratchet does not need to watch.
pub fn device_restore_from_file_action(
    device: DeviceId,
    file_name: String,
    bytes: Vec<u8>,
) -> crate::UiAction {
    DeviceRestoreFromFileDataOp::action_for(device, file_name, bytes)
}

impl ControllerOp for DeviceRestoreFromFileDataOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Restore from a backup file",
            "Replace this board's files with the backup file you picked.",
            ActionPriority::Secondary,
        )
        .with_icon("upload")
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Foreground {
            deadline: crate::PROJECT_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

/// What a file the user just picked says before anything is written:
/// `Err` is the refusal, in the archive's own words (not a LightPlayer
/// backup, a format this Studio doesn't read, an unsafe entry); `Ok(Some
/// (…))` is the one question a backup naming a different board ever asks.
/// `Ok(None)` means proceed — either the board's MAC is not known yet (a
/// board that has never said hello), or the backup and the board agree.
pub fn check_backup_file(
    bytes: &[u8],
    current_base_mac: Option<&str>,
) -> Result<Option<String>, String> {
    let (manifest, _tree) = read_archive(bytes).map_err(|error| error.to_string())?;
    let mismatch = match (manifest.base_mac.as_deref(), current_base_mac) {
        (Some(archive_mac), Some(board_mac))
            if normalize_base_mac(archive_mac).as_deref()
                != normalize_base_mac(board_mac).as_deref() =>
        {
            Some(format!(
                "This backup is from {}, not {}.",
                short_board_label(archive_mac),
                short_board_label(board_mac),
            ))
        }
        _ => None,
    };
    Ok(mismatch)
}

/// `"10:bd:a3:b0:8e:30"` → `"LP-8e30"`: a short, stable name for a board
/// this browser has no live title for (the archive names a board that is
/// not the one connected).
fn short_board_label(mac: &str) -> String {
    let hex: String = mac.chars().filter(char::is_ascii_hexdigit).collect();
    let tail = if hex.len() >= 4 {
        &hex[hex.len() - 4..]
    } else {
        hex.as_str()
    };
    format!("LP-{}", tail.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_link::layout_migration::device_backup_archive::{
        BACKUP_FORMAT_VERSION, BackupManifest, BackupPurpose, write_archive,
    };
    use lpa_link::layout_migration::lpfs_tree::LpfsTree;

    fn archive_bytes(base_mac: Option<&str>) -> Vec<u8> {
        let tree = LpfsTree::from_files([("/hardware.json".to_string(), b"{}".to_vec())]);
        let manifest = BackupManifest {
            format_version: BACKUP_FORMAT_VERSION,
            captured_at_epoch_seconds: 1.0,
            device_uid: None,
            chip: Some("esp32c6".to_string()),
            base_mac: base_mac.map(str::to_string),
            partition_offset: 0,
            partition_length: 1,
            target_partition_offset: None,
            target_partition_length: None,
            block_size: 4096,
            file_count: tree.file_count(),
            total_bytes: tree.total_bytes(),
            purpose: BackupPurpose::Backup,
        };
        write_archive(&tree, &manifest).unwrap()
    }

    #[test]
    fn a_matching_or_unknown_board_asks_nothing() {
        let bytes = archive_bytes(Some("10:bd:a3:b0:8e:30"));
        assert_eq!(
            check_backup_file(&bytes, Some("10:BD:A3:B0:8E:30")).unwrap(),
            None,
            "the same board, differently cased, is not a mismatch"
        );
        assert_eq!(
            check_backup_file(&bytes, None).unwrap(),
            None,
            "a board with no known MAC yet asks nothing"
        );
    }

    #[test]
    fn a_different_board_names_both_in_one_question() {
        let bytes = archive_bytes(Some("10:bd:a3:b0:8e:30"));
        let message = check_backup_file(&bytes, Some("60:55:f9:0a:0b:0c"))
            .unwrap()
            .expect("a mismatch");
        assert_eq!(message, "This backup is from LP-8e30, not LP-0b0c.");
    }

    #[test]
    fn a_bad_archive_is_refused_in_words_not_a_code() {
        let message = check_backup_file(b"not a zip", Some("60:55:f9:0a:0b:0c")).unwrap_err();
        assert!(
            message.contains("zip") || message.contains("manifest"),
            "{message}"
        );
    }

    #[test]
    fn the_offer_is_agent_unreachable_and_the_data_op_is_not_published() {
        let offer = DeviceRestoreFromFileOp {
            device: DeviceId(1),
        }
        .default_action_meta();
        assert!(offer.needs_user_activation, "a file picker needs a click");
        assert!(offer.consequence.copy().is_some(), "it is Lasting");
    }
}
