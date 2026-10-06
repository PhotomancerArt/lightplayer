//! What the device card says and offers about a board's files across the C6
//! repartition (plan P06): the question before they move, the refusal when
//! they do not fit, the line on a board whose files are waiting, and the
//! verbs that resolve each. Core decides; the card only lays these out.
//!
//! The verbs are offers (docs/adr/2026-10-01-agentic-control-offers-in-core.md):
//! published into the view's offer tree under `devices/<board>/…`, so the
//! app agent reads and presses the same ones the card draws. The view types
//! below carry only words and the offers' paths.
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
use crate::{ActionConfirmation, OfferPath, UiAction, UiOffer, UiOfferTree};

/// `devices/<board>/continue-update`: the question's Continue.
pub const CONTINUE_UPDATE: &str = "continue-update";
/// `devices/<board>/cancel-update`: the question's Cancel.
pub const CANCEL_UPDATE: &str = "cancel-update";
/// `devices/<board>/download-backup`: the backup, as a file.
pub const DOWNLOAD_BACKUP: &str = "download-backup";
/// `devices/<board>/restore-files`: put a pending backup back.
pub const RESTORE_FILES: &str = "restore-files";
/// `devices/<board>/finish-update`: move a held board's waiting files.
pub const FINISH_UPDATE: &str = "finish-update";

/// The card's layout facts for one device.
#[derive(Clone, Debug, PartialEq)]
pub struct UiDeviceLayout {
    /// Where this card's verbs are offered (`devices/<board>`, the card's
    /// [`crate::BoardRef`]): the card reads its verbs from here rather than
    /// spelling the path itself.
    pub offers_at: OfferPath,
    /// The panel the firmware zone shows instead of its verb row: the
    /// question, or the refusal.
    pub panel: Option<UiLayoutPanel>,
    /// One line in the firmware zone (a board whose files are waiting).
    pub line: Option<String>,
    /// "Restore files" — the resume rule's verb (the line names the date).
    pub restore: Option<OfferPath>,
    /// The Update verb relabelled "Finish update" on a board holding its
    /// files for a migration.
    pub finish_update: Option<OfferPath>,
    /// Download the backup this card is about (always offered beside a
    /// restore).
    pub download: Option<OfferPath>,
}

/// The question (or the refusal), as the card draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct UiLayoutPanel {
    pub title: String,
    pub body: String,
    /// Fewer than a quarter free, or the backup could not be stored here.
    pub warning: Option<String>,
    pub download: OfferPath,
    /// `None` on a refusal. Its offer is disabled until the backup is safe
    /// (stored, or downloaded).
    pub continue_action: Option<OfferPath>,
    /// `None` on a refusal (the activity has already ended).
    pub cancel: Option<OfferPath>,
}

/// The card's layout facts, or `None` when there is nothing to say; every
/// verb they name is published into `offers`.
///
/// `offers_at` is the card's `devices/<board>` prefix (where the rest of its
/// verbs are placed); `fs` is the board's last hello's
/// filesystem state and `has_uid` whether that hello named a stamped
/// identity; `pending` the stored backup still marked pending for its base
/// MAC; `staged` what its last inspection staged.
pub fn device_layout_view(
    view: &DeviceView,
    offers_at: OfferPath,
    fs: BoardFs,
    has_uid: bool,
    staged: Option<&LayoutStaging>,
    pending: Option<&BackupEntry>,
    offers: &mut UiOfferTree,
) -> Option<UiDeviceLayout> {
    let device = view.id;
    let mut publish = |verb: &str, icon: &str, action: UiAction| {
        let path = offers_at.clone().child(verb);
        offers.publish(UiOffer::new(path.clone(), icon, action));
        path
    };
    let mut layout = UiDeviceLayout {
        offers_at: offers_at.clone(),
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
        layout.panel = Some(question_panel(
            device,
            &flash_layout.verdict,
            staged,
            &mut publish,
        ));
        return Some(layout);
    }

    // The refusal, until something else happens on the card.
    if view.activity.is_none()
        && view
            .last_outcome
            .as_ref()
            .is_some_and(|outcome| !outcome.ok)
        && let Some(sentence) = staged.and_then(|s| s.verdict.refusal_sentence())
    {
        layout.panel = Some(UiLayoutPanel {
            title: "This board's files don't fit the new firmware".to_string(),
            body: format!(
                "{sentence} Nothing was changed. Remove a project from the board, then update \
                 again."
            ),
            warning: None,
            download: publish(
                DOWNLOAD_BACKUP,
                "download",
                DeviceBackupOp::action_for(device),
            ),
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
            layout.finish_update = Some(publish(
                FINISH_UPDATE,
                "upload",
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
                )
                // Lasting, as every Flash is (D7) — with words that say what
                // this one changes for good: the old layout is retired.
                .lasting(ActionConfirmation::new(
                    "Finish moving this board's files?",
                    "The board gets the new firmware again, its waiting files move to the new \
                     layout, and the old layout is retired. Studio asks once more, with a \
                     backup, before the files move.",
                    "finish",
                )),
            ));
        }
        return Some(layout);
    }
    // A board that came back without its files while a backup of them is
    // still pending (an interrupted update): offer them back. "Without its
    // files" is the boot that formatted — or ANY later boot of that empty
    // filesystem, which mounts fine but names no identity (the walk's W7b:
    // a reboot between the interruption and the user's return must not
    // hide the way back).
    if let Some(entry) = pending
        && (fs == BoardFs::Formatted || (fs == BoardFs::Mounted && !has_uid))
    {
        layout.line = Some(format!(
            "This board's files from {} are in a backup in this browser.",
            date(entry.captured_at_epoch_seconds)
        ));
        if let Some(FirmwareVerb::Update(choice)) = firmware_verb(view) {
            layout.restore = Some(publish(
                RESTORE_FILES,
                "upload",
                DevicesOp::action_for(Action::Flash {
                    device,
                    board_id: choice.board_id.clone(),
                    build_id: choice.build_id.clone(),
                    park_first: choice.park_first,
                    name: None,
                    restore_backup: true,
                })
                // Short: the line above it already names the backup's date,
                // and the row also holds Download backup.
                .with_label("Restore files")
                .with_summary(format!(
                    "Write the firmware and put the files backed up on {} back on this board. \
                     It replaces what is on the board now.",
                    date(entry.captured_at_epoch_seconds)
                ))
                // Lasting (D7): it writes over the board's filesystem.
                .lasting(ActionConfirmation::new(
                    "Replace this board's files with the backup?",
                    format!(
                        "Whatever is on the board now is replaced by the files backed up on {}. \
                         The backup stays in this browser.",
                        date(entry.captured_at_epoch_seconds)
                    ),
                    "restore",
                )),
            ));
        }
        layout.download = Some(publish(
            DOWNLOAD_BACKUP,
            "download",
            DeviceBackupOp::action_for(device),
        ));
        return Some(layout);
    }
    None
}

fn question_panel(
    device: DeviceId,
    verdict: &LayoutVerdict,
    staged: Option<&LayoutStaging>,
    publish: &mut impl FnMut(&str, &str, UiAction) -> OfferPath,
) -> UiLayoutPanel {
    let download = publish(
        DOWNLOAD_BACKUP,
        "download",
        DeviceBackupOp::action_for(device),
    );
    let cancel = Some(publish(
        CANCEL_UPDATE,
        "revert",
        DevicesOp::action_for(Action::CancelActivity { device }),
    ));
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
                download,
                continue_action: Some(publish(CONTINUE_UPDATE, "apply", continue_action)),
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
            download,
            continue_action: Some(publish(CONTINUE_UPDATE, "apply", confirm)),
            cancel,
        },
        // A question is only asked for the two verdicts above; the others
        // never wait (kept total so a new verdict is a compile error).
        LayoutVerdict::Plain | LayoutVerdict::Refused { .. } => UiLayoutPanel {
            title: String::new(),
            body: String::new(),
            warning: None,
            download,
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

    /// G1 rehearsal (2026-10-03): the refusal said "36 files (834 KB) …
    /// room for 704 KB" while the test that refused is littlefs blocks plus
    /// a reserve — so a board holding LESS than 704 KB can be refused with
    /// numbers that say it fits. The refusal states the planner's own
    /// measure: blocks of 4 KB, out of the new layout's 176, with the
    /// reserve an update keeps.
    #[test]
    fn a_refusal_states_the_measure_the_planner_refused_by() {
        use super::super::device_layout_step::stage_layout;
        use super::super::device_layout_step::tests as step;
        // 190 files of 3 KB (570 KB): under 704 KB, and still refused —
        // every file takes a block of its own.
        let tree = step::tree(190);
        assert!(tree.total_bytes() < 720_896, "the premise: under 704 KB");
        let staging = stage_layout(
            &step::inspection_of(&step::legacy_chip(&tree)),
            None,
            false,
            1.0,
        )
        .unwrap();
        let mut view = running_c6();
        view.last_outcome = Some(lpa_devices::view::OutcomeView {
            summary: "not updated".to_string(),
            ok: false,
        });
        let layout = device_layout_view(
            &view,
            prefix(),
            BoardFs::Mounted,
            true,
            Some(&staging),
            None,
            &mut UiOfferTree::new(),
        )
        .expect("the refusal");
        let body = layout.panel.expect("the refusal panel").body;
        assert!(body.contains("blocks of 4 KB"), "{body}");
        assert!(body.contains("176"), "the new layout's blocks: {body}");
        assert!(body.contains("keep 16"), "the reserve: {body}");
        assert!(
            !body.contains("704 KB"),
            "never a byte room the test did not use: {body}"
        );
    }

    #[test]
    fn dates_and_sizes_read_naturally() {
        assert_eq!(date(1_800_000_000.0), "Jan 15");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(720_896), "704 KB");
    }

    /// The backup is offered on the boot that formatted AND on any later
    /// boot of that empty filesystem (mounted, but naming no identity) —
    /// never on a board that has its own files and identity back.
    #[test]
    fn a_pending_backup_is_offered_until_the_board_has_its_identity_back() {
        let entry = super::super::device_backup_store::BackupEntry {
            base_mac: "60:55:f9:0a:0b:0c".to_string(),
            archive: "a.zip".to_string(),
            captured_at_epoch_seconds: 1_800_000_000.0,
            purpose: "layout-migration".to_string(),
            status: super::super::device_backup_store::BackupStatus::Pending,
            file_count: 3,
            total_bytes: 100,
        };
        let view = running_c6();
        let offered = |fs, has_uid| {
            device_layout_view(
                &view,
                prefix(),
                fs,
                has_uid,
                None,
                Some(&entry),
                &mut UiOfferTree::new(),
            )
            .is_some_and(|layout| layout.restore.is_some())
        };
        assert!(
            offered(BoardFs::Formatted, false),
            "the boot that formatted"
        );
        assert!(
            offered(BoardFs::Mounted, false),
            "a later boot of the empty fs"
        );
        assert!(!offered(BoardFs::Mounted, true), "its own files are back");
        assert!(
            device_layout_view(
                &view,
                prefix(),
                BoardFs::Mounted,
                false,
                None,
                None,
                &mut UiOfferTree::new()
            )
            .is_none(),
            "no backup, nothing to offer"
        );
    }

    /// The five layout offers' levels (docs/adr/2026-10-01-offer-tree-and-
    /// consequence-levels.md): every verb that changes the board's data for
    /// good is `Lasting` with its own words, so the card arms it and the app
    /// agent hands it to the user; the download needs a real click; Cancel
    /// writes nothing and is routine.
    #[test]
    fn the_layout_offers_carry_their_consequence_levels() {
        use lpa_devices::view::ActivityView;
        use lpa_devices::{ActivityKind, FlashLayoutView};

        let at = |verb: &str| {
            let path = prefix().child(verb);
            assert_eq!(path.to_string(), format!("devices/mac-6055f90a0b0c/{verb}"));
            path
        };
        let lasting = |offers: &UiOfferTree, verb: &str| {
            let offer = offers
                .get(&at(verb))
                .unwrap_or_else(|| panic!("{verb} offered"));
            let copy = offer
                .consequence()
                .copy()
                .unwrap_or_else(|| panic!("{verb} is Lasting: {:?}", offer.consequence()));
            assert!(!copy.message.is_empty(), "{verb} says what is lost");
            assert!(offer.action.meta().needs_user(), "{verb} is the user's");
            copy.title.clone()
        };

        // The question: Continue is Lasting, Cancel routine, the download a
        // real click that loses nothing. Continue stays Lasting although the
        // sheet draws it as one press (G1 walk, 2026-10-03): the sheet is
        // the asking, and this level is what keeps the agent handing it to
        // the user.
        let mut asking = running_c6();
        asking.activity = Some(ActivityView {
            kind: ActivityKind::Flash,
            label: "Flashing firmware".to_string(),
            percent: None,
            cancellable: true,
            cancel_requested: false,
            layout: Some(FlashLayoutView {
                verdict: LayoutVerdict::Migrate {
                    files: 3,
                    bytes: 9_000,
                    free_blocks: 150,
                    tight: false,
                    backup_stored: true,
                    device_uid: None,
                },
                awaiting_consent: true,
            }),
            update: None,
        });
        let mut offers = UiOfferTree::new();
        device_layout_view(
            &asking,
            prefix(),
            BoardFs::Mounted,
            true,
            None,
            None,
            &mut offers,
        )
        .expect("the question");
        assert_eq!(lasting(&offers, CONTINUE_UPDATE), "Rewrite this board now?");
        let cancel = offers.get(&at(CANCEL_UPDATE)).expect("cancel offered");
        assert!(cancel.consequence().is_routine());
        assert!(!cancel.action.meta().needs_user(), "the agent may cancel");
        let download = offers.get(&at(DOWNLOAD_BACKUP)).expect("download offered");
        assert!(
            download.consequence().is_routine(),
            "a download loses nothing"
        );
        assert!(download.action.meta().needs_user_activation);

        // A held board: Finish update retires the old layout.
        let mut offers = UiOfferTree::new();
        device_layout_view(
            &running_c6(),
            prefix(),
            BoardFs::LegacyHeld,
            true,
            None,
            None,
            &mut offers,
        )
        .expect("the held line");
        assert_eq!(
            lasting(&offers, FINISH_UPDATE),
            "Finish moving this board's files?"
        );

        // A board back without its files: Restore writes over them.
        let entry = super::super::device_backup_store::BackupEntry {
            base_mac: "60:55:f9:0a:0b:0c".to_string(),
            archive: "a.zip".to_string(),
            captured_at_epoch_seconds: 1_800_000_000.0,
            purpose: "layout-migration".to_string(),
            status: super::super::device_backup_store::BackupStatus::Pending,
            file_count: 3,
            total_bytes: 100,
        };
        let mut offers = UiOfferTree::new();
        device_layout_view(
            &running_c6(),
            prefix(),
            BoardFs::Formatted,
            false,
            None,
            Some(&entry),
            &mut offers,
        )
        .expect("the restore line");
        assert_eq!(
            lasting(&offers, RESTORE_FILES),
            "Replace this board's files with the backup?"
        );
        let download = offers.get(&at(DOWNLOAD_BACKUP)).expect("download offered");
        assert!(download.consequence().is_routine());
        assert!(download.action.meta().needs_user_activation);
    }

    /// The layout facts carry the path their verbs were published at: the
    /// card's own `devices/<board>` prefix (a board known by its MAC is
    /// `mac-` and its 12 hex digits — `crate::BoardRef`), handed in by the
    /// controller beside the rest of the card's verbs.
    #[test]
    fn the_layout_facts_name_the_prefix_their_verbs_are_under() {
        // And the layout facts carry the path their verbs were published at.
        let layout = device_layout_view(
            &running_c6(),
            prefix(),
            BoardFs::LegacyHeld,
            true,
            None,
            None,
            &mut UiOfferTree::new(),
        )
        .expect("the held line");
        assert_eq!(layout.offers_at.to_string(), "devices/mac-6055f90a0b0c");
        assert_eq!(
            layout.finish_update.map(|path| path.to_string()),
            Some("devices/mac-6055f90a0b0c/finish-update".to_string())
        );
    }

    /// The fixture board's verbs' prefix: its MAC as the roster records it,
    /// through the same `BoardRef` the controller places every card's verbs
    /// by.
    fn prefix() -> OfferPath {
        let mac = lpa_devices::identity::MacAddress("60:55:f9:0a:0b:0c".to_string());
        let key = lpa_devices::BoardKey::from_mac(&mac).expect("a MAC");
        OfferPath::board(&crate::BoardRef::Mac(key))
    }

    /// A running C6 on a resolved board (the Update verb resolves).
    fn running_c6() -> DeviceView {
        use lpa_devices::view::{Escape, FirmwareFace, LoadedProject};
        DeviceView {
            id: DeviceId(7),
            title: "Porch".to_string(),
            status: lpa_devices::DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: Some("esp32c6".to_string()),
            board_id: Some("seeed/xiao-esp32-c6".to_string()),
            firmware_face: FirmwareFace::LightPlayer {
                firmware: Some("fw-esp32c6 abc1234".to_string()),
                wire: lpa_devices::WireVersion::Match,
                age: lpa_devices::FirmwareAge::Unknown,
            },
            remembered_firmware: None,
            degraded: None,
            engine_fps: None,
            link_counters: None,
            loaded_project: LoadedProject::Empty,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            escapes: vec![Escape::Disconnect, Escape::Forget],
            update_blocked: None,
            last_update_outcome: None,
        }
    }
}
