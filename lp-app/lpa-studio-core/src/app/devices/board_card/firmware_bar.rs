//! The Firmware row: what runs the board (`docs/ux/anatomy/board-card.md`,
//! "Firmware ▲"). **Firmware · version · state · action**; its work in the
//! row, and everything else in its details.
//!
//! The busiest part of today's card — the face's verdict, the update
//! story's line and bar, the layout question, Flash, Update, Reinstall,
//! Other version…, From a file…, Finish update, Download backup and Factory
//! reset — as one row.
//!
//! First match wins:
//!
//! | Board | Name | State | Tone | Action / work |
//! |---|---|---|---|---|
//! | Flash or erase running | the version, else nothing | (work: "installing · 30%", "erasing") | Neutral | the work's Cancel |
//! | An update running, or about to (its story is progress) | the version | (work: "updating · 42%", "backing up · 12%") | Neutral | Cancel while backing up |
//! | The layout question or refusal open | the version | "confirmation required" | Attention | Review (opens the details, which rise) |
//! | Its file store refused (`fs: refused`, an `fs-tree` build) | the version | "data unreadable" | Attention | — (the details say where the files are; no click here is safe) |
//! | Its files wait for a firmware that reads their store | the version | "update required" | Attention | — |
//! | Its files are waiting (a move that stopped half-way) | the version | "update interrupted" | Attention | Resume (`finish-update`) |
//! | Keeps crashing | the version | "crashing" | Error | Reinstall |
//! | Missing a part Studio can't get | the version | "incomplete" | Attention | Install (Studio's own) |
//! | Only its core runs (no engine) | the version | "incomplete" (or "updating" mid-update) | Attention | — |
//! | A core from before over-the-air updates | "v… (pre-OTA)" | — | Neutral | Update (by USB, once) |
//! | A core that updates nearby once (not over Wi‑Fi or the relay yet) | the version | — | Neutral | — (its details say how) |
//! | Play only, an update available | the version | — | Neutral | Update with a lock (Unlock) |
//! | An update offered (over the air, or by USB) | the version | — | Neutral | Update |
//! | Offline, or a closed port (nothing heard this time) | the version remembered, dimmed | — | Neutral | — |
//! | Too old to say hello | "unknown (pre-2026.07)" | — | Neutral | Install (the USB update), unless the primary is Install firmware |
//! | No LightPlayer on it (blank, another firmware, download mode, silent) | "none", or what it is | the state, when it says one | Neutral | — (Install firmware is the primary) |
//! | Nothing known | "unknown" | — | Neutral | — |
//! | Otherwise | the version | — | Neutral | — |
//!
//! The name is Studio's one firmware version ([`crate::firmware_version`]):
//! a release as its tag, `v2026.10.16-1`, a local build `dev c688c99`,
//! never the build's target prefix (`fw-esp32c6 …`, the details' "Build").
//! An update on offer is no alarm: the row is untinted and its button
//! carries it (P16). The button says the verb alone — "Update", "Install"
//! — because "Update to v2026.10.16-1" does not fit beside the version at
//! the card's width; the version it brings is the details' "Newest". Orange
//! means it needs you; red that it is broken. A board whose data was lost
//! says so on its Project row (O3), and its ways back are there.

use lpa_devices::view::{FirmwareFace, PendingLinkView};
use lpa_devices::{FirmwareAge, UpdateBoardState, WireVersion};

use super::bar_work::{bar_work, board_pick, running_work, update_work};
use super::board_card_input::BoardCardInput;
use super::detail_sections::{danger, facts, notice, verbs, without_empty};
use super::project_bar::data_lost;
use super::ui_bar_work::{BarWorkState, UiBarWork};
use super::ui_card_action::{UiActionDraw, UiCardAction};
use super::ui_detail_panel::UiDetailPanel;
use super::ui_stack_bar::{BarLayer, UiBarDetails, UiStackBar};
use crate::app::devices::device_firmware_face::{device_firmware_line, pending_firmware_line};
use crate::app::devices::device_identity::{IdentityFirmware, device_identity_line};
use crate::app::devices::device_update_standing::UpdateStanding;
use crate::app::devices::device_update_words::UpdateRowKind;
use crate::core::data_kinds::{
    FIRMWARE_NONE, FIRMWARE_UNKNOWN, PRE_HELLO_FIRMWARE, firmware_label_version, firmware_version,
    pre_ota_version,
};
use crate::{RichLine, UiOffer, UiStatusKind};

/// The state of a board whose file store it refused: its files are kept,
/// and nothing on the card can read them.
pub const DATA_UNREADABLE: &str = "data unreadable";
/// The state of a board whose firmware does not read the tree store its
/// files are in: kept, waiting for a firmware that does.
pub const UPDATE_REQUIRED: &str = "update required";
/// The state of a board holding its files for a move that stopped
/// half-way; Resume finishes it.
pub const UPDATE_INTERRUPTED: &str = "update interrupted";
/// The state while the layout question waits for an answer.
pub const CONFIRMATION_REQUIRED: &str = "confirmation required";
/// The state of a board whose firmware keeps crashing.
const CRASHING: &str = "crashing";
/// The state of a board missing a part of its firmware.
const INCOMPLETE: &str = "incomplete";

/// The Firmware row.
pub(crate) fn firmware_bar(input: &BoardCardInput<'_>) -> UiStackBar {
    let view = input.view;
    let update = input.update;
    let standing = update.map(|update| &update.standing);
    let layout = input.layout;
    let blocked = view.firmware_blocked.is_some();
    let identity = device_identity_line(view);
    let version = version_alone(input, &identity.firmware);
    let mut stale = false;
    let mut opens_details = None;
    let (summary, aside, tone, action): (String, Option<&str>, UiStatusKind, _) = if layout
        .is_some_and(|layout| layout.panel.is_some())
    {
        opens_details = Some("Review");
        (
            version.clone(),
            Some(CONFIRMATION_REQUIRED),
            UiStatusKind::Attention,
            None,
        )
    } else if layout.is_some_and(|layout| layout.store_needs_newer_firmware) {
        // Words only: the details say a firmware that reads the store
        // brings the files back.
        (
            version.clone(),
            Some(UPDATE_REQUIRED),
            UiStatusKind::Attention,
            None,
        )
    } else if layout.is_some_and(|layout| layout.store_refused) {
        // Words only: the details name the tool that reads its files.
        (
            version.clone(),
            Some(DATA_UNREADABLE),
            UiStatusKind::Attention,
            None,
        )
    } else if let Some(finish) = input.offer("finish-update").filter(|_| !data_lost(input)) {
        (
            version.clone(),
            Some(UPDATE_INTERRUPTED),
            UiStatusKind::Attention,
            (!blocked).then(|| UiCardAction::press(finish, "Resume").with_icon("download")),
        )
    } else if update.is_some_and(|update| update.kind == UpdateRowKind::NeedsYou) {
        match standing {
            Some(UpdateStanding::KeepsCrashing { .. }) => (
                version.clone(),
                Some(CRASHING),
                UiStatusKind::Error,
                input.offer("reinstall-firmware").map(|reinstall| {
                    UiCardAction::press(reinstall, "Reinstall").with_icon("retry")
                }),
            ),
            Some(UpdateStanding::CantGetVersion { .. }) => (
                version.clone(),
                Some(INCOMPLETE),
                UiStatusKind::Attention,
                input
                    .offer("install-firmware")
                    .filter(|install| install.params().is_empty())
                    .map(|install| UiCardAction::press(install, "Install").with_icon("download")),
            ),
            // Pre-OTA: one update over USB, and from then on it updates
            // without a cable. No tone: the button carries it.
            Some(UpdateStanding::NeedsUsbOnce { board, .. }) => (
                pre_ota_version(&board.version),
                None,
                UiStatusKind::Neutral,
                input
                    .offer("update-firmware")
                    .map(|update| usb_update(input, update).with_icon("usb")),
            ),
            // Updates nearby once (a core from before updates through
            // Wi‑Fi or the relay): nothing to press here; its details say
            // how.
            _ => (version.clone(), None, UiStatusKind::Neutral, None),
        }
    } else if let Some(UpdateStanding::PlayOnly { .. }) = standing {
        (
            version.clone(),
            None,
            UiStatusKind::Neutral,
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
            // Over the air: one press.
            false => UiCardAction::press(offer, "Update").with_icon("download"),
        };
        (version.clone(), None, UiStatusKind::Neutral, Some(action))
    } else if let (IdentityFirmware::Remembered(_), FirmwareFace::Unknown) =
        (&identity.firmware, &view.firmware_face)
    {
        // Last known, dimmed (P13); only the Link row says since when.
        stale = true;
        (version.clone(), None, UiStatusKind::Neutral, None)
    } else if view.firmware_face.wants_flash() {
        // Install firmware (`flash`) is the primary. An older LightPlayer
        // whose board is known is offered the USB update instead of a
        // flash: that is the row's answer, here where the face says it is
        // needed.
        let update = input
            .offer("update-firmware")
            .filter(|_| input.offer("flash").is_none())
            .map(|update| {
                let word = match view.firmware_face {
                    FirmwareFace::NoHello => "Install",
                    _ => "Update",
                };
                UiCardAction {
                    word: word.to_string(),
                    ..usb_update(input, update)
                }
            });
        let (name, state) = face_words(&view.firmware_face);
        (name, state, UiStatusKind::Neutral, update)
    } else if let FirmwareFace::CoreOnly {
        version: core,
        state,
    } = &view.firmware_face
    {
        let (name, state, tone) = core_only_words(core.as_deref(), *state);
        (name, state, tone, None)
    } else {
        (version.clone(), None, UiStatusKind::Neutral, None)
    };
    // A flash waiting for the layout question's answer is not working: the
    // row says the confirmation it needs, and Review opens it.
    let work = firmware_work(input).filter(|_| opens_details.is_none());
    let running = work
        .as_ref()
        .is_some_and(|work| work.state == BarWorkState::Running);
    // A flash on a board with no LightPlayer yet: the work's words take the
    // name's place ("installing · 30%").
    let summary = match (running, view.firmware_face.wants_flash()) {
        (true, true) => String::new(),
        _ => summary,
    };
    UiStackBar {
        details: details(input, &version, tone, action.as_ref()),
        aside: aside.filter(|_| !running).map(str::to_string),
        tone: if running { UiStatusKind::Neutral } else { tone },
        action: if running { None } else { action },
        opens_details: opens_details.filter(|_| !running),
        stale,
        work,
        ..UiStackBar::new(BarLayer::Firmware, "firmware", summary)
    }
}

/// A new board's Firmware row: "unknown" while it identifies; once it
/// settles, what the verdict found — "none" on a blank chip, "unknown
/// (pre-2026.07)" on a LightPlayer too old to say hello. Quiet, with no
/// action: Install firmware is the primary.
pub(crate) fn pending_firmware_bar(pending: &PendingLinkView) -> UiStackBar {
    let face = &pending.firmware_face;
    let (summary, aside, tone) = match face {
        FirmwareFace::Unknown => (FIRMWARE_UNKNOWN.to_string(), None, UiStatusKind::Neutral),
        // A core with no engine says no hello, so it waits here: the same
        // words as the board's own card once it is kept.
        FirmwareFace::CoreOnly { version, state } => core_only_words(version.as_deref(), *state),
        face if face.wants_flash() => {
            let (summary, aside) = face_words(face);
            (summary, aside, UiStatusKind::Neutral)
        }
        face => (
            firmware_label_version(&device_firmware_line(face, None)),
            None,
            UiStatusKind::Neutral,
        ),
    };
    let mut said = facts("Firmware", Vec::new());
    said.sentence = Some(pending_firmware_line(face));
    UiStackBar {
        aside: aside.map(str::to_string),
        tone,
        details: UiBarDetails {
            sections: vec![said],
            panels: Vec::new(),
            raised: false,
        },
        ..UiStackBar::new(BarLayer::Firmware, "firmware", summary)
    }
}

/// A core with no engine, as the row's name, state and tone: its version
/// and "incomplete" (orange) while it waits for its engine, "crashing"
/// (red) when the engine keeps crashing, "updating" while it finishes.
fn core_only_words(
    core: Option<&str>,
    state: UpdateBoardState,
) -> (String, Option<&'static str>, UiStatusKind) {
    let name = core.map_or_else(|| FIRMWARE_UNKNOWN.to_string(), firmware_version);
    match state {
        UpdateBoardState::EngineCrashing => (name, Some(CRASHING), UiStatusKind::Error),
        UpdateBoardState::Updating | UpdateBoardState::OnTrial => {
            (name, Some("updating"), UiStatusKind::Neutral)
        }
        UpdateBoardState::NeedsEngine | UpdateBoardState::Running | UpdateBoardState::Unknown => {
            (name, Some(INCOMPLETE), UiStatusKind::Attention)
        }
    }
}

/// The Firmware row's work: a flash, an erase or an update running (the
/// update as one number), or an update story in progress with no activity
/// of its own here (another device's, one about to start).
fn firmware_work(input: &BoardCardInput<'_>) -> Option<UiBarWork> {
    if let Some(work) = bar_work(input, BarLayer::Firmware) {
        return Some(work);
    }
    let update = input
        .update
        .filter(|update| update.kind == UpdateRowKind::Progress)?;
    let progress = update.progress?;
    let (verb, percent) = update_work(&update.standing)?;
    Some(UiBarWork {
        other_device: progress.other_device,
        ..running_work(verb, percent)
    })
}

/// The version alone, in Studio's one firmware version: the update story's
/// version when there is one, else the version the board said hello with
/// (or the one it is remembered by), else "unknown".
fn version_alone(input: &BoardCardInput<'_>, firmware: &IdentityFirmware) -> String {
    if let Some(update) = input.update {
        return update.version.text.clone();
    }
    firmware
        .label()
        .map_or_else(|| FIRMWARE_UNKNOWN.to_string(), firmware_label_version)
}

/// Whether the board is older than this Studio, so the USB update is the
/// row's action rather than a verb in its details: an update story that
/// says one is available, or a face older than Studio by its version (or,
/// with no version to compare, its wire).
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

/// A face that wants a flash, as the row's name and state: "none" on a
/// blank chip, "unknown (pre-2026.07)" on a LightPlayer too old to say
/// hello, what else it runs otherwise.
fn face_words(face: &FirmwareFace) -> (String, Option<&'static str>) {
    match face {
        FirmwareFace::Blank => (FIRMWARE_NONE.to_string(), None),
        FirmwareFace::NoHello => (PRE_HELLO_FIRMWARE.to_string(), None),
        FirmwareFace::Bootloader => (FIRMWARE_UNKNOWN.to_string(), Some("download mode")),
        FirmwareFace::Foreign { label } => (
            label
                .clone()
                .unwrap_or_else(|| "other firmware".to_string()),
            None,
        ),
        FirmwareFace::Silent => (FIRMWARE_UNKNOWN.to_string(), Some("no response")),
        FirmwareFace::OlderLightPlayer { .. } => {
            (FIRMWARE_UNKNOWN.to_string(), Some("older LightPlayer"))
        }
        FirmwareFace::Unknown
        | FirmwareFace::LightPlayer { .. }
        | FirmwareFace::CoreOnly { .. } => (FIRMWARE_UNKNOWN.to_string(), None),
    }
}

/// The Firmware row's details: the notice, the firmware's facts, its
/// panels, every firmware verb not drawn as the action, and Factory reset
/// apart. A board whose data was lost has its ways back on the Project
/// row's details (O3), not here.
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
    let lost = data_lost(input);

    // 1. The notice. A refused store's line is the notice, before any
    // update story.
    let refused = layout.is_some_and(|layout| layout.store_refused);
    // The row's action is the update (or its Unlock): the story says what
    // it brings.
    let offers_the_update = action.is_some_and(|action| {
        ["update-firmware", "unlock"]
            .into_iter()
            .filter_map(|verb| input.offer(verb))
            .any(|offer| offer.path == action.offer)
    });
    let story = update.filter(|update| {
        !refused
            && (update.kind == UpdateRowKind::NeedsYou
                || (offers_the_update
                    && matches!(
                        update.standing,
                        UpdateStanding::Available { .. } | UpdateStanding::PlayOnly { .. }
                    )))
    });
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
    } else if let Some(line) = layout
        .and_then(|layout| layout.line.clone())
        .filter(|_| !lost)
    {
        sections.push(notice(
            "Firmware",
            UiStatusKind::Attention,
            line,
            action.cloned(),
        ));
    } else if view.firmware_face.wants_flash() {
        // Quiet: Install firmware is the primary; the face's verdict is
        // the details' sentence.
        let board = device_identity_line(view).board;
        sections.push(notice(
            "Firmware",
            UiStatusKind::Neutral,
            device_firmware_line(&view.firmware_face, board.as_deref()),
            None,
        ));
    } else if tone != UiStatusKind::Neutral {
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
        .map(|update| update.sentence.clone())
        .or_else(|| face_verdict(view, update, story.is_some()));
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
    if let Some(restore) = input.offer("restore-from-file").filter(|_| !lost) {
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
        verb(input.offer("finish-update"), Some("Resume"), "download"),
        update_verb,
        verb(
            input.offer("download-backup").filter(|_| !lost),
            None,
            "download",
        ),
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

/// How a running LightPlayer's firmware compares with this Studio's, in
/// today's firmware line's words ("fw-esp32c6 abc1234 — older than Studio,
/// update recommended", "… — newer than Studio"), when no update story says
/// it: a board with no standing (another target, or no facts about this
/// Studio's own build) would otherwise say it nowhere.
fn face_verdict(
    view: &lpa_devices::view::DeviceView,
    update: Option<&crate::UiDeviceUpdate>,
    noticed: bool,
) -> Option<String> {
    if update.is_some() || noticed {
        return None;
    }
    let FirmwareFace::LightPlayer { .. } = view.firmware_face else {
        return None;
    };
    let line = device_firmware_line(&view.firmware_face, None);
    line.contains(" — ").then_some(line)
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
        assert_eq!(work.words, "installing · 62%");
        assert!(work.cancel.is_some());
        assert_eq!(bar.action, None);
        assert_eq!(bar.tone, UiStatusKind::Neutral);
    }

    /// The update is the row's work while it runs, one number for the
    /// whole of it (never "1 of 2"), and the version stays the name.
    #[test]
    fn an_update_is_the_rows_work_as_one_number() {
        for (row, words, other) in [
            (UpdateFixtureRow::BackingUp, "backing up · 18%", false),
            (UpdateFixtureRow::Updating, "updating · 20%", false),
            (UpdateFixtureRow::Finishing, "updating · 85%", false),
            (UpdateFixtureRow::FinishingResumed, "resuming · 85%", false),
            (UpdateFixtureRow::Restoring, "restoring · 35%", false),
            (UpdateFixtureRow::AnotherDevice, "updating · 40%", true),
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

    /// With no update story, how the board's firmware compares with this
    /// Studio's is still said: the firmware details carry today's line —
    /// older (the bar blue with Update), not this build, or newer.
    #[test]
    fn a_running_boards_verdict_is_said_without_an_update_story() {
        for (age, says) in [
            (FirmwareAge::Older, "older than Studio, update recommended"),
            (
                FirmwareAge::Different,
                "not this Studio's build, update recommended",
            ),
            (FirmwareAge::Newer, "newer than Studio"),
        ] {
            let mut fixture = CardFixture::ready();
            fixture.view.firmware_face = FirmwareFace::LightPlayer {
                firmware: Some("fw-esp32c6 abc1234".to_string()),
                wire: WireVersion::Match,
                age,
            };
            let bar = firmware_bar(&fixture.input());
            let sentence = bar
                .details
                .sections
                .iter()
                .find_map(|section| section.sentence.clone())
                .unwrap_or_else(|| panic!("{age:?} is said"));
            assert!(sentence.contains(says), "{age:?}: {sentence}");
        }
        // A current board says nothing more than its version.
        let mut current = CardFixture::ready();
        assert!(
            firmware_bar(&current.input())
                .details
                .sections
                .iter()
                .all(|section| section.sentence.is_none())
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

    /// Broken firmware is red, the state a word: "crashing", with
    /// Reinstall.
    #[test]
    fn a_crashing_board_is_red_and_offers_reinstall() {
        let mut fixture = story(UpdateFixtureRow::KeepsCrashing);
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.summary, "v2026.10.03-1");
        assert_eq!(bar.aside.as_deref(), Some("crashing"));
        assert_eq!(bar.tone, UiStatusKind::Error);
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
        assert_eq!(bar.aside.as_deref(), Some("incomplete"));
        let install = bar.action.clone().expect("Install <own>");
        assert_eq!(
            install.word, "Install",
            "the version it brings is the details'"
        );
        assert_eq!(line(&bar, "Newest").as_deref(), Some("v2026.10.05-2"));
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

    /// A core from before over-the-air updates says so after its version,
    /// in place (P3), untinted, with Update by USB.
    #[test]
    fn a_pre_ota_board_updates_by_usb_once() {
        let mut fixture = story(UpdateFixtureRow::NeedsUsbOnce);
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.summary, "v2026.10.03-1 (pre-OTA)");
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        let update = bar.action.expect("Update");
        assert_eq!(update.word, "Update");
        assert_eq!(update.icon.as_deref(), Some("usb"), "the USB icon says how");
        assert!(update.offer.to_string().ends_with("/update-firmware"));
    }

    /// Ported: the version is set apart as the version alone — never the
    /// line's "X → Y available", the board or the MAC.
    #[test]
    fn the_update_line_sets_the_version_apart() {
        let mut fixture = story(UpdateFixtureRow::Available);
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.summary, "v2026.10.03-1");
        assert_eq!(bar.kind, Some("Firmware"));
        assert!(!bar.summary.contains('→'));
        assert!(!bar.summary.contains("a0:f2"));
        assert_eq!(
            bar.tone,
            UiStatusKind::Neutral,
            "no tone: the button carries it (P16)"
        );
        let update = bar.action.clone().expect("Update");
        assert_eq!(update.word, "Update");
        assert_eq!(update.draw, UiActionDraw::Press);
        assert!(update.offer.to_string().ends_with("/update-firmware"));
        assert_eq!(line(&bar, "Newest").as_deref(), Some("v2026.10.05-2"));
        assert_eq!(line(&bar, "Version").as_deref(), Some("v2026.10.03-1"));
        assert_eq!(line(&bar, "Commit").as_deref(), Some("a41c9e2"));
        assert!(
            line(&bar, "Build").is_some_and(|build| build.starts_with("2026.10.03-1+")),
            "the raw build in details"
        );
        assert_eq!(
            bar.details.notice().map(|notice| notice.tone),
            Some(UiStatusKind::Neutral)
        );
    }

    #[test]
    fn an_older_board_by_usb_offers_update_untinted() {
        let mut fixture = CardFixture::ready();
        fixture.view.firmware_face = FirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 2026.10.01-1".to_string()),
            wire: WireVersion::Match,
            age: FirmwareAge::Older,
        };
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.summary, "v2026.10.01-1", "never the target prefix");
        assert_eq!(bar.tone, UiStatusKind::Neutral);
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
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        let update = bar.action.expect("Update");
        assert_eq!(update.word, "Update");
        assert_eq!(update.icon.as_deref(), Some("lock"));
        assert_eq!(update.draw, UiActionDraw::Sheet);
        assert!(update.offer.to_string().ends_with("/unlock"));
    }

    /// Offline, the version remembered stays, dimmed (P13); only the Link
    /// row says since when.
    #[test]
    fn an_offline_board_keeps_its_remembered_version_dimmed() {
        let mut fixture = CardFixture::offline();
        fixture.view.remembered_firmware = Some("fw-esp32c6 2026.10.05-2".to_string());
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.summary, "v2026.10.05-2");
        assert_eq!(bar.aside, None);
        assert!(bar.stale);
        assert_eq!(bar.tone, UiStatusKind::Neutral);
    }

    /// No LightPlayer on it is no alarm: the row says what is there,
    /// quietly, and Install firmware is the primary.
    #[test]
    fn a_face_that_wants_a_flash_says_what_is_there_quietly() {
        for (face, name, state) in [
            (FirmwareFace::Blank, "none", None),
            (FirmwareFace::NoHello, "unknown (pre-2026.07)", None),
            (FirmwareFace::Bootloader, "unknown", Some("download mode")),
            (
                FirmwareFace::Foreign { label: None },
                "other firmware",
                None,
            ),
            (FirmwareFace::Silent, "unknown", Some("no response")),
            (
                FirmwareFace::OlderLightPlayer { proto: None },
                "unknown",
                Some("older LightPlayer"),
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
            assert_eq!(bar.summary, name, "{face:?}");
            assert_eq!(bar.aside.as_deref(), state, "{face:?}");
            assert_eq!(bar.tone, UiStatusKind::Neutral, "{face:?}");
            assert_eq!(bar.action, None, "Install firmware is the primary");
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
        assert_eq!(bar.aside.as_deref(), Some("older LightPlayer"));
        assert_eq!(bar.tone, UiStatusKind::Neutral, "the button carries it");
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
        assert_eq!(
            bar.summary, "v2026.10.05-2",
            "the version, never the build label"
        );
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
    fn nothing_known_names_what_is_not_known() {
        let mut fixture = CardFixture::ready();
        fixture.view.firmware_face = FirmwareFace::Unknown;
        assert_eq!(firmware_bar(&fixture.input()).summary, "unknown");
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
    fn a_new_board_is_unknown_then_none() {
        let mut pending = pending_view();
        let bar = pending_firmware_bar(&pending);
        assert_eq!(bar.summary, "unknown");
        assert_eq!(bar.tone, UiStatusKind::Neutral);
        pending.firmware_face = FirmwareFace::Blank;
        let bar = pending_firmware_bar(&pending);
        assert_eq!(bar.summary, "none");
        assert_eq!(
            bar.tone,
            UiStatusKind::Neutral,
            "Install firmware is the primary"
        );
        assert_eq!(
            bar.details.sections[0].sentence.as_deref(),
            Some("Blank flash — needs firmware")
        );
        assert_eq!(bar.action, None);
        pending.firmware_face = FirmwareFace::NoHello;
        assert_eq!(
            pending_firmware_bar(&pending).summary,
            "unknown (pre-2026.07)"
        );
    }

    /// A core with no engine says no hello, so it waits as a new board:
    /// its row reads as the kept card's will, the version and "incomplete".
    #[test]
    fn a_new_core_only_board_is_incomplete() {
        let mut pending = pending_view();
        pending.firmware_face = FirmwareFace::CoreOnly {
            version: Some("2026.10.16-1".to_string()),
            state: UpdateBoardState::NeedsEngine,
        };
        let bar = pending_firmware_bar(&pending);
        assert_eq!(bar.summary, "v2026.10.16-1");
        assert_eq!(bar.aside.as_deref(), Some("incomplete"));
        assert_eq!(bar.tone, UiStatusKind::Attention);
        pending.firmware_face = FirmwareFace::CoreOnly {
            version: None,
            state: UpdateBoardState::EngineCrashing,
        };
        let bar = pending_firmware_bar(&pending);
        assert_eq!(bar.summary, "unknown");
        assert_eq!(bar.aside.as_deref(), Some("crashing"));
        assert_eq!(bar.tone, UiStatusKind::Error);
    }

    /// The layout question is a confirmation the row asks for: orange,
    /// "confirmation required", and Review opens the details that rise.
    #[test]
    fn the_layout_question_is_a_confirmation_with_review() {
        let mut fixture = CardFixture::ready();
        let mut layout = crate::UiDeviceLayout::note_only(fixture.board.clone(), "studio-b");
        layout.project_note = None;
        layout.panel = Some(crate::UiLayoutPanel {
            title: "Move this board's files?".to_string(),
            body: String::new(),
            warning: None,
            download: fixture.board.clone().child("download-backup"),
            continue_action: None,
            cancel: None,
        });
        fixture.layout = Some(layout);
        let bar = firmware_bar(&fixture.input());
        assert_eq!(bar.aside.as_deref(), Some(CONFIRMATION_REQUIRED));
        assert_eq!(bar.tone, UiStatusKind::Attention);
        assert_eq!(bar.opens_details, Some("Review"));
        assert!(bar.details.raised);
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
