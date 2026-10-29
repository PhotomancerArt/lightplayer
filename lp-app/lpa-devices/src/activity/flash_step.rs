//! [`FlashStep`]: which part of a Flash the card names.
//!
//! A Flash is more than a firmware write since the C6 repartition: it reads
//! the board first (its partition table, and its files when they must move),
//! may wait for the user's answer, writes the firmware, moves the files and
//! checks them. The card used to say "Flashing firmware…" through all of
//! it — through the read, through the question and through the file move
//! (G1 walk, 2026-10-03, Yona: "the 'Flashing firmware…' label isn't really
//! right for the first phase"). Each step now has its own words, and the
//! card's label follows the step.
//!
//! The two waits after the write have their own words too: the board
//! restarting on its new firmware, and the board model being saved on it.
//! Yona's phone install (2026-10-25) sat on a bare spinner through both,
//! after a stale 100 % from the layout read and a percent that flickered
//! away on every esptool log line
//! (`docs/defects/2026-10-25-install-progress-jumped-to-100-and-vanished.md`).

use serde::{Deserialize, Serialize};

/// The step of a Flash the card's label names.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum FlashStep {
    /// Reading the board's layout (and its files, when they must move) in
    /// its bootloader, before anything is written.
    ReadingBoard,
    /// The question is up: the board's files move only on the user's yes.
    WaitingForAnswer,
    /// The firmware image is being written — and, on a plain flash, the
    /// board is coming back on it.
    FlashingFirmware,
    /// The board's files are being written at the new layout.
    MovingFiles,
    /// The files are being read back, and then the board's own boot is
    /// proving they mounted.
    CheckingFiles,
    /// The firmware is written and the board is restarting on it: the
    /// flasher's closing reset, then the wait for the new firmware's hello.
    Restarting,
    /// The new firmware answered; the board model picked for it is being
    /// saved on the board (its `/hardware.json`).
    SavingBoardModel,
}

impl FlashStep {
    /// The card's label for this step, present tense.
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadingBoard => "Reading the board…",
            Self::WaitingForAnswer => "Waiting for your answer…",
            Self::FlashingFirmware => "Flashing firmware…",
            Self::MovingFiles => "Moving files…",
            Self::CheckingFiles => "Checking the files…",
            Self::Restarting => "Restarting the board…",
            Self::SavingBoardModel => "Saving the board model…",
        }
    }

    /// Whether the card shows a percent at this step, given the label of
    /// the coarse effect's latest progress. A step with real work to count
    /// shows its own; the question, the restart and the save have nothing
    /// to count. Reading the board shows one only for its files: the layout
    /// read is a few kilobytes, so its percent is 100 almost at once and
    /// would read as the install being done before anything was written.
    pub fn shows_percent(self, progress: Option<&str>) -> bool {
        match self {
            Self::ReadingBoard => {
                progress.is_some_and(|label| label.starts_with("Reading the board's files"))
            }
            Self::FlashingFirmware | Self::MovingFiles | Self::CheckingFiles => true,
            Self::WaitingForAnswer | Self::Restarting | Self::SavingBoardModel => false,
        }
    }

    /// The step a write's progress names. The executors (esptool-js in the
    /// browser, espflash on a host) label their progress by what they are
    /// writing ("Writing firmware", "Moving files", "Verifying files",
    /// "Resetting the board"); `carries` says whether this write moves the
    /// board's files, which is what makes the closing reset part of
    /// checking them rather than the plain restart.
    pub fn of_write(progress: Option<&str>, carries: bool) -> Self {
        let progress = progress.unwrap_or_default();
        if progress.starts_with("Moving files") {
            Self::MovingFiles
        } else if progress.starts_with("Verifying files")
            || (carries && progress.starts_with("Resetting the board"))
        {
            Self::CheckingFiles
        } else if progress.starts_with("Resetting the board") {
            Self::Restarting
        } else {
            Self::FlashingFirmware
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_read_shows_no_percent_and_the_files_read_does() {
        let reading = FlashStep::ReadingBoard;
        assert!(!reading.shows_percent(Some("Reading the board's layout")));
        assert!(!reading.shows_percent(None));
        assert!(reading.shows_percent(Some("Reading the board's files")));
        assert!(FlashStep::FlashingFirmware.shows_percent(Some("Writing firmware")));
        assert!(!FlashStep::Restarting.shows_percent(Some("Resetting the board")));
        assert!(!FlashStep::SavingBoardModel.shows_percent(None));
        assert!(!FlashStep::WaitingForAnswer.shows_percent(None));
    }

    #[test]
    fn a_write_names_its_step_from_its_progress() {
        let of = FlashStep::of_write;
        assert_eq!(
            of(Some("Writing firmware"), true),
            FlashStep::FlashingFirmware
        );
        assert_eq!(
            of(Some("Writing firmware image 1/3"), false),
            FlashStep::FlashingFirmware
        );
        assert_eq!(of(Some("Moving files"), true), FlashStep::MovingFiles);
        assert_eq!(of(Some("Verifying files"), true), FlashStep::CheckingFiles);
        assert_eq!(
            of(Some("Resetting the board"), true),
            FlashStep::CheckingFiles
        );
        assert_eq!(
            of(Some("Resetting the board"), false),
            FlashStep::Restarting,
            "a plain flash's reset is the board restarting"
        );
        assert_eq!(of(None, true), FlashStep::FlashingFirmware);
    }

    #[test]
    fn every_step_reads_as_plain_words() {
        for step in [
            FlashStep::ReadingBoard,
            FlashStep::WaitingForAnswer,
            FlashStep::FlashingFirmware,
            FlashStep::MovingFiles,
            FlashStep::CheckingFiles,
            FlashStep::Restarting,
            FlashStep::SavingBoardModel,
        ] {
            let label = step.label();
            assert!(label.ends_with('…'), "{label}");
            for jargon in [
                "lpfs",
                "superblock",
                "partition",
                "inspect",
                "hardware.json",
                "manifest",
            ] {
                assert!(!label.to_lowercase().contains(jargon), "{label}");
            }
        }
    }
}
