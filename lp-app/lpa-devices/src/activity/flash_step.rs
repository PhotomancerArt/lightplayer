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
        }
    }

    /// The step a write's progress names. The executors (esptool-js in the
    /// browser, espflash on a host) label their progress by what they are
    /// writing ("Writing firmware", "Moving files", "Verifying files",
    /// "Resetting the board"); `carries` says whether this write moves the
    /// board's files, which is what makes the closing reset part of
    /// checking them rather than of the firmware write.
    pub fn of_write(progress: Option<&str>, carries: bool) -> Self {
        let progress = progress.unwrap_or_default();
        if progress.starts_with("Moving files") {
            Self::MovingFiles
        } else if progress.starts_with("Verifying files")
            || (carries && progress.starts_with("Resetting the board"))
        {
            Self::CheckingFiles
        } else {
            Self::FlashingFirmware
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            FlashStep::FlashingFirmware,
            "a plain flash's reset is the firmware write's end"
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
        ] {
            let label = step.label();
            assert!(label.ends_with('…'), "{label}");
            for jargon in ["lpfs", "superblock", "partition", "inspect"] {
                assert!(!label.to_lowercase().contains(jargon), "{label}");
            }
        }
    }
}
