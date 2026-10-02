//! What the device card says and offers about a board's files across the C6
//! repartition (plan P06): the question before they move, the refusal when
//! they do not fit, the line on a board whose files are waiting, and the
//! verbs that resolve each. Core decides; the card only lays these out.
//!
//! Plain language on purpose (Yona gets lost in plan-speak): "files",
//! "board", "backup" — never `lpfs`, "superblock" or a phase name.

use lpa_devices::wire::BoardFs;
use lpa_devices::{Action, DeviceId, DeviceView, LayoutVerdict};

use super::device_backup_op::DeviceBackupOp;
use super::device_backup_store::BackupEntry;
use super::device_flash::{FirmwareVerb, firmware_verb};
use super::device_layout_step::LayoutStaging;
use super::devices_op::DevicesOp;
use crate::UiAction;

/// The card's layout facts for one device.
#[derive(Clone, Debug, PartialEq)]
pub struct UiDeviceLayout {
    /// The panel the firmware zone shows instead of its verb row: the
    /// question, or the refusal.
    pub panel: Option<UiLayoutPanel>,
    /// One line in the firmware zone (a board whose files are waiting).
    pub line: Option<String>,
    /// "Restore files from backup (<date>)" — the resume rule's verb.
    pub restore: Option<UiAction>,
    /// The Update verb relabelled "Finish update" on a board holding its
    /// files for a migration.
    pub finish_update: Option<UiAction>,
    /// Download the backup this card is about (always offered beside a
    /// restore).
    pub download: Option<UiAction>,
}

/// The question (or the refusal), as the card draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct UiLayoutPanel {
    pub title: String,
    pub body: String,
    /// Fewer than a quarter free, or the backup could not be stored here.
    pub warning: Option<String>,
    pub download: UiAction,
    /// `None` on a refusal. Disabled until the backup is safe (stored, or
    /// downloaded).
    pub continue_action: Option<UiAction>,
    /// `None` on a refusal (the activity has already ended).
    pub cancel: Option<UiAction>,
}

/// The card's layout facts, or `None` when there is nothing to say.
///
/// `fs` is the board's last hello's filesystem state; `pending` the stored
/// backup still marked pending for its base MAC; `staged` what its last
/// inspection staged.
pub fn device_layout_view(
    view: &DeviceView,
    fs: BoardFs,
    staged: Option<&LayoutStaging>,
    pending: Option<&BackupEntry>,
) -> Option<UiDeviceLayout> {
    let device = view.id;
    let mut layout = UiDeviceLayout {
        panel: None,
        line: None,
        restore: None,
        finish_update: None,
        download: None,
    };

    // The question, while a Flash waits on it.
    if let Some(flash_layout) = view
        .activity
        .as_ref()
        .and_then(|activity| activity.layout.as_ref())
        .filter(|layout| layout.awaiting_consent)
    {
        layout.panel = Some(question_panel(device, &flash_layout.verdict, staged));
        return Some(layout);
    }

    // The refusal, until something else happens on the card.
    if view.activity.is_none()
        && view
            .last_outcome
            .as_ref()
            .is_some_and(|outcome| !outcome.ok)
        && let Some(LayoutVerdict::Refused {
            files,
            bytes,
            room_bytes,
        }) = staged.map(|s| &s.verdict)
    {
        layout.panel = Some(UiLayoutPanel {
            title: "This board's files don't fit the new firmware".to_string(),
            body: format!(
                "This board holds {files} files ({}); after the update it has room for {}. \
                 Nothing was changed. Remove a project from the board, then update again.",
                size(*bytes),
                size(*room_bytes)
            ),
            warning: None,
            download: DeviceBackupOp::action_for(device),
            continue_action: None,
            cancel: None,
        });
        return Some(layout);
    }

    if view.activity.is_some() {
        return None;
    }
    // A board holding its files for a migration: the Update verb finishes it.
    if fs == BoardFs::LegacyHeld {
        layout.line =
            Some("This board's files are waiting — Finish update moves them.".to_string());
        if let Some(FirmwareVerb::Update(choice)) = firmware_verb(view) {
            layout.finish_update = Some(
                DevicesOp::action_for(Action::Flash {
                    device,
                    board_id: choice.board_id.clone(),
                    build_id: choice.build_id.clone(),
                    park_first: choice.park_first,
                    name: None,
                    restore_backup: false,
                })
                .with_label("Finish update")
                .with_summary(
                    "Write the firmware again and move this board's waiting files onto it.",
                ),
            );
        }
        return Some(layout);
    }
    // A board that came back without its files while a backup of them is
    // still pending (an interrupted update): offer them back.
    if let Some(entry) = pending
        && fs == BoardFs::Formatted
    {
        layout.line = Some(format!(
            "This board's files from {} are in a backup in this browser.",
            date(entry.captured_at_epoch_seconds)
        ));
        if let Some(FirmwareVerb::Update(choice)) = firmware_verb(view) {
            layout.restore = Some(
                DevicesOp::action_for(Action::Flash {
                    device,
                    board_id: choice.board_id.clone(),
                    build_id: choice.build_id.clone(),
                    park_first: choice.park_first,
                    name: None,
                    restore_backup: true,
                })
                .with_label(format!(
                    "Restore files from backup ({})",
                    date(entry.captured_at_epoch_seconds)
                ))
                .with_summary(
                    "Write the firmware and put the backed-up files back on this board. \
                     It replaces what is on the board now.",
                ),
            );
        }
        layout.download = Some(DeviceBackupOp::action_for(device));
        return Some(layout);
    }
    None
}

fn question_panel(
    device: DeviceId,
    verdict: &LayoutVerdict,
    staged: Option<&LayoutStaging>,
) -> UiLayoutPanel {
    let cancel = Some(DevicesOp::action_for(Action::CancelActivity { device }));
    let confirm = DevicesOp::action_for(Action::ConfirmFlashLayout { device });
    match verdict {
        LayoutVerdict::Migrate {
            files,
            bytes,
            tight,
            backup_stored,
            ..
        } => {
            let downloaded = staged.is_some_and(|s| s.downloaded);
            let (body, warning, continue_action) = match backup_stored {
                true => (
                    format!(
                        "Studio saved a backup of this board's {files} files ({}) in this \
                         browser, and now rewrites the board. It takes about a minute. Keep it \
                         plugged in.",
                        size(*bytes)
                    ),
                    tight.then(|| {
                        "The board will be nearly full afterwards: less than a quarter of its \
                         space left."
                            .to_string()
                    }),
                    confirm,
                ),
                false => (
                    format!(
                        "This update rewrites the board and moves its {files} files ({}). It \
                         takes about a minute. Keep it plugged in.",
                        size(*bytes)
                    ),
                    Some(
                        "This browser could not keep a backup. Download it first — then \
                         Continue."
                            .to_string(),
                    ),
                    match downloaded {
                        true => confirm,
                        false => confirm.disabled("Download the backup first."),
                    },
                ),
            };
            UiLayoutPanel {
                title: "Move this board's files to the new layout".to_string(),
                body,
                warning,
                download: DeviceBackupOp::action_for(device),
                continue_action: Some(continue_action),
                cancel,
            }
        }
        LayoutVerdict::Restore {
            captured_at,
            files,
            bytes,
            ..
        } => UiLayoutPanel {
            title: "Put this board's files back".to_string(),
            body: format!(
                "Studio puts back the {files} files ({}) it saved from this board on {}, \
                 replacing what is on it now. It takes about a minute. Keep it plugged in.",
                size(*bytes),
                date(*captured_at as f64)
            ),
            warning: None,
            download: DeviceBackupOp::action_for(device),
            continue_action: Some(confirm),
            cancel,
        },
        // A question is only asked for the two verdicts above; the others
        // never wait (kept total so a new verdict is a compile error).
        LayoutVerdict::Plain | LayoutVerdict::Refused { .. } => UiLayoutPanel {
            title: String::new(),
            body: String::new(),
            warning: None,
            download: DeviceBackupOp::action_for(device),
            continue_action: None,
            cancel,
        },
    }
}

/// `"1.5 KB"`, `"704 KB"`.
fn size(bytes: u64) -> String {
    if bytes < 10 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} KB", bytes / 1024)
    }
}

/// `"Oct 1"` (UTC) from epoch seconds.
fn date(epoch_secs: f64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (epoch_secs as i64).div_euclid(86_400);
    let z = days + 719_468;
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{} {day}", MONTHS[(month - 1) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_sizes_read_naturally() {
        assert_eq!(date(1_800_000_000.0), "Jan 15");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(720_896), "704 KB");
    }
}
