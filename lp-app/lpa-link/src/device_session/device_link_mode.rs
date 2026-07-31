//! Which of three things is on the other end of the wire.
//!
//! # Why this cannot be answered from enumeration data
//!
//! The obvious approach — read the USB vendor/product id — **cannot work**
//! on the boards LightPlayer targets. ESP32-C6 and ESP32-S3 use the chip's
//! native USB-Serial-JTAG peripheral, and the ROM bootloader uses *that same
//! peripheral*: the device enumerates as `303A:1001` whether it is running
//! our firmware or sitting in download mode waiting to be flashed. Boards
//! behind a CP2102/CH340 bridge are worse — they always report the bridge's
//! ids, never the chip's state.
//!
//! So the mode has to be established by *talking* to the device. See
//! `docs/adr/2026-07-30-bootloader-mode-detection.md`.
//!
//! # The evidence, cheapest first
//!
//! 1. A proto-matching `ServerHello` ⇒ [`App`](DeviceLinkMode::App). This is
//!    the existing readiness gate and stays the only thing that grants
//!    readiness; this module classifies, it does not promote.
//! 2. Boot lines matching a ROM download-mode signature ⇒ corroboration for
//!    [`Bootloader`](DeviceLinkMode::Bootloader). Strong when present,
//!    meaningless when absent: a board already sitting in download mode
//!    printed its banner before anyone attached, so silence proves nothing.
//! 3. An esptool SYNC handshake that answers ⇒ `Bootloader`, authoritatively,
//!    plus the chip identity for free.
//! 4. Nothing answers ⇒ [`Unknown`](DeviceLinkMode::Unknown).
//!
//! # The probe is not free
//!
//! Step 3 resets the device (DTR/RTS), and on USB-Serial-JTAG that reset
//! drops USB enumeration and invalidates the port handle. **Probing a
//! healthy device reboots it.** That is why the probe is an explicit
//! `DeviceMode::Management` escalation rather than a step in the routine
//! connect ladder — see [`crate::device_session::device_mode`].

use super::device_readiness::{BootLineClassifier, NoFirmwareReason};

/// What is on the other end of the wire.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum DeviceLinkMode {
    /// LightPlayer firmware is running and speaking the wire protocol.
    App,
    /// A ROM or stub bootloader is listening — the device can be flashed,
    /// erased, or handed a boot-control record, but it is not running the
    /// app.
    Bootloader {
        /// Chip identity, when a SYNC probe supplied it. `None` when the
        /// classification rests on boot lines alone.
        chip_name: Option<String>,
        /// How this was established, for UX that needs to say why.
        evidence: BootloaderEvidence,
    },
    /// Neither answered: no power, a charge-only cable, the wrong port, or
    /// foreign firmware that speaks neither protocol.
    #[default]
    Unknown,
}

/// How a [`DeviceLinkMode::Bootloader`] classification was reached.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootloaderEvidence {
    /// An esptool SYNC handshake answered. Authoritative.
    SyncHandshake,
    /// Boot output carried a ROM download-mode signature. Corroborating:
    /// its presence is strong, its absence means nothing.
    BootLines,
}

impl DeviceLinkMode {
    /// Classify from boot lines alone, without probing.
    ///
    /// This is the passive read, safe to run on every connect. It can reach
    /// `App` and `Bootloader { evidence: BootLines }`, and otherwise answers
    /// `Unknown` — which means "no *passive* evidence", not "nothing is
    /// there". Escalate to a SYNC probe to tell those apart.
    pub fn from_boot_lines(classifier: &BootLineClassifier, hello_seen: bool) -> Self {
        if hello_seen {
            return Self::App;
        }
        if classifier.no_firmware_detected()
            && classifier.no_firmware_reason() == NoFirmwareReason::RomDownloadMode
        {
            return Self::Bootloader {
                chip_name: None,
                evidence: BootloaderEvidence::BootLines,
            };
        }
        Self::Unknown
    }

    /// Promote to an authoritative `Bootloader` after a SYNC probe answered.
    pub fn from_sync_probe(chip_name: Option<String>) -> Self {
        Self::Bootloader {
            chip_name,
            evidence: BootloaderEvidence::SyncHandshake,
        }
    }

    pub fn is_app(&self) -> bool {
        matches!(self, Self::App)
    }

    pub fn is_bootloader(&self) -> bool {
        matches!(self, Self::Bootloader { .. })
    }

    /// Whether a SYNC probe would add anything.
    ///
    /// False for `App` — probing a healthy device reboots it for no gain —
    /// and false once a probe has already answered.
    pub fn probe_would_help(&self) -> bool {
        match self {
            Self::App => false,
            Self::Bootloader { evidence, .. } => *evidence != BootloaderEvidence::SyncHandshake,
            Self::Unknown => true,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Bootloader { .. } => "bootloader",
            Self::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classifier_with(lines: &[&str]) -> BootLineClassifier {
        let mut classifier = BootLineClassifier::new();
        for line in lines {
            classifier.observe_line(*line);
        }
        classifier
    }

    #[test]
    fn a_hello_means_app_mode_whatever_the_boot_lines_said() {
        // Boot lines are only ever corroboration; a hello settles it.
        let classifier = classifier_with(&["ESP-ROM:esp32c6-20220919", "waiting for download"]);
        assert_eq!(
            DeviceLinkMode::from_boot_lines(&classifier, true),
            DeviceLinkMode::App
        );
    }

    #[test]
    fn download_mode_boot_lines_classify_as_bootloader() {
        let classifier = classifier_with(&["ESP-ROM:esp32c6-20220919", "waiting for download"]);
        assert_eq!(
            DeviceLinkMode::from_boot_lines(&classifier, false),
            DeviceLinkMode::Bootloader {
                chip_name: None,
                evidence: BootloaderEvidence::BootLines,
            }
        );
    }

    #[test]
    fn silence_is_unknown_not_bootloader() {
        // A board ALREADY in download mode printed its banner before anyone
        // attached. Absence of the signature must not be read as absence of
        // a bootloader.
        let classifier = classifier_with(&[]);
        assert_eq!(
            DeviceLinkMode::from_boot_lines(&classifier, false),
            DeviceLinkMode::Unknown
        );
        assert!(DeviceLinkMode::Unknown.probe_would_help());
    }

    #[test]
    fn blank_flash_is_unknown_until_a_probe_answers() {
        // Blank flash is a no-firmware signature but NOT a download-mode one:
        // the chip may be in either state, and only a probe can say.
        let classifier =
            classifier_with(&["ESP-ROM:esp32c6-20220919", "invalid header: 0xffffffff"]);
        assert!(classifier.no_firmware_detected());
        assert_eq!(
            DeviceLinkMode::from_boot_lines(&classifier, false),
            DeviceLinkMode::Unknown
        );
    }

    #[test]
    fn a_sync_probe_is_authoritative_and_carries_chip_identity() {
        let mode = DeviceLinkMode::from_sync_probe(Some("ESP32-C6".to_string()));
        assert_eq!(
            mode,
            DeviceLinkMode::Bootloader {
                chip_name: Some("ESP32-C6".to_string()),
                evidence: BootloaderEvidence::SyncHandshake,
            }
        );
        assert!(mode.is_bootloader());
        assert!(!mode.probe_would_help(), "a probe that answered is final");
    }

    #[test]
    fn probing_a_healthy_device_is_never_worth_it() {
        // The probe resets the device and drops USB enumeration. On App mode
        // that is pure cost.
        assert!(!DeviceLinkMode::App.probe_would_help());
    }

    #[test]
    fn boot_line_evidence_can_still_be_upgraded_by_a_probe() {
        let mode = DeviceLinkMode::Bootloader {
            chip_name: None,
            evidence: BootloaderEvidence::BootLines,
        };
        assert!(
            mode.probe_would_help(),
            "boot lines give no chip identity; a probe adds it"
        );
    }

    #[test]
    fn modes_have_stable_log_names() {
        assert_eq!(DeviceLinkMode::App.as_str(), "app");
        assert_eq!(DeviceLinkMode::Unknown.as_str(), "unknown");
        assert_eq!(DeviceLinkMode::from_sync_probe(None).as_str(), "bootloader");
    }

    #[test]
    fn unknown_is_the_default() {
        assert_eq!(DeviceLinkMode::default(), DeviceLinkMode::Unknown);
    }
}
