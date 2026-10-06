//! The words for each [`FirmwareFace`]: the FIRMWARE zone's info line, the
//! pending link's, and the preview slot's sentence when there is no
//! picture because of what is on the flash.
//!
//! In core rather than in the renderer for the usual reason — these are
//! decisions, and decisions get tests, one per variant. The renderer used to
//! own a `BLANK_FLASH_LINE` constant and three `if needs_firmware` branches,
//! and every needs-firmware verdict drew as "Blank flash" (bench 2026-09-04:
//! a running board one wire version behind Studio). A match here is
//! exhaustive, so the next variant is a compile error, not a silent blank.

use lpa_devices::view::FirmwareFace;
use lpa_devices::{FirmwareAge, UpdateBoardState, WireVersion};

/// The FIRMWARE zone's info line on a device card: the firmware label
/// (which leads with the version) joined to the board it was built for (the
/// header's already-resolved display name, so the two name the board
/// identically), with how it stands against this Studio when that is known;
/// or the verdict that asks for a flash, in the words of THAT verdict.
///
/// "Older than Studio" follows the VERSION ([`FirmwareAge`]). The wire proto
/// speaks only when no version can be compared (a board or a Studio that
/// reports none), as it did before boards said their version.
///
/// User words, not wire words: "older than Studio", never "proto 19"
/// (the terminal carries the numbers).
pub fn device_firmware_line(face: &FirmwareFace, board: Option<&str>) -> String {
    match face {
        FirmwareFace::Unknown => "No firmware reported yet".to_string(),
        FirmwareFace::LightPlayer {
            firmware,
            wire,
            age,
        } => {
            let parts: Vec<&str> = firmware.as_deref().into_iter().chain(board).collect();
            let mut line = if parts.is_empty() {
                "No firmware reported yet".to_string()
            } else {
                parts.join(" · ")
            };
            let standing = match (age, wire) {
                (FirmwareAge::Older, _)
                | (FirmwareAge::Unknown, WireVersion::BoardOlder { .. }) => {
                    Some(" — older than Studio, update recommended")
                }
                (FirmwareAge::Different, _) => {
                    Some(" — not this Studio's build, update recommended")
                }
                (FirmwareAge::Newer, _)
                | (FirmwareAge::Unknown, WireVersion::BoardNewer { .. }) => {
                    Some(" — newer than Studio")
                }
                (FirmwareAge::Current, _) | (FirmwareAge::Unknown, WireVersion::Match) => None,
            };
            if let Some(standing) = standing {
                line.push_str(standing);
            }
            line
        }
        // A board running only its core, in the words of the update
        // standing's rows for what it is doing. When this Studio can tell
        // its update story (its own build is known), the card reads the
        // standing's words instead (`device_update_words`).
        FirmwareFace::CoreOnly { version, state } => core_only_line(version.as_deref(), *state),
        FirmwareFace::NoHello => "Pre-hello firmware — needs firmware".to_string(),
        FirmwareFace::OlderLightPlayer { .. } => {
            "Older LightPlayer firmware — flash to update; the project stays".to_string()
        }
        FirmwareFace::Blank => "Blank flash — needs firmware".to_string(),
        FirmwareFace::Bootloader => "Waiting in ROM download mode — needs firmware".to_string(),
        FirmwareFace::Foreign { label: Some(label) } => {
            format!("{label} — replace with LightPlayer firmware")
        }
        FirmwareFace::Foreign { label: None } => {
            "Unrecognized firmware — replace with LightPlayer firmware".to_string()
        }
        FirmwareFace::Silent => "No response — try flashing firmware".to_string(),
    }
}

/// A core-only board's line: its firmware keeps crashing, it is mid-update,
/// or it waits for its firmware.
fn core_only_line(version: Option<&str>, state: UpdateBoardState) -> String {
    let version = version.unwrap_or("LightPlayer");
    match state {
        UpdateBoardState::EngineCrashing => format!("{version} keeps crashing"),
        UpdateBoardState::Updating | UpdateBoardState::OnTrial => {
            "Finishing the update…".to_string()
        }
        UpdateBoardState::NeedsEngine | UpdateBoardState::Running | UpdateBoardState::Unknown => {
            format!("{version} · waiting for its firmware")
        }
    }
}

/// The FIRMWARE zone's info line on a PENDING link: the settled verdict's
/// words, or the honest "nothing is known yet" while it identifies. A
/// pending link has no board of record, so no board joins the line.
pub fn pending_firmware_line(face: &FirmwareFace) -> String {
    match face {
        FirmwareFace::Unknown => "Firmware unknown until this board identifies".to_string(),
        settled => device_firmware_line(settled, None),
    }
}

/// The preview slot's sentence when what is on the flash is the reason
/// there is no picture. `None` for a LightPlayer (the project decides) and
/// while nothing is settled.
pub fn firmware_face_preview_sentence(face: &FirmwareFace) -> Option<String> {
    let sentence = match face {
        FirmwareFace::Unknown | FirmwareFace::LightPlayer { .. } => return None,
        // The update-states spike's sentences for a stopped show.
        FirmwareFace::CoreOnly {
            state: UpdateBoardState::Updating | UpdateBoardState::OnTrial,
            ..
        } => "Updating… the picture returns when the board does.",
        FirmwareFace::CoreOnly { .. } => "Not running — the board is waiting for its firmware.",
        FirmwareFace::NoHello => "No picture — this firmware is too old to say what it runs.",
        FirmwareFace::OlderLightPlayer { .. } => {
            "No picture — this LightPlayer firmware is too old for this Studio."
        }
        FirmwareFace::Blank => "Nothing running — a blank chip has no picture.",
        FirmwareFace::Bootloader => "Nothing running — the board is waiting in its bootloader.",
        FirmwareFace::Foreign { .. } => "No picture — this board is not running LightPlayer.",
        FirmwareFace::Silent => "No picture — the board is not responding.",
    };
    Some(sentence.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light_player(firmware: Option<&str>, wire: WireVersion) -> FirmwareFace {
        FirmwareFace::LightPlayer {
            firmware: firmware.map(str::to_string),
            wire,
            age: FirmwareAge::Unknown,
        }
    }

    fn aged(firmware: &str, age: FirmwareAge) -> FirmwareFace {
        FirmwareFace::LightPlayer {
            firmware: Some(firmware.to_string()),
            wire: WireVersion::Match,
            age,
        }
    }

    /// "Out of date" follows the VERSION: an older release reads older than
    /// Studio and recommends the update, a newer one says so and recommends
    /// nothing, the same build says nothing.
    #[test]
    fn older_than_studio_follows_the_version() {
        let board = Some("XIAO ESP32-C6");
        assert_eq!(
            device_firmware_line(
                &aged("fw-esp32c6 2026.10.02-3 · abc1234def01", FirmwareAge::Older),
                board
            ),
            "fw-esp32c6 2026.10.02-3 · abc1234def01 · XIAO ESP32-C6 — older than Studio, \
             update recommended"
        );
        assert_eq!(
            device_firmware_line(&aged("fw-esp32c6 2026.10.04-1", FirmwareAge::Newer), None),
            "fw-esp32c6 2026.10.04-1 — newer than Studio"
        );
        assert_eq!(
            device_firmware_line(&aged("fw-esp32c6 2026.10.03-1", FirmwareAge::Current), None),
            "fw-esp32c6 2026.10.03-1"
        );
        assert_eq!(
            device_firmware_line(&aged("fw-esp32c6 626a1b851", FirmwareAge::Different), None),
            "fw-esp32c6 626a1b851 — not this Studio's build, update recommended"
        );
    }

    /// The version outranks the wire proto: a board on an older proto whose
    /// version is this Studio's own is not called older, and a board on the
    /// same proto with an older version is.
    #[test]
    fn the_version_outranks_the_wire_proto() {
        let current_on_old_wire = FirmwareFace::LightPlayer {
            firmware: Some("fw-esp32c6 2026.10.03-1".to_string()),
            wire: WireVersion::BoardOlder {
                board: 33,
                studio: 34,
            },
            age: FirmwareAge::Current,
        };
        assert_eq!(
            device_firmware_line(&current_on_old_wire, None),
            "fw-esp32c6 2026.10.03-1"
        );
        assert!(
            device_firmware_line(&aged("fw-esp32c6 2026.10.01-1", FirmwareAge::Older), None)
                .contains("older than Studio")
        );
    }

    /// The bench case: an older board names its firmware AND says it is
    /// older, in one line, in user words.
    #[test]
    fn an_older_light_player_names_its_firmware_and_says_it_is_older() {
        let face = light_player(
            Some("fw-esp32v3 7c80a27"),
            WireVersion::BoardOlder {
                board: 19,
                studio: 20,
            },
        );
        assert_eq!(
            device_firmware_line(&face, Some("QuinLED-Dig-Uno")),
            "fw-esp32v3 7c80a27 · QuinLED-Dig-Uno — older than Studio, update recommended"
        );
        assert!(
            !device_firmware_line(&face, None).contains("proto"),
            "wire numbers stay in the terminal"
        );
    }

    #[test]
    fn a_newer_light_player_says_so_without_recommending_anything() {
        let face = light_player(
            Some("fw-esp32c6 abc1234"),
            WireVersion::BoardNewer {
                board: 21,
                studio: 20,
            },
        );
        assert_eq!(
            device_firmware_line(&face, None),
            "fw-esp32c6 abc1234 — newer than Studio"
        );
    }

    #[test]
    fn a_current_light_player_reads_firmware_then_board() {
        let face = light_player(Some("fw-esp32c6 0.9.3"), WireVersion::Match);
        assert_eq!(
            device_firmware_line(&face, Some("XIAO ESP32-C6")),
            "fw-esp32c6 0.9.3 · XIAO ESP32-C6"
        );
        // A board that hello'd but reported no firmware label still names
        // the board rather than dropping to the honest-nothing line.
        assert_eq!(
            device_firmware_line(
                &light_player(None, WireVersion::Match),
                Some("XIAO ESP32-C6")
            ),
            "XIAO ESP32-C6"
        );
        assert_eq!(
            device_firmware_line(&light_player(None, WireVersion::Match), None),
            "No firmware reported yet"
        );
    }

    /// Every face that wants a flash says so in ITS OWN words — never the
    /// blank chip's.
    #[test]
    fn every_flash_face_has_its_own_words() {
        let faces = [
            FirmwareFace::NoHello,
            FirmwareFace::OlderLightPlayer { proto: Some(29) },
            FirmwareFace::Blank,
            FirmwareFace::Bootloader,
            FirmwareFace::Foreign {
                label: Some("Seeed XIAO factory firmware".to_string()),
            },
            FirmwareFace::Foreign { label: None },
            FirmwareFace::Silent,
        ];
        let mut lines = std::collections::BTreeSet::new();
        for face in &faces {
            assert!(face.wants_flash(), "{face:?}");
            let line = device_firmware_line(face, Some("XIAO ESP32-C6"));
            assert!(!line.is_empty());
            assert!(
                line.contains("firmware") || line.contains("flash"),
                "{face:?} → {line}"
            );
            assert!(
                lines.insert(line.clone()),
                "{face:?} repeats a line: {line}"
            );
        }
        assert_eq!(
            device_firmware_line(&FirmwareFace::Blank, None),
            "Blank flash — needs firmware"
        );
        assert_eq!(
            device_firmware_line(&FirmwareFace::Unknown, Some("XIAO ESP32-C6")),
            "No firmware reported yet"
        );
    }

    /// A board still on firmware from before the USB link moved onto
    /// lp-link: the card names it LightPlayer, asks for a flash, and says
    /// the project survives it — it used to read "Unrecognized firmware".
    #[test]
    fn older_light_player_firmware_asks_for_a_flash_that_keeps_the_project() {
        let face = FirmwareFace::OlderLightPlayer { proto: None };
        assert!(face.wants_flash());
        assert_eq!(
            pending_firmware_line(&face),
            "Older LightPlayer firmware — flash to update; the project stays"
        );
        assert_eq!(
            firmware_face_preview_sentence(&face).as_deref(),
            Some("No picture — this LightPlayer firmware is too old for this Studio.")
        );
    }

    /// A board running only its core never reads as a flash face: it says
    /// what it is doing, in the update standing's words.
    #[test]
    fn a_core_only_board_says_what_it_is_doing() {
        let face = |state| FirmwareFace::CoreOnly {
            version: Some("2026.10.05-2".to_string()),
            state,
        };
        assert!(!face(UpdateBoardState::NeedsEngine).wants_flash());
        assert_eq!(
            device_firmware_line(&face(UpdateBoardState::EngineCrashing), None),
            "2026.10.05-2 keeps crashing"
        );
        assert_eq!(
            device_firmware_line(&face(UpdateBoardState::NeedsEngine), None),
            "2026.10.05-2 · waiting for its firmware"
        );
        assert_eq!(
            device_firmware_line(&face(UpdateBoardState::OnTrial), None),
            "Finishing the update…"
        );
        assert_eq!(
            firmware_face_preview_sentence(&face(UpdateBoardState::NeedsEngine)).as_deref(),
            Some("Not running — the board is waiting for its firmware.")
        );
        assert_eq!(
            firmware_face_preview_sentence(&face(UpdateBoardState::Updating)).as_deref(),
            Some("Updating… the picture returns when the board does.")
        );
    }

    #[test]
    fn a_pending_link_is_honest_until_it_settles() {
        assert_eq!(
            pending_firmware_line(&FirmwareFace::Unknown),
            "Firmware unknown until this board identifies"
        );
        assert_eq!(
            pending_firmware_line(&FirmwareFace::Blank),
            "Blank flash — needs firmware"
        );
    }

    #[test]
    fn only_the_flash_faces_explain_the_missing_picture() {
        assert_eq!(firmware_face_preview_sentence(&FirmwareFace::Unknown), None);
        assert_eq!(
            firmware_face_preview_sentence(&light_player(
                Some("fw-esp32v3 x"),
                WireVersion::BoardOlder {
                    board: 19,
                    studio: 20
                }
            )),
            None,
            "an older LightPlayer's picture is its project's business"
        );
        assert_eq!(
            firmware_face_preview_sentence(&FirmwareFace::Blank).as_deref(),
            Some("Nothing running — a blank chip has no picture.")
        );
        for face in [
            FirmwareFace::NoHello,
            FirmwareFace::Bootloader,
            FirmwareFace::Foreign { label: None },
            FirmwareFace::Silent,
        ] {
            let sentence = firmware_face_preview_sentence(&face).expect("a sentence");
            assert!(!sentence.contains("blank"), "{face:?} → {sentence}");
        }
    }
}
