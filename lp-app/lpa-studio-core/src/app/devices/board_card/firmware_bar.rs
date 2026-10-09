//! The firmware bar: the version alone, blue when Update is offered, its
//! work in the bar, and everything else in its details.
//!
//! The busiest part of today's card — the face's verdict, the update
//! story's line and bar, the layout question, the restore face, Flash,
//! Update, Reinstall, Other version…, From a file…, Restore from a backup
//! file…, Download backup and Factory reset — as one bar.
//!
//! First match wins:
//!
//! | Board | Summary | Aside | Tone | Action / work |
//! |---|---|---|---|---|
//! | Flash or erase running | (the work shows) | — | Neutral | the work's Cancel |
//! | An update running, or about to (its story is progress) | (the work: the update's short words) | — | Neutral | Cancel while backing up |
//! | The layout question or refusal open | "Needs your answer" (a flash's own step shows as its work) | — | Attention | — (the details rise) |
//! | Its files need restoring | "Its files need restoring" | — | Attention | Restore files |
//! | Its files are waiting | "Files waiting" | — | Attention | Finish update |
//! | Needs you: keeps crashing | the update's words | — | Attention | Reinstall |
//! | Needs you: a version Studio can't get | the update's words | — | Attention | Install <Studio's own> |
//! | Needs you: one update over USB | the update's words | — | Attention | Update (by USB) |
//! | An update offered over the air | the version alone | — | Live | the offer's word ("Update", "Install <v>") |
//! | Older, the update offered by USB | the version alone | — | Live | Update (the board pick, when it must pick) |
//! | Play only, an update available | the version alone | — | Live | Update with a lock (Unlock) |
//! | Offline, or a closed port (nothing heard this time) | the remembered version | "last seen" ("when it's back" when the standing says Available) | Neutral (Live) | — |
//! | The flash wants firmware | "No firmware", "Pre-hello firmware", … | — | Attention | — (Install is the primary); Update (by USB) for an older LightPlayer whose board is known |
//! | Nothing known | "Not known yet" | — | Neutral | — |
//! | Otherwise | the version alone | — | Neutral | — |
//!
//! "The version alone" never repeats the board, the MAC or a verdict
//! (R7): the update story's version when there is one, else the firmware
//! label the board said hello with (`dev 5eb70a7`, `fw-esp32c6 abc1234`).
//! Blue means Update and nothing else; orange means it needs you.

use lpa_devices::view::{FirmwareFace, PendingLinkView};
use lpa_devices::{FirmwareAge, WireVersion};

use super::bar_work::{bar_work, board_pick};
use super::board_card_input::BoardCardInput;
use super::detail_sections::{danger, facts, notice, verbs, without_empty};
use super::ui_bar_work::BarWorkState;
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::device_firmware_face::{device_firmware_line, pending_firmware_line};
use crate::app::devices::device_identity::{IdentityFirmware, device_identity_line};
use crate::app::devices::device_update_standing::UpdateStanding;
use crate::app::devices::device_update_words::UpdateRowKind;
use crate::{RichLine, UiOffer, UiStatusKind};

/// The firmware bar.
pub(crate) fn firmware_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let view = input.view;
    let update = input.update;
    let standing = update.map(|update| &update.standing);
    let layout = input.layout;
    let blocked = view.firmware_blocked.is_some();
    let identity = device_identity_line(view);
    let version = version_alone(input, &identity.firmware);
    let mut aside = None;
    let (summary, tone, action) = if layout.is_some_and(|layout| layout.panel.is_some()) {
        (
            "Needs your answer".to_string(),
            UiStatusKind::Attention,
            None,
        )
    } else if input.offer("restore-files").is_some() || input.offer("restore-from-file").is_some() {
        (
            "Its files need restoring".to_string(),
            UiStatusKind::Attention,
            input
                .offer("restore-files")
                .filter(|_| !blocked)
                .map(|restore| UiCardAction::press(restore, "Restore files").with_icon("upload")),
        )
    } else if let Some(finish) = input.offer("finish-update") {
        (
            "Files waiting".to_string(),
            UiStatusKind::Attention,
            (!blocked).then(|| UiCardAction::press(finish, "Finish update").with_icon("download")),
        )
    } else if let Some(update) = update.filter(|update| update.kind == UpdateRowKind::NeedsYou) {
        let answer = match standing {
            Some(UpdateStanding::KeepsCrashing { .. }) => input
                .offer("reinstall-firmware")
                .map(|reinstall| UiCardAction::press(reinstall, "Reinstall").with_icon("retry")),
            Some(UpdateStanding::CantGetVersion { .. }) => input
                .offer("install-firmware")
                .filter(|install| install.params().is_empty())
                .map(|install| UiCardAction::own_words(install).with_icon("download")),
            Some(UpdateStanding::NeedsUsbOnce { .. }) => input
                .offer("update-firmware")
                .map(|update| usb_update(input, update)),
            _ => None,
        };
        (update.line.clone(), UiStatusKind::Attention, answer)
    } else if let Some(UpdateStanding::PlayOnly { .. }) = standing {
        (
            version.clone(),
            UiStatusKind::Live,
            input.offer("unlock").map(|unlock| {
                UiCardAction::press(unlock, "Update")
                    .with_icon("lock")
                    .drawn(UiActionDraw::Sheet)
            }),
        )
    } else if let Some(offer) = input
        .offer("update-firmware")
        .filter(|_| update_recommended(input))
    {
        let action = match offer.consequence().arms() || !offer.params().is_empty() {
            // The flash path by USB: Lasting, arms in place; the board pick
            // when the board must be picked.
            true => usb_update(input, offer),
            // Over the air: one press, in the offer's own word.
            false => UiCardAction::own_words(offer).with_icon("download"),
        };
        (version.clone(), UiStatusKind::Live, Some(action))
    } else if let (IdentityFirmware::Remembered(_), FirmwareFace::Unknown) =
        (&identity.firmware, &view.firmware_face)
    {
        let available = matches!(standing, Some(UpdateStanding::Available { .. }));
        aside = Some(
            match available {
                true => "when it's back",
                false => "last seen",
            }
            .to_string(),
        );
        let tone = match available {
            true => UiStatusKind::Live,
            false => UiStatusKind::Neutral,
        };
        (version.clone(), tone, None)
    } else if view.firmware_face.wants_flash() {
        // Install (`flash`) is the primary. An older LightPlayer whose board
        // is known is offered the USB update instead of a flash: that is
        // the bar's answer, here where the face says it is needed.
        let update = input
            .offer("update-firmware")
            .filter(|_| input.offer("flash").is_none())
            .map(|update| usb_update(input, update));
        (
            face_words(&view.firmware_face).to_string(),
            UiStatusKind::Attention,
            update,
        )
    } else if let FirmwareFace::CoreOnly { .. } = view.firmware_face {
        (
            device_firmware_line(&view.firmware_face, None),
            UiStatusKind::Attention,
            None,
        )
    } else {
        (version.clone(), UiStatusKind::Neutral, None)
    };
    let work = firmware_work(input);
    let running = work
        .as_ref()
        .is_some_and(|work| work.state == BarWorkState::Running);
    UiStackBar {
        layer: BarLayer::Firmware,
        icon: "firmware".to_string(),
        details: details(input, &version, tone, action.as_ref()),
        summary,
        aside,
        aside_icon: None,
        tone: if running { UiStatusKind::Neutral } else { tone },
        action: if running { None } else { action },
        work,
    }
}

/// A new board's firmware bar: what the settled verdict says, short, in
/// Attention when it wants a flash; "Known once it identifies" until then.
/// No action: Install is the primary.
pub(crate) fn pending_firmware_bar(pending: &PendingLinkView) -> UiStackBar {
    let face = &pending.firmware_face;
    let (summary, tone) = match face {
        FirmwareFace::Unknown => (
            "Known once it identifies".to_string(),
            UiStatusKind::Neutral,
        ),
        face if face.wants_flash() => (face_words(face).to_string(), UiStatusKind::Attention),
        face => (device_firmware_line(face, None), UiStatusKind::Neutral),
    };
    let line = pending_firmware_line(face);
    let sections = match tone {
        UiStatusKind::Attention => vec![notice("Firmware", tone, line, None)],
        _ => {
            let mut said = facts("Firmware", Vec::new());
            said.sentence = Some(line);
            vec![said]
        }
    };
    UiStackBar {
        layer: BarLayer::Firmware,
        icon: "firmware".to_string(),
        summary,
        aside: None,
        aside_icon: None,
        tone,
        action: None,
        work: None,
        details: UiBarDetails {
            sections,
            panels: Vec::new(),
            raised: false,
        },
    }
}

/// The firmware bar's work: a flash, an erase or an update running (the
/// update in its story's words), or an update story in progress with no
/// activity of its own here (another device's, one about to start).
fn firmware_work(input: &BoardCardInput<'_>) -> Option<super::ui_bar_work::UiBarWork> {
    if let Some(work) = bar_work(input, BarLayer::Firmware) {
        return Some(work);
    }
    let update = input
        .update
        .filter(|update| update.kind == UpdateRowKind::Progress)?;
    let progress = update.progress?;
    Some(super::ui_bar_work::UiBarWork {
        words: update.line.clone(),
        percent: progress.percent,
        state: BarWorkState::Running,
        cancel: None,
        other_device: progress.other_device,
    })
}

/// The version alone: the update story's version when there is one, else
/// the label the board said hello with (or the one it is remembered by),
/// else "Not known yet".
fn version_alone(input: &BoardCardInput<'_>, firmware: &IdentityFirmware) -> String {
    if let Some(update) = input.update {
        return update.version.text.clone();
    }
    firmware
        .label()
        .map_or_else(|| "Not known yet".to_string(), str::to_string)
}

/// Whether the board is older than this Studio, so the USB update is the
/// bar's blue action rather than a verb in its details: an update story
/// that says one is available, or a face older than Studio by its version
/// (or, with no version to compare, its wire).
fn update_recommended(input: &BoardCardInput<'_>) -> bool {
    if let Some(update) = input.update {
        return matches!(update.standing, UpdateStanding::Available { .. });
    }
    matches!(
        input.view.firmware_face,
        FirmwareFace::LightPlayer {
            age: FirmwareAge::Older | FirmwareAge::Different,
            ..
        } | FirmwareFace::LightPlayer {
            age: FirmwareAge::Unknown,
            wire: WireVersion::BoardOlder { .. },
            ..
        }
    )
}

/// The update by USB: "Update", arming in place; the board pick when the
/// board must be picked.
fn usb_update(input: &BoardCardInput<'_>, offer: &UiOffer) -> UiCardAction {
    let action = UiCardAction::press(offer, "Update").with_icon("download");
    match offer.params().is_empty() {
        true => action,
        false => action.drawn(board_pick(input)),
    }
}

/// A face that wants a flash, in the bar's words.
fn face_words(face: &FirmwareFace) -> &'static str {
    match face {
        FirmwareFace::Blank => "No firmware",
        FirmwareFace::NoHello => "Pre-hello firmware",
        FirmwareFace::Bootloader => "In download mode",
        FirmwareFace::Foreign { .. } => "Other firmware",
        FirmwareFace::Silent => "No response",
        FirmwareFace::OlderLightPlayer { .. } => "Older LightPlayer",
        FirmwareFace::Unknown
        | FirmwareFace::LightPlayer { .. }
        | FirmwareFace::CoreOnly { .. } => "Not known yet",
    }
}

/// The firmware bar's details: the notice, the firmware's facts, its
/// panels, every firmware verb not drawn as the action, and Factory reset
/// apart.
fn details(
    input: &BoardCardInput<'_>,
    version: &str,
    tone: UiStatusKind,
    action: Option<&UiCardAction>,
) -> UiBarDetails {
    let view = input.view;
    let update = input.update;
    let layout = input.layout;
    let mut sections = Vec::new();

    // 1. The notice.
    let story = update
        .filter(|update| update.kind == UpdateRowKind::NeedsYou || tone == UiStatusKind::Live);
    if let Some(update) = story {
        sections.push(notice(
            "Firmware",
            tone,
            update.sentence.clone(),
            action.cloned(),
        ));
    } else if let Some(panel) = layout.and_then(|layout| layout.panel.as_ref()) {
        sections.push(notice(
            "Firmware",
            UiStatusKind::Attention,
            panel.title.clone(),
            None,
        ));
    } else if let Some(line) = layout.and_then(|layout| layout.line.clone()) {
        sections.push(notice(
            "Firmware",
            UiStatusKind::Attention,
            line,
            action.cloned(),
        ));
    } else if view.firmware_face.wants_flash() {
        let board = device_identity_line(view).board;
        sections.push(notice(
            "Firmware",
            UiStatusKind::Attention,
            device_firmware_line(&view.firmware_face, board.as_deref()),
            None,
        ));
    } else if tone == UiStatusKind::Attention {
        sections.push(notice(
            "Firmware",
            tone,
            device_firmware_line(&view.firmware_face, None),
            None,
        ));
    }

    // 2. The facts.
    let mut lines = vec![RichLine::new("Version", version)];
    if let Some(update) = update {
        if let Some(commit) = &update.version.commit {
            lines.push(RichLine::new("Commit", commit.clone()));
        }
        lines.push(RichLine::new("Build", update.version.raw.clone()));
    } else if let Some(label) = device_identity_line(view).firmware.label() {
        lines.push(RichLine::new("Build", label));
    }
    if let Some(newest) = update.and_then(|update| newest(&update.standing)) {
        lines.push(RichLine::new("Newest", newest));
    }
    if let Some(blocked) = &view.firmware_blocked {
        lines.push(RichLine::new("Over this link", blocked.clone()));
    }
    let mut firmware = facts("Firmware", lines);
    // The update's sentence when nothing above said it: while the board's
    // lights are held, and for what is only worth knowing (up to date,
    // rolled back, newer than this Studio).
    firmware.sentence = update
        .filter(|update| {
            story.is_none() && (update.light.is_some() || update.kind == UpdateRowKind::Information)
        })
        .map(|update| update.sentence.clone());
    sections.push(firmware);

    // 3. The panels.
    let mut panels = Vec::new();
    let layout_panel = layout.and_then(|layout| layout.panel.clone());
    // The layout question presses Download backup itself: the verbs below
    // do not offer it a second time.
    let panel_download = layout_panel.as_ref().map(|panel| panel.download.clone());
    if let Some(panel) = layout_panel {
        panels.push(UiDetailPanel::Layout(panel));
    }
    if let Some(install) = input
        .offer("install-firmware")
        .filter(|install| !install.params().is_empty())
    {
        panels.push(UiDetailPanel::OtherVersion {
            install: install.path.clone(),
            from_file: input
                .offer("install-firmware-file")
                .map(|file| file.path.clone()),
        });
    }
    if let Some(restore) = input.offer("restore-from-file") {
        panels.push(UiDetailPanel::RestoreFromFile {
            offer: restore.path.clone(),
            current_base_mac: layout.and_then(|layout| layout.current_base_mac.clone()),
        });
    }

    // 4. The verbs the bar does not draw as its action.
    let drawn = action.map(|action| &action.offer);
    let update_verb = input.offer("update-firmware").map(|update| {
        match update.consequence().arms() || !update.params().is_empty() {
            true => usb_update(input, update),
            false => UiCardAction::own_words(update).with_icon("download"),
        }
    });
    let actions: Vec<UiCardAction> = [
        verb(
            input.offer("reinstall-firmware"),
            Some("Reinstall"),
            "retry",
        ),
        verb(
            input
                .offer("install-firmware")
                .filter(|install| install.params().is_empty()),
            None,
            "download",
        ),
        verb(
            input.offer("restore-files"),
            Some("Restore files"),
            "upload",
        ),
        verb(
            input.offer("finish-update"),
            Some("Finish update"),
            "download",
        ),
        update_verb,
        verb(input.offer("download-backup"), None, "download"),
    ]
    .into_iter()
    .flatten()
    .filter(|action| Some(&action.offer) != drawn)
    .filter(|action| Some(&action.offer) != panel_download.as_ref())
    .collect();
    sections.push(verbs(actions));

    // 5. Factory reset, apart.
    sections.push(danger(
        input
            .offer("erase")
            .map(|erase| UiCardAction::own_words(erase).with_icon(erase.icon.clone()))
            .into_iter()
            .collect(),
    ));

    UiBarDetails {
        sections: without_empty(sections),
        panels,
        raised: layout.is_some_and(|layout| layout.panel.is_some()),
    }
}

/// This Studio's own version, when the standing names it as the newest.
fn newest(standing: &UpdateStanding) -> Option<String> {
    match standing {
        UpdateStanding::Available { to, .. } | UpdateStanding::PlayOnly { to, .. } => {
            Some(to.short())
        }
        // Newer: the board's own is the newest, and the sentence names this
        // Studio's.
        UpdateStanding::CantGetVersion { own, .. } => Some(own.short()),
        _ => None,
    }
}

/// A firmware verb in `word` (or its own), with `icon`.
fn verb(offer: Option<&UiOffer>, word: Option<&str>, icon: &str) -> Option<UiCardAction> {
    let offer = offer?;
    let action = match word {
        Some(word) => UiCardAction::press(offer, word),
        None => UiCardAction::own_words(offer),
    };
    Some(action.with_icon(icon))
}

#[cfg(test)]
mod tests {
    use lpa_devices::ActivityKind;
    use lpa_devices::device::DeviceStatus;
    use lpa_devices::view::{Escape, LoadedProject};

    use super::super::card_fixtures::{CardFixture, activity, board};
    use super::super::primary_action::tests::pending_view;
    use super::*;
    use crate::app::devices::device_update_fixtures::{UpdateFixture, UpdateFixtureRow};
    use crate::{UiDeviceAccess, UiLinkKind, UiUnlockOffer};

    #[test]
    fn a_flash_running_is_the_bars_work_with_its_cancel() {
        let mut fixture = CardFixture::ready().with_activity(activity(
            ActivityKind::Flash,
            "Flashing firmware",
            Some(62),
        ));
        let bar = firmware_bar(&fixture.input());
        let work = bar.work.expect("the flash");
        assert_eq!(work.words, "Flashing firmware · 62%");
        assert!(work.cancel.is_some());
        assert_eq!(bar.action, None);
        assert_eq!(bar.tone, UiStatusKind::Neutral);
    }

    /// Ported: the update's own short words are the bar's work while it
    /// runs (the header chip used to take them; the bar does now).
    #[test]
    fn the_header_chip_takes_the_updates_word_while_it_owns_the_board() {
        for (row, words, other) in [
            (UpdateFixtureRow::BackingUp, "Backing up · 18%", false),
            (UpdateFixtureRow::Updating, "Updating · 1 of 2 · 40%", false),
            (
                UpdateFixtureRow::Finishing,
                "Updating · 2 of 2 · 70%",
                false,
            ),
            (
                UpdateFixtureRow::FinishingResumed,
                "Resuming · 2 of 2 · 70%",
                false,
            ),
            (UpdateFixtureRow::Restoring, "Restoring · 35%", false),
            (
                UpdateFixtureRow::AnotherDevice,
                "Another device is updating it · 40%",
                true,
            ),
        ] {
            let mut fixture = story(row);
            let bar = firmware_bar(&fixture.input());
            let work = bar.work.unwrap_or_else(|| panic!("{row:?} is work"));
            assert_eq!(work.words, words, "{row:?}");
            assert_eq!(work.other_device, other, "{row:?}");
            assert_eq!(work.state, BarWorkState::Running);
            assert_eq!(bar.tone, UiStatusKind::Neutral, "{row:?}");
        }
        // Cancel only while backing up.
        let mut backing_up = story(UpdateFixtureRow::BackingUp);
        assert!(
            firmware_bar(&backing_up.input())
                .work
                .unwrap()
                .cancel
                .is_some()
        );
        let mut updating = story(UpdateFixtureRow::Updating);
        assert!(
            firmware_bar(&updating.input())
                .work
                .unwrap()
                .cancel
                .is_none()
        );
    }

    /// What is only worth knowing is never lost: up to date, rolled back
    /// and newer than this Studio leave the bar at the version alone, and
    /// the firmware details say the update's sentence. A newer board's
    /// details never call this Studio's older build the newest.
    #[test]
    fn an_update_worth_knowing_says_its_sentence_in_the_details() {
        for (row, says) in [
            (UpdateFixtureRow::UpToDate, "the same as this Studio"),
            (UpdateFixtureRow::RolledBack, "didn't start"),
            (UpdateFixtureRow::Newer, "Reload Studio to catch up"),
        ] {
            let mut fixture = story(row);
            let bar = firmware_bar(&fixture.input());
            assert_eq!(bar.tone, UiStatusKind::Neutral, "{row:?}");
            assert_eq!(bar.action, None, "{row:?}");
            let sentence = bar
                .details
                .sections
                .iter()
                .find_map(|section| section.sentence.clone())
                .unwrap_or_else(|| panic!("{row:?} says its sentence"));
            assert!(sentence.contains(says), "{row:?}: {sentence}");
        }
        let mut newer = story(UpdateFixtureRow::Newer);
        assert_eq!(line(&firmware_bar(&newer.input()), "Newest"), None);
    }

    #[test]
    fn a_crashing_board_offers_reinstall() {
        let mut fixture = story(UpdateFixtureRow::KeepsCrashing);
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.summary, "2026.10.03-1 keeps crashing");
        assert_eq!(bar.tone, UiStatusKind::Attention);
        let reinstall = bar.action.clone().expect("Reinstall");
        assert_eq!(reinstall.word, "Reinstall");
        assert!(reinstall.offer.to_string().ends_with("/reinstall-firmware"));
        let notice = bar.details.notice().expect("the notice");
        assert!(
            notice
                .sentence
                .as_deref()
                .unwrap()
                .contains("keeps crashing")
        );
        assert!(
            bar.details
                .panels
                .iter()
                .any(|panel| matches!(panel, UiDetailPanel::OtherVersion { .. })),
            "Other version… is in its details"
        );
    }

    /// With only Studio's own build to offer (no release index), Install
    /// <own> is one press, and the bar's action; with a list to choose
    /// from, it is the Other version panel in the details.
    #[test]
    fn a_board_needing_a_version_studio_cannot_get_installs_studios_own() {
        let mut fixture = story_with(UpdateFixtureRow::CantGetVersion, UiLinkKind::Usb, true);
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.tone, UiStatusKind::Attention);
        let install = bar.action.expect("Install <own>");
        assert_eq!(install.word, "Install 2026.10.05-2");
        assert!(install.offer.to_string().ends_with("/install-firmware"));

        let mut listed = story(UpdateFixtureRow::CantGetVersion);
        let bar = firmware_bar(&listed.input());
        assert_eq!(bar.action, None);
        assert!(
            bar.details
                .panels
                .iter()
                .any(|panel| matches!(panel, UiDetailPanel::OtherVersion { .. }))
        );
    }

    #[test]
    fn a_board_needing_one_update_over_usb_updates_by_usb() {
        let mut fixture = story(UpdateFixtureRow::NeedsUsbOnce);
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.summary, "Needs one update over USB");
        assert_eq!(bar.tone, UiStatusKind::Attention);
        let update = bar.action.expect("Update");
        assert_eq!(update.word, "Update");
        assert!(update.offer.to_string().ends_with("/update-firmware"));
    }

    /// Ported: the version is set apart as the version alone — never the
    /// line's "X → Y available", the board or the MAC.
    #[test]
    fn the_update_line_sets_the_version_apart() {
        let mut fixture = story(UpdateFixtureRow::Available);
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.summary, "2026.10.03-1");
        assert!(!bar.summary.contains('→'));
        assert!(!bar.summary.contains("a0:f2"));
        assert_eq!(bar.tone, UiStatusKind::Live, "blue: an update is offered");
        let update = bar.action.clone().expect("Update");
        assert_eq!(update.word, "Update");
        assert_eq!(update.draw, UiActionDraw::Press);
        assert!(update.offer.to_string().ends_with("/update-firmware"));
        assert_eq!(line(&bar, "Newest").as_deref(), Some("2026.10.05-2"));
        assert_eq!(line(&bar, "Version").as_deref(), Some("2026.10.03-1"));
        assert_eq!(line(&bar, "Commit").as_deref(), Some("a41c9e2"));
        assert!(
            line(&bar, "Build").is_some_and(|build| build.starts_with("2026.10.03-1+")),
            "the raw build in details"
        );
        assert_eq!(
            bar.details.notice().map(|notice| notice.tone),
            Some(UiStatusKind::Live)
        );
    }

    #[test]
    fn an_older_board_by_usb_is_blue_with_update() {
        let mut fixture = CardFixture::ready();
        fixture.view.firmware_face = FirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 2026.10.01-1".to_string()),
            wire: WireVersion::Match,
            age: FirmwareAge::Older,
        };
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.summary, "fw-esp32c6 2026.10.01-1");
        assert_eq!(bar.tone, UiStatusKind::Live);
        let update = bar.action.expect("Update");
        assert_eq!(update.word, "Update");
        assert!(update.offer.to_string().ends_with("/update-firmware"));

        // The same board on Studio's own version: the USB reflash is a verb
        // in its details, and the bar is not blue.
        let mut current = CardFixture::ready();
        let bar = firmware_bar(&current.input());
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        assert_eq!(bar.action, None);
        assert!(verb_words(&bar).contains(&"Update".to_string()));
    }

    #[test]
    fn a_play_only_board_updates_through_unlock_with_a_lock() {
        let mut fixture = story_over(UpdateFixtureRow::PlayOnly, UiLinkKind::Bluetooth);
        fixture.access = Some(UiDeviceAccess {
            unlock: Some(UiUnlockOffer::PlayOnly),
            ..UiDeviceAccess::default()
        });
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.tone, UiStatusKind::Live);
        let update = bar.action.expect("Update");
        assert_eq!(update.word, "Update");
        assert_eq!(update.icon.as_deref(), Some("lock"));
        assert_eq!(update.draw, UiActionDraw::Sheet);
        assert!(update.offer.to_string().ends_with("/unlock"));
    }

    /// Offline, the standing is not known (the window that held the board's
    /// update facts closed with its link), so the bar says "last seen".
    #[test]
    fn an_offline_board_says_its_remembered_version_last_seen() {
        let mut fixture = CardFixture::offline();
        fixture.view.remembered_firmware = Some("fw-esp32c6 2026.10.05-2".to_string());
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.summary, "fw-esp32c6 2026.10.05-2");
        assert_eq!(bar.aside.as_deref(), Some("last seen"));
        assert_eq!(bar.tone, UiStatusKind::Neutral);
    }

    #[test]
    fn a_face_that_wants_a_flash_says_so_in_attention() {
        for (face, words) in [
            (FirmwareFace::Blank, "No firmware"),
            (FirmwareFace::NoHello, "Pre-hello firmware"),
            (FirmwareFace::Bootloader, "In download mode"),
            (FirmwareFace::Foreign { label: None }, "Other firmware"),
            (FirmwareFace::Silent, "No response"),
            (
                FirmwareFace::OlderLightPlayer { proto: None },
                "Older LightPlayer",
            ),
        ] {
            let mut fixture = CardFixture::ready();
            fixture.view.status = DeviceStatus::NeedsAttention;
            fixture.view.firmware_face = face.clone();
            fixture.view.loaded_project = LoadedProject::Unknown;
            fixture.view.can_receive_project = false;
            fixture.view.can_remove_project = false;
            fixture.view.board_id = None;
            let bar = firmware_bar(&fixture.input());
            assert_eq!(bar.summary, words, "{face:?}");
            assert_eq!(bar.tone, UiStatusKind::Attention, "{face:?}");
            assert_eq!(bar.action, None, "Install is the primary");
        }
    }

    /// An older LightPlayer whose board is known is offered the USB update,
    /// not a flash: the bar that says it is older carries it.
    #[test]
    fn an_older_lightplayer_with_a_known_board_updates_from_its_bar() {
        let mut fixture = CardFixture::ready();
        fixture.view.status = DeviceStatus::NeedsAttention;
        fixture.view.firmware_face = FirmwareFace::OlderLightPlayer { proto: Some(32) };
        fixture.view.loaded_project = LoadedProject::Unknown;
        fixture.view.can_receive_project = false;
        fixture.view.can_remove_project = false;
        let input = fixture.input();
        assert!(input.offer("flash").is_none(), "no flash for a known board");
        let bar = firmware_bar(&input);
        assert_eq!(bar.summary, "Older LightPlayer");
        assert_eq!(bar.tone, UiStatusKind::Attention);
        let action = bar.action.expect("the USB update");
        assert_eq!(action.offer, board().child("update-firmware"));
        assert_eq!(action.word, "Update");
    }

    /// Ported: the face's verdict, joined to its board, is the details'
    /// notice now; the label is the version on the bar, the board is the
    /// hardware bar's.
    #[test]
    fn the_firmware_line_names_the_firmware_and_its_board() {
        let mut blank = CardFixture::ready();
        blank.view.firmware_face = FirmwareFace::Blank;
        blank.view.status = DeviceStatus::NeedsAttention;
        blank.view.loaded_project = LoadedProject::Unknown;
        blank.view.can_receive_project = false;
        blank.view.can_remove_project = false;
        let bar = firmware_bar(&blank.input());
        assert_eq!(
            bar.details
                .notice()
                .and_then(|notice| notice.sentence.as_deref()),
            Some("Blank flash — needs firmware")
        );
        let mut running = CardFixture::ready();
        let bar = firmware_bar(&running.input());
        assert_eq!(bar.summary, "fw-esp32c6 2026.10.05-2");
        assert!(
            !bar.summary.contains("XIAO"),
            "the board is the hardware bar's"
        );
        assert_eq!(
            line(&bar, "Build").as_deref(),
            Some("fw-esp32c6 2026.10.05-2")
        );
    }

    #[test]
    fn nothing_known_is_not_known_yet() {
        let mut fixture = CardFixture::ready();
        fixture.view.firmware_face = FirmwareFace::Unknown;
        assert_eq!(firmware_bar(&fixture.input()).summary, "Not known yet");
    }

    /// Over a link that cannot carry firmware, every refused verb keeps its
    /// own reason, and the reason is said once as a fact.
    #[test]
    fn refused_verbs_carry_their_reason() {
        let mut fixture = CardFixture::ready().over(UiLinkKind::Bluetooth);
        fixture.view.update_blocked = Some(lpa_devices::view::FIRMWARE_NEEDS_USB.to_string());
        let bar = firmware_bar(&fixture.input());
        assert_eq!(
            line(&bar, "Over this link").as_deref(),
            Some(lpa_devices::view::FIRMWARE_NEEDS_USB)
        );
        let reset = bar
            .details
            .sections
            .last()
            .and_then(|danger| danger.affordances.first())
            .expect("Factory reset");
        assert_eq!(reset.word, "Factory reset");
        assert_eq!(
            reset.refused.as_deref(),
            Some(lpa_devices::view::FIRMWARE_NEEDS_USB)
        );
    }

    #[test]
    fn a_new_board_says_known_once_it_identifies_or_no_firmware() {
        let mut pending = pending_view();
        let bar = pending_firmware_bar(&pending);
        assert_eq!(bar.summary, "Known once it identifies");
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        pending.firmware_face = FirmwareFace::Blank;
        let bar = pending_firmware_bar(&pending);
        assert_eq!(bar.summary, "No firmware");
        assert_eq!(bar.tone, UiStatusKind::Attention);
        assert_eq!(
            bar.details
                .notice()
                .and_then(|notice| notice.sentence.as_deref()),
            Some("Blank flash — needs firmware")
        );
        assert_eq!(bar.action, None);
    }

    /// `row` on the fixture board over USB: its update story's words, and
    /// the offers its standing publishes.
    fn story(row: UpdateFixtureRow) -> CardFixture {
        story_over(row, UiLinkKind::Usb)
    }

    fn story_over(row: UpdateFixtureRow, link: UiLinkKind) -> CardFixture {
        story_with(row, link, false)
    }

    /// `offline`: this Studio could not read the store's release index.
    fn story_with(row: UpdateFixtureRow, link: UiLinkKind, offline: bool) -> CardFixture {
        let mut fixture = CardFixture::ready().over(link);
        fixture.view.loaded_project = LoadedProject::Empty;
        fixture.view.can_remove_project = false;
        let mut update = UpdateFixture::new(row, fixture.view.clone());
        if offline {
            update = update.offline();
        }
        fixture.view = update.view.clone();
        if fixture.view.activity.is_some() {
            fixture.view.status = DeviceStatus::Busy;
            fixture.view.can_receive_project = false;
        }
        fixture.update = update.words();
        fixture.update_facts = update.offer_facts();
        assert!(
            fixture.view.escapes.contains(&Escape::Disconnect),
            "a linked board"
        );
        fixture
    }

    fn verb_words(bar: &UiStackBar) -> Vec<String> {
        bar.details
            .sections
            .iter()
            .filter(|section| section.title == "Actions")
            .flat_map(|section| section.affordances.iter().map(|action| action.word.clone()))
            .collect()
    }

    fn line(bar: &UiStackBar, label: &str) -> Option<String> {
        bar.details
            .sections
            .iter()
            .flat_map(|section| section.lines.iter())
            .find(|line| line.label == label)
            .map(|line| line.value.clone())
    }
}
