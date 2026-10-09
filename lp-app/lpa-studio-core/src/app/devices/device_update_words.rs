//! The words for every update standing: the firmware zone's line, the
//! picture slot's sentence (also the line's hover), the light the slot
//! shows when the show has stopped, the header chip, and the editor
//! popover's run word and stat line.
//!
//! Verbatim from the update-states spike, direction C (DS16): the version
//! reads as a version ([`super::device_update_version`]), `<link>` is
//! "USB" or "Bluetooth". In core because they are decisions with a test
//! each; the web only lays them out. Data only — the buttons are offers
//! ([`super::device_update_offers`]).

use super::device_update_route::UpdateLink;
use super::device_update_standing::UpdateStanding;
use super::device_update_version::{UpdateVersion, UpdateVersionDisplay};

/// The three kinds a row reads as, by tone alone: a lit bar is progress,
/// needs-you text is orange, information is plain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateRowKind {
    /// Something is happening; nobody needs to act.
    Progress,
    /// Nothing moves until a person does something.
    NeedsYou,
    /// True and worth knowing; nothing to do.
    Information,
}

/// What the board's own lights show while the show has stopped — and so
/// what the card's picture slot shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateLight {
    /// Updating: firmware is being written, finished or put back.
    DarkYellow,
    /// Waiting for its firmware.
    DarkRed,
}

/// The firmware zone's bar while an update runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpdateProgress {
    /// `None` before the first progress report.
    pub percent: Option<u8>,
    /// Another device is doing it (the bar reads as someone else's).
    pub other_device: bool,
}

/// A device card's update words. See the module docs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiDeviceUpdate {
    pub kind: UpdateRowKind,
    /// The firmware zone's line.
    pub line: String,
    /// The whole sentence: the picture slot's when the show has stopped,
    /// and the line's hover.
    pub sentence: String,
    /// The picture slot's light; `None` while the show runs.
    pub light: Option<UpdateLight>,
    /// The header chip's word.
    pub chip: String,
    /// The version the header's second identity row names.
    pub version: UpdateVersionDisplay,
    /// The bar, while an update runs or is about to.
    pub progress: Option<UpdateProgress>,
}

/// The tone the popover's run word reads in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateRunTone {
    /// An update on offer.
    Live,
    /// An update running.
    Working,
    /// Waiting for a person.
    Attention,
}

/// The run word the editor's device popover shows instead of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateRunWord {
    pub text: String,
    pub tone: UpdateRunTone,
}

/// The editor's device popover, for a board with an update story.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSessionUpdate {
    /// `None`: the popover's own run word ("running") stands.
    pub run_word: Option<UpdateRunWord>,
    /// `<chip> · X → Y · <mac>` (or `… · Y keeps crashing · …`).
    pub stat_line: String,
}

/// The card's words for `standing`, or `None` when it tells no update
/// story (the card is today's).
pub fn update_words(standing: &UpdateStanding) -> Option<UiDeviceUpdate> {
    use UpdateRowKind::{Information, NeedsYou, Progress};
    let board = standing.board()?;
    let version = board.display();
    let words = |kind, line: String, sentence: String, light, chip: &str| UiDeviceUpdate {
        kind,
        line,
        sentence,
        light,
        chip: chip.to_string(),
        version: version.clone(),
        progress: None,
    };
    let progress = |percent, other_device| {
        Some(UpdateProgress {
            percent,
            other_device,
        })
    };
    let yellow = Some(UpdateLight::DarkYellow);
    let red = Some(UpdateLight::DarkRed);
    Some(match standing {
        UpdateStanding::Nothing => return None,
        UpdateStanding::UpToDate { version } if version.is_dev() => words(
            Information,
            format!("{} · same as this Studio", version.short()),
            format!("{}, the same as this Studio.", capitalized(&version.long())),
            None,
            "Ready",
        ),
        UpdateStanding::UpToDate { version } => words(
            Information,
            format!("{} · up to date", version.short()),
            format!("{}, the same as this Studio.", version.short()),
            None,
            "Ready",
        ),
        UpdateStanding::Available { board, to } => {
            let (line, sentence) = available(board, to);
            words(Information, line, sentence, None, "Ready")
        }
        UpdateStanding::BackingUp { percent, .. } => UiDeviceUpdate {
            progress: progress(*percent, false),
            ..words(
                Progress,
                format!("Backing up current firmware…{}", pct(*percent)),
                running_sentence(
                    "Backing up current firmware…",
                    *percent,
                    "The show keeps running meanwhile.",
                ),
                None,
                "Backing up",
            )
        },
        UpdateStanding::Updating {
            to, link, percent, ..
        } => UiDeviceUpdate {
            progress: progress(*percent, false),
            ..words(
                Progress,
                format!("Updating over {}…{}", link.word(), pct(*percent)),
                running_sentence(
                    &format!("Updating to {} over {}…", to.short(), link.word()),
                    *percent,
                    "Keep the board powered.",
                ),
                yellow,
                "Updating",
            )
        },
        UpdateStanding::Finishing {
            to,
            percent,
            resumed,
            ..
        } => UiDeviceUpdate {
            progress: progress(*percent, false),
            ..words(
                Progress,
                format!("Finishing the update…{}", pct(*percent)),
                // "Interrupted" only when this Studio found the update
                // half-way; the last phase of an update it ran is ordinary.
                if *resumed {
                    running_sentence(
                        &format!("Finishing the update to {}…", to.short()),
                        *percent,
                        "It was interrupted; this Studio is completing it.",
                    )
                } else {
                    running_sentence(
                        "Installing the rest of the firmware…",
                        *percent,
                        "Keep the board powered.",
                    )
                },
                yellow,
                "Updating",
            )
        },
        UpdateStanding::Restoring { board, percent, .. } => UiDeviceUpdate {
            progress: progress(*percent, false),
            ..words(
                Progress,
                format!("Restoring firmware…{}", pct(*percent)),
                running_sentence(
                    &format!("Restoring firmware {}…", board.short()),
                    *percent,
                    "Part of it was missing; this Studio had a copy.",
                ),
                yellow,
                "Restoring firmware",
            )
        },
        UpdateStanding::AnotherDevice { to, percent, .. } => {
            let head = match to {
                Some(to) => format!("Another device is updating this board to {}…", to.short()),
                None => "Another device is updating this board…".to_string(),
            };
            UiDeviceUpdate {
                progress: progress(*percent, true),
                ..words(
                    Progress,
                    format!("Another device is updating it…{}", pct(*percent)),
                    running_sentence(&head, *percent, "If it stops, this Studio finishes it."),
                    yellow,
                    "Updating",
                )
            }
        }
        UpdateStanding::NeedsUsbOnce { link, .. } => words(
            NeedsYou,
            "Needs one update over USB".to_string(),
            format!(
                "This board can't update over {} yet. Update it over USB once; after that it \
                 updates without a cable.",
                link.word()
            ),
            None,
            "Ready",
        ),
        UpdateStanding::KeepsCrashing { board, .. } => words(
            NeedsYou,
            format!("{} keeps crashing", board.short()),
            format!(
                "{} keeps crashing on this board, so it stopped trying. Reinstall it, or \
                 install another version.",
                board.short()
            ),
            red,
            "Needs firmware",
        ),
        UpdateStanding::CantGetVersion { board, own, .. } => words(
            NeedsYou,
            format!("Needs {}, which Studio can't get", board.short()),
            format!(
                "This board needs {} to start, and this Studio can't get it. Connect to the \
                 internet, or install {} instead.",
                board.long(),
                own.short()
            ),
            red,
            "Needs firmware",
        ),
        UpdateStanding::RolledBack { board, refused } => words(
            Information,
            format!("Back on {} · the update didn't start", board.short()),
            format!(
                "The update to {} didn't start, so the board went back to {}.",
                refused.short(),
                board.short()
            ),
            None,
            "Ready",
        ),
        UpdateStanding::Newer { board, own } => words(
            Information,
            format!("{} · newer than this Studio", board.short()),
            format!(
                "{} is newer than this Studio ({}). Reload Studio to catch up.",
                board.short(),
                own.short()
            ),
            None,
            "Ready",
        ),
        UpdateStanding::NotOverWifiYet {
            link: UpdateLink::Relay,
            ..
        } => words(
            NeedsYou,
            "Update nearby once".to_string(),
            "This board updates via lightplayer.app after one update nearby.".to_string(),
            None,
            "Ready",
        ),
        UpdateStanding::NotOverWifiYet { .. } => words(
            NeedsYou,
            "Update over USB or Bluetooth once".to_string(),
            "This board updates over USB or Bluetooth until it has been updated once.".to_string(),
            None,
            "Ready",
        ),
        UpdateStanding::NoWirelessBuild { link, .. } => words(
            Information,
            format!("Can't update over {} from this Studio", link.word()),
            "This Studio's build can't update the board wirelessly. Update it over USB, or \
             from a Studio that can."
                .to_string(),
            None,
            "Ready",
        ),
        UpdateStanding::PlayOnly { board, to } => words(
            Information,
            format!("{} → {} available", board.short(), to.short()),
            format!(
                "Update available: {} → {}. Installing it needs the author password.",
                board.short(),
                to.short()
            ),
            None,
            "Ready",
        ),
    })
}

/// The editor popover's words for `standing` on a board of `chip` at `mac`,
/// or `None` when it tells no update story.
pub fn update_session_words(
    standing: &UpdateStanding,
    chip: Option<&str>,
    mac: Option<&str>,
) -> Option<UiSessionUpdate> {
    let words = update_words(standing)?;
    let board = standing.board()?;
    let run_word = match words.kind {
        UpdateRowKind::Progress => Some(UpdateRunWord {
            text: match words.progress.and_then(|p| p.percent) {
                Some(percent) => format!("{} · {percent}%", words.chip),
                None => words.chip.clone(),
            },
            tone: UpdateRunTone::Working,
        }),
        UpdateRowKind::NeedsYou if words.light.is_some() => Some(UpdateRunWord {
            text: words.chip.clone(),
            tone: UpdateRunTone::Attention,
        }),
        _ => match standing {
            UpdateStanding::Available { .. } | UpdateStanding::PlayOnly { .. } => {
                Some(UpdateRunWord {
                    text: "running · update available".to_string(),
                    tone: UpdateRunTone::Live,
                })
            }
            _ => None,
        },
    };
    let middle = match standing {
        UpdateStanding::Available { to, .. }
        | UpdateStanding::PlayOnly { to, .. }
        | UpdateStanding::BackingUp { to, .. }
        | UpdateStanding::Updating { to, .. }
        | UpdateStanding::Finishing { to, .. }
        | UpdateStanding::AnotherDevice { to: Some(to), .. } => {
            format!("{} → {}", board.short(), to.short())
        }
        UpdateStanding::KeepsCrashing { .. } => format!("{} keeps crashing", board.short()),
        UpdateStanding::CantGetVersion { .. } => format!("needs {}", board.short()),
        _ => board.short(),
    };
    let stat_line = chip
        .into_iter()
        .map(str::to_string)
        .chain([middle])
        .chain(mac.map(str::to_string))
        .collect::<Vec<_>>()
        .join(" · ");
    Some(UiSessionUpdate {
        run_word,
        stat_line,
    })
}

/// The update-available line and sentence: a release one version behind
/// reads `X → Y available`; a dev build on either side says which build
/// each runs, because two dev builds have no order (spike §3).
fn available(board: &UpdateVersion, to: &UpdateVersion) -> (String, String) {
    if to.is_dev() {
        (
            format!("{} · this Studio is {}", board.short(), to.short()),
            format!(
                "This board runs {}; this Studio is {}.",
                board.long(),
                to.long()
            ),
        )
    } else if board.is_dev() {
        (
            format!("{} · Studio has {}", board.short(), to.short()),
            format!(
                "This board runs {}; this Studio has {}.",
                board.long(),
                to.short()
            ),
        )
    } else {
        (
            format!("{} → {} available", board.short(), to.short()),
            format!("Update available: {} → {}.", board.short(), to.short()),
        )
    }
}

/// ` 40%`, or nothing before the first report.
fn pct(percent: Option<u8>) -> String {
    percent.map(|p| format!(" {p}%")).unwrap_or_default()
}

/// A progress sentence: `head` (ending in `…`), the percent, then `tail`.
fn running_sentence(head: &str, percent: Option<u8>, tail: &str) -> String {
    match percent {
        Some(p) => format!("{head} {p}%. {tail}"),
        None => format!("{head} {tail}"),
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::UpdateStageFacts;

    use super::super::device_update_route::UpdateLink;
    use super::super::device_update_standing::tests::{
        board_x, board_y, busy, crashing, facts_of, inputs, needs_engine, newer as board_newer,
        ready_view, refused, studio_y, updating_view,
    };
    use super::super::device_update_standing::update_standing;
    use super::*;

    fn x() -> UpdateVersion {
        UpdateVersion::with_build_id("2026.10.03-1", "2026.10.03-1+a41c9e2d11f0")
    }
    fn y() -> UpdateVersion {
        UpdateVersion::with_build_id("2026.10.05-2", "2026.10.05-2+626a1b851aaa")
    }

    /// The line, the sentence, the light and the chip, as the spike has
    /// them.
    fn says(
        standing: &UpdateStanding,
        line: &str,
        sentence: &str,
        light: Option<UpdateLight>,
        chip: &str,
    ) -> UiDeviceUpdate {
        let words = update_words(standing).expect("words");
        assert_eq!(words.line, line);
        assert_eq!(words.sentence, sentence);
        assert_eq!(words.light, light);
        assert_eq!(words.chip, chip);
        words
    }

    #[test]
    fn up_to_date() {
        let words = says(
            &UpdateStanding::UpToDate { version: y() },
            "2026.10.05-2 · up to date",
            "2026.10.05-2, the same as this Studio.",
            None,
            "Ready",
        );
        assert_eq!(words.kind, UpdateRowKind::Information);
        assert_eq!(words.version.text, "2026.10.05-2");
        assert_eq!(words.version.commit.as_deref(), Some("626a1b8"));
    }

    #[test]
    fn update_available() {
        let words = says(
            &UpdateStanding::Available {
                board: x(),
                to: y(),
            },
            "2026.10.03-1 → 2026.10.05-2 available",
            "Update available: 2026.10.03-1 → 2026.10.05-2.",
            None,
            "Ready",
        );
        assert_eq!(
            words.version.text, "2026.10.03-1",
            "the header names the board's"
        );
        // A dev board.
        says(
            &UpdateStanding::Available {
                board: UpdateVersion::new("5eb70a7c2"),
                to: y(),
            },
            "dev 5eb70a7 · Studio has 2026.10.05-2",
            "This board runs dev build 5eb70a7; this Studio has 2026.10.05-2.",
            None,
            "Ready",
        );
    }

    #[test]
    fn backing_up() {
        let words = says(
            &UpdateStanding::BackingUp {
                board: x(),
                to: y(),
                percent: Some(18),
            },
            "Backing up current firmware… 18%",
            "Backing up current firmware… 18%. The show keeps running meanwhile.",
            None,
            "Backing up",
        );
        assert_eq!(words.kind, UpdateRowKind::Progress);
        assert_eq!(
            words.progress,
            Some(UpdateProgress {
                percent: Some(18),
                other_device: false
            })
        );
    }

    #[test]
    fn updating() {
        for (link, word) in [
            (UpdateLink::Usb, "USB"),
            (UpdateLink::Bluetooth, "Bluetooth"),
        ] {
            says(
                &UpdateStanding::Updating {
                    board: x(),
                    to: y(),
                    link,
                    percent: Some(40),
                },
                &format!("Updating over {word}… 40%"),
                &format!("Updating to 2026.10.05-2 over {word}… 40%. Keep the board powered."),
                Some(UpdateLight::DarkYellow),
                "Updating",
            );
        }
    }

    #[test]
    fn finishing_an_update_this_studio_ran_is_not_called_interrupted() {
        let words = says(
            &UpdateStanding::Finishing {
                board: x(),
                to: y(),
                percent: Some(70),
                running: true,
                resumed: false,
            },
            "Finishing the update… 70%",
            "Installing the rest of the firmware… 70%. Keep the board powered.",
            Some(UpdateLight::DarkYellow),
            "Updating",
        );
        assert!(!words.sentence.contains("interrupted"));
    }

    #[test]
    fn finishing_one_this_studio_found_half_way_says_so() {
        for running in [true, false] {
            says(
                &UpdateStanding::Finishing {
                    board: x(),
                    to: y(),
                    percent: Some(70),
                    running,
                    resumed: true,
                },
                "Finishing the update… 70%",
                "Finishing the update to 2026.10.05-2… 70%. It was interrupted; this Studio is \
                 completing it.",
                Some(UpdateLight::DarkYellow),
                "Updating",
            );
        }
    }

    #[test]
    fn restoring() {
        says(
            &UpdateStanding::Restoring {
                board: x(),
                percent: Some(35),
                running: true,
            },
            "Restoring firmware… 35%",
            "Restoring firmware 2026.10.03-1… 35%. Part of it was missing; this Studio had a copy.",
            Some(UpdateLight::DarkYellow),
            "Restoring firmware",
        );
        // Before its first report (it is about to start by itself).
        says(
            &UpdateStanding::Restoring {
                board: x(),
                percent: None,
                running: false,
            },
            "Restoring firmware…",
            "Restoring firmware 2026.10.03-1… Part of it was missing; this Studio had a copy.",
            Some(UpdateLight::DarkYellow),
            "Restoring firmware",
        );
    }

    #[test]
    fn another_device() {
        let words = says(
            &UpdateStanding::AnotherDevice {
                board: x(),
                to: Some(y()),
                percent: Some(40),
            },
            "Another device is updating it… 40%",
            "Another device is updating this board to 2026.10.05-2… 40%. If it stops, this \
             Studio finishes it.",
            Some(UpdateLight::DarkYellow),
            "Updating",
        );
        assert!(words.progress.unwrap().other_device);
    }

    #[test]
    fn needs_usb_once() {
        let words = says(
            &UpdateStanding::NeedsUsbOnce {
                board: x(),
                to: y(),
                link: UpdateLink::Bluetooth,
            },
            "Needs one update over USB",
            "This board can't update over Bluetooth yet. Update it over USB once; after that it \
             updates without a cable.",
            None,
            "Ready",
        );
        assert_eq!(words.kind, UpdateRowKind::NeedsYou);
    }

    #[test]
    fn keeps_crashing() {
        says(
            &UpdateStanding::KeepsCrashing { board: y() },
            "2026.10.05-2 keeps crashing",
            "2026.10.05-2 keeps crashing on this board, so it stopped trying. Reinstall it, or \
             install another version.",
            Some(UpdateLight::DarkRed),
            "Needs firmware",
        );
    }

    #[test]
    fn a_version_studio_cant_get() {
        let v = UpdateVersion::new("2026.09.28-4");
        says(
            &UpdateStanding::CantGetVersion { board: v, own: y() },
            "Needs 2026.09.28-4, which Studio can't get",
            "This board needs 2026.09.28-4 to start, and this Studio can't get it. Connect to the \
             internet, or install 2026.10.05-2 instead.",
            Some(UpdateLight::DarkRed),
            "Needs firmware",
        );
        says(
            &UpdateStanding::CantGetVersion {
                board: UpdateVersion::new("5eb70a7c2"),
                own: y(),
            },
            "Needs dev 5eb70a7, which Studio can't get",
            "This board needs dev build 5eb70a7 to start, and this Studio can't get it. Connect \
             to the internet, or install 2026.10.05-2 instead.",
            Some(UpdateLight::DarkRed),
            "Needs firmware",
        );
    }

    #[test]
    fn rolled_back() {
        says(
            &UpdateStanding::RolledBack {
                board: x(),
                refused: y(),
            },
            "Back on 2026.10.03-1 · the update didn't start",
            "The update to 2026.10.05-2 didn't start, so the board went back to 2026.10.03-1.",
            None,
            "Ready",
        );
    }

    #[test]
    fn newer() {
        says(
            &UpdateStanding::Newer {
                board: UpdateVersion::new("2026.10.07-4"),
                own: y(),
            },
            "2026.10.07-4 · newer than this Studio",
            "2026.10.07-4 is newer than this Studio (2026.10.05-2). Reload Studio to catch up.",
            None,
            "Ready",
        );
    }

    #[test]
    fn play_only() {
        says(
            &UpdateStanding::PlayOnly {
                board: x(),
                to: y(),
            },
            "2026.10.03-1 → 2026.10.05-2 available",
            "Update available: 2026.10.03-1 → 2026.10.05-2. Installing it needs the author \
             password.",
            None,
            "Ready",
        );
    }

    #[test]
    fn nothing_has_no_words() {
        assert_eq!(update_words(&UpdateStanding::Nothing), None);
        assert_eq!(
            update_session_words(&UpdateStanding::Nothing, Some("esp32c6"), None),
            None
        );
    }

    /// Spike §3: the version, read as a version, for every way a board and
    /// this Studio can compare — the header's text, the line, and the
    /// offer's label (the offer itself is `device_update_offers`').
    #[test]
    fn the_version_reads_as_a_version_in_every_comparison() {
        let cases: [(&str, &str, &str, &str); 9] = [
            // board version, studio version, header text, line
            (
                "2026.10.05-2",
                "2026.10.05-2",
                "2026.10.05-2",
                "2026.10.05-2 · up to date",
            ),
            (
                "2026.10.03-1",
                "2026.10.05-2",
                "2026.10.03-1",
                "2026.10.03-1 → 2026.10.05-2 available",
            ),
            (
                "2026.10.07-4",
                "2026.10.05-2",
                "2026.10.07-4",
                "2026.10.07-4 · newer than this Studio",
            ),
            (
                "5eb70a7c2",
                "2026.10.05-2",
                "dev 5eb70a7",
                "dev 5eb70a7 · Studio has 2026.10.05-2",
            ),
            (
                "5eb70a7-dirty-142233PT",
                "2026.10.05-2",
                "dev 5eb70a7 (modified)",
                "dev 5eb70a7 (modified) · Studio has 2026.10.05-2",
            ),
            (
                "5eb70a7c2",
                "5eb70a7c2",
                "dev 5eb70a7",
                "dev 5eb70a7 · same as this Studio",
            ),
            (
                "5eb70a7c2",
                "626a1b851",
                "dev 5eb70a7",
                "dev 5eb70a7 · this Studio is dev 626a1b8",
            ),
            (
                "2026.10.03-1",
                "626a1b851",
                "2026.10.03-1",
                "2026.10.03-1 · this Studio is dev 626a1b8",
            ),
            (
                "2026.12.31-146",
                "2026.12.31-147",
                "2026.12.31-146",
                "2026.12.31-146 → 2026.12.31-147 available",
            ),
        ];
        let view = ready_view();
        for (board, studio, header, line) in cases {
            let same = board == studio;
            let core = if same { [0xBB; 32] } else { [0xAA; 32] };
            let engine = if same { [0xBE; 32] } else { [0xAE; 32] };
            let manifest =
                super::super::device_update_standing::tests::manifest(board, core, engine);
            let own =
                super::super::device_update_standing::tests::build(studio, [0xBB; 32], [0xBE; 32]);
            let facts = facts_of(&manifest);
            let standing = update_standing(&inputs(&view, Some(&facts), Some(&own)));
            let words = update_words(&standing).expect("words");
            assert_eq!(words.version.text, header, "{board} vs {studio}");
            assert_eq!(words.line, line, "{board} vs {studio}");
            if board.contains("-dirty-") {
                assert!(words.version.raw.contains(board), "the raw string on hover");
            }
        }
    }

    #[test]
    fn the_editor_popover_takes_the_short_words_and_the_version() {
        let session = |s: &UpdateStanding| {
            update_session_words(s, Some("esp32c6"), Some("60:55:f9:0a:0b:0c")).unwrap()
        };
        let up = session(&UpdateStanding::UpToDate { version: y() });
        assert_eq!(up.run_word, None, "the popover's own \"running\" stands");
        assert_eq!(up.stat_line, "esp32c6 · 2026.10.05-2 · 60:55:f9:0a:0b:0c");

        let available = session(&UpdateStanding::Available {
            board: x(),
            to: y(),
        });
        assert_eq!(
            available.run_word,
            Some(UpdateRunWord {
                text: "running · update available".to_string(),
                tone: UpdateRunTone::Live
            })
        );
        assert_eq!(
            available.stat_line,
            "esp32c6 · 2026.10.03-1 → 2026.10.05-2 · 60:55:f9:0a:0b:0c"
        );

        let updating = session(&UpdateStanding::Updating {
            board: x(),
            to: y(),
            link: UpdateLink::Usb,
            percent: Some(40),
        });
        assert_eq!(
            updating.run_word,
            Some(UpdateRunWord {
                text: "Updating · 40%".to_string(),
                tone: UpdateRunTone::Working
            })
        );
        assert_eq!(
            updating.stat_line,
            "esp32c6 · 2026.10.03-1 → 2026.10.05-2 · 60:55:f9:0a:0b:0c"
        );

        let crashing = session(&UpdateStanding::KeepsCrashing { board: y() });
        assert_eq!(
            crashing.run_word,
            Some(UpdateRunWord {
                text: "Needs firmware".to_string(),
                tone: UpdateRunTone::Attention
            })
        );
        assert_eq!(
            crashing.stat_line,
            "esp32c6 · 2026.10.05-2 keeps crashing · 60:55:f9:0a:0b:0c"
        );
    }

    /// The rows the standing reaches from real board facts all have words
    /// (the fixtures are the standing's own).
    #[test]
    fn every_row_the_facts_reach_has_words() {
        let view = ready_view();
        let y = studio_y();
        for manifest in [
            board_x(),
            board_y(),
            board_newer(),
            refused(),
            crashing(),
            needs_engine(),
            busy(),
        ] {
            let facts = facts_of(&manifest);
            let standing = update_standing(&inputs(&view, Some(&facts), Some(&y)));
            let words = update_words(&standing).expect("words");
            assert!(!words.line.is_empty() && !words.sentence.is_empty());
        }
        let running = updating_view(Some(UpdateStageFacts::Updating), Some(40));
        let facts = facts_of(&board_x());
        let standing = update_standing(&inputs(&running, Some(&facts), Some(&y)));
        assert_eq!(
            update_words(&standing).unwrap().line,
            "Updating over USB… 40%"
        );
    }
}
