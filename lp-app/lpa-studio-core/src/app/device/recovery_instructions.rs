//! How to get a particular board into bootloader mode — and how to admit
//! when we do not know.
//!
//! # Why this is keyed on the chip, not the board
//!
//! The obvious source for board-specific instructions is the board manifest.
//! It does not work here. `HwManifest` is **device-side**: it is read from
//! the device's own `/hardware.json`, or compiled into its firmware. A device
//! that will not boot cannot tell Studio what board it is — and that is
//! exactly the situation these instructions exist for.
//!
//! So instructions key on the **chip family**, which Studio can honestly know
//! two ways: the `ServerHello` from a device it reached earlier, or the SYNC
//! probe's `chip_name` (see `lpa_link::DeviceLinkMode`).
//!
//! When it knows neither — the common case, since the user has not got into
//! bootloader mode yet — it renders **generic** guidance and says so.
//! Asserting "hold BOOT" on a board that may have no BOOT button is worse
//! than admitting the uncertainty: a user who follows a confident wrong
//! instruction concludes their device is dead.

/// One step of a bootloader-entry sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryStep {
    pub text: String,
}

impl RecoveryStep {
    fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// A bootloader-entry sequence, and how much Studio actually knows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryInstructions {
    /// What to call the target in the heading — the chip when known,
    /// otherwise a generic noun.
    pub subject: String,
    pub steps: Vec<RecoveryStep>,
    /// True when these are generic ESP32 steps rather than steps for a chip
    /// Studio has actually identified. The UI must say so: an unhedged
    /// instruction that does not match the user's board reads as "your
    /// device is broken".
    pub is_generic: bool,
}

impl RecoveryInstructions {
    /// Instructions for whatever Studio knows.
    ///
    /// `chip_name` is the chip family as reported by a hello or a SYNC probe
    /// — `None` when Studio has never reached this device, which is the
    /// normal state when someone is trying to *get into* bootloader mode.
    pub fn for_chip(chip_name: Option<&str>) -> Self {
        match ChipFamily::parse(chip_name) {
            Some(family) => Self {
                subject: family.display_name().to_string(),
                steps: family.steps(),
                is_generic: false,
            },
            None => Self {
                subject: "this device".to_string(),
                steps: ChipFamily::generic_steps(),
                is_generic: true,
            },
        }
    }
}

/// The chip families Studio has real instructions for.
///
/// Deliberately coarse. The entry sequence is a property of the chip's boot
/// straps, not of the board, so a family is the right granularity — and it is
/// the only granularity a probe can actually report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChipFamily {
    Esp32C6,
    Esp32S3,
}

impl ChipFamily {
    /// Parse a chip name from a hello or an esptool probe.
    ///
    /// Probe output is free-form ("ESP32-C6 (QFN32) (revision v0.2)"), so
    /// this matches on a substring rather than expecting an exact token.
    fn parse(chip_name: Option<&str>) -> Option<Self> {
        let name = chip_name?.to_ascii_lowercase();
        // Check S3 before C6: neither is a prefix of the other, but keeping
        // the checks explicit avoids a future family accidentally matching
        // two arms.
        if name.contains("esp32-s3") || name.contains("esp32s3") {
            Some(Self::Esp32S3)
        } else if name.contains("esp32-c6") || name.contains("esp32c6") {
            Some(Self::Esp32C6)
        } else {
            None
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::Esp32C6 => "ESP32-C6",
            Self::Esp32S3 => "ESP32-S3",
        }
    }

    fn steps(self) -> Vec<RecoveryStep> {
        // Both families use the same USB-Serial-JTAG strap sequence; they are
        // separate variants so the heading can name the chip and so a family
        // that diverges later has somewhere to go.
        // Knowing the CHIP does not tell us the silkscreen: the button label
        // is a board property. Boards routinely shorten it to "B" for space,
        // so even the chip-specific path hedges the label — it just does not
        // have to hedge the sequence.
        vec![
            RecoveryStep::new("Unplug the USB cable."),
            RecoveryStep::new("Hold the BOOT button — often labelled just \"B\". Keep holding it."),
            RecoveryStep::new("Plug the cable back in while still holding it."),
            RecoveryStep::new("Let go after a second or two."),
        ]
    }

    fn generic_steps() -> Vec<RecoveryStep> {
        vec![
            RecoveryStep::new("Unplug the USB cable."),
            RecoveryStep::new(
                "Hold the BOOT button — many boards shorten the label to just \
                 \"B\", and some use IO0 or FLASH. Keep holding it.",
            ),
            RecoveryStep::new("Plug the cable back in while still holding that button."),
            RecoveryStep::new("Let go after a second or two."),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_chip_gets_named_non_generic_instructions() {
        let instructions = RecoveryInstructions::for_chip(Some("ESP32-C6"));
        assert_eq!(instructions.subject, "ESP32-C6");
        assert!(!instructions.is_generic);
        assert!(!instructions.steps.is_empty());
    }

    #[test]
    fn probe_output_is_matched_as_a_substring() {
        // esptool reports "ESP32-C6FH4 (QFN32) (revision v0.2)"; an exact
        // match would fall through to generic for every real device.
        let instructions =
            RecoveryInstructions::for_chip(Some("ESP32-C6FH4 (QFN32) (revision v0.2)"));
        assert_eq!(instructions.subject, "ESP32-C6");
        assert!(!instructions.is_generic);
    }

    #[test]
    fn a_firmware_package_name_identifies_the_chip() {
        // `UiDeviceCard.fw.package` is "fw-esp32c6" — the package names the
        // chip it was built for, so a device Studio reached earlier can get
        // specific instructions even though it is unreachable NOW. This is
        // the only chip source available before the user gets into bootloader
        // mode.
        let instructions = RecoveryInstructions::for_chip(Some("fw-esp32c6"));
        assert_eq!(instructions.subject, "ESP32-C6");
        assert!(!instructions.is_generic);

        let instructions = RecoveryInstructions::for_chip(Some("fw-esp32s3"));
        assert_eq!(instructions.subject, "ESP32-S3");
    }

    #[test]
    fn chip_matching_is_case_insensitive() {
        assert_eq!(
            RecoveryInstructions::for_chip(Some("esp32s3")).subject,
            "ESP32-S3"
        );
        assert_eq!(
            RecoveryInstructions::for_chip(Some("ESP32-S3")).subject,
            "ESP32-S3"
        );
    }

    #[test]
    fn an_unknown_device_gets_generic_instructions_and_admits_it() {
        // The normal state before the user reaches bootloader mode: Studio
        // has never talked to this device.
        let instructions = RecoveryInstructions::for_chip(None);
        assert!(instructions.is_generic);
        assert_eq!(instructions.subject, "this device");
        assert!(!instructions.steps.is_empty());
    }

    #[test]
    fn an_unrecognized_chip_falls_back_to_generic_rather_than_guessing() {
        let instructions = RecoveryInstructions::for_chip(Some("ESP32-H2"));
        assert!(
            instructions.is_generic,
            "an unknown chip must not inherit another family's button sequence"
        );
    }

    #[test]
    fn generic_steps_hedge_the_button_name() {
        // A board whose button is labelled IO0 is not "broken" — the copy has
        // to allow for it, or the user concludes it is.
        let instructions = RecoveryInstructions::for_chip(None);
        let joined = instructions
            .steps
            .iter()
            .map(|step| step.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        // "B" is the one that actually bites: boards routinely shorten the
        // silkscreen for space, and a user hunting for a button labelled
        // "BOOT" on a board that says "B" concludes they have the wrong
        // board. Reported from a real bench, 2026-07-31.
        assert!(
            joined.contains("\"B\"") && joined.contains("IO0") && joined.contains("FLASH"),
            "generic copy must cover the abbreviated silkscreens: {joined}"
        );
    }

    #[test]
    fn no_sequence_asserts_an_unhedged_button_label() {
        // Knowing the chip does not tell us the silkscreen — that is a board
        // property, and boards shorten it to "B". Every sequence, specific or
        // generic, must allow for that.
        for chip in [Some("ESP32-C6"), Some("ESP32-S3"), None] {
            let joined = RecoveryInstructions::for_chip(chip)
                .steps
                .iter()
                .map(|step| step.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            assert!(
                joined.contains("\"B\""),
                "{chip:?} must allow for the abbreviated label: {joined}"
            );
        }
    }

    #[test]
    fn every_sequence_starts_by_unplugging() {
        // The strap is sampled at reset, so holding BOOT on an already-running
        // board does nothing. Getting this order wrong is the single most
        // common reason the ritual fails.
        for chip in [Some("ESP32-C6"), Some("ESP32-S3"), None] {
            let instructions = RecoveryInstructions::for_chip(chip);
            assert!(
                instructions.steps[0].text.to_lowercase().contains("unplug"),
                "{chip:?} sequence must start by unplugging"
            );
        }
    }
}
