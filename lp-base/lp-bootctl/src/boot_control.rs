//! The decoded record, and every way decoding can end.

use crate::boot_flags::BootFlags;
use crate::sector::{RECORD_LEN, decode_record, encode_record};

/// A valid boot-control record: what the next boot was asked to do.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct BootControl {
    flags: BootFlags,
}

impl BootControl {
    /// A record carrying no instructions.
    pub const NONE: Self = Self {
        flags: BootFlags::NONE,
    };

    pub const fn new(flags: BootFlags) -> Self {
        Self { flags }
    }

    pub const fn flags(self) -> BootFlags {
        self.flags
    }

    /// Whether this boot should skip the project auto-load.
    pub const fn skip_project_autoload(self) -> bool {
        self.flags.contains(BootFlags::SKIP_PROJECT_AUTOLOAD)
    }

    /// Encode to the on-flash record.
    ///
    /// Writers that can be interrupted must use
    /// [`encode_write_order`](crate::encode_write_order) instead, which
    /// exposes the ordering that makes a torn write safe.
    pub fn encode(self) -> [u8; RECORD_LEN] {
        encode_record(self.flags)
    }
}

/// Every way a read of the boot-control sector can resolve.
///
/// Only [`Valid`](Self::Valid) can change how the device boots. Every other
/// variant — including all four failure modes — means "boot normally"; they
/// are distinguished so the firmware log can say *why*, not so callers can
/// treat them differently.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum DecodeOutcome {
    /// A record this build understands.
    Valid(BootControl),
    /// Erased flash: no record was ever written here.
    Blank,
    /// Not erased, but not ours either — foreign data or a torn write that
    /// lost the magic.
    Invalid,
    /// Our magic, but the payload does not match its checksum.
    CrcMismatch,
    /// Our magic and a good checksum, from a format this build predates.
    UnsupportedVersion { found: u16 },
}

impl DecodeOutcome {
    /// The record, if there is a usable one.
    pub fn control(self) -> Option<BootControl> {
        match self {
            Self::Valid(control) => Some(control),
            _ => None,
        }
    }

    /// Whether this boot should skip the project auto-load.
    ///
    /// The single question the boot path asks. Every failure mode answers
    /// `false`, so a corrupt sector can never *cause* a degraded boot — it
    /// can only fail to prevent one.
    pub fn skip_project_autoload(self) -> bool {
        self.control()
            .is_some_and(BootControl::skip_project_autoload)
    }

    /// A short, stable reason for the firmware boot log.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Valid(_) => "valid",
            Self::Blank => "blank",
            Self::Invalid => "invalid",
            Self::CrcMismatch => "crc-mismatch",
            Self::UnsupportedVersion { .. } => "unsupported-version",
        }
    }
}

/// Decode a boot-control sector read from flash.
///
/// `bytes` may be the whole 4 KB sector or just its head; only the first
/// [`RECORD_LEN`] bytes are examined. A short slice decodes to
/// [`DecodeOutcome::Invalid`].
pub fn decode(bytes: &[u8]) -> DecodeOutcome {
    decode_record(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_instructs_nothing() {
        assert!(!BootControl::NONE.skip_project_autoload());
        assert!(BootControl::NONE.flags().is_empty());
    }

    #[test]
    fn encode_decode_round_trips() {
        let control = BootControl::new(BootFlags::SKIP_PROJECT_AUTOLOAD);
        let decoded = decode(&control.encode());
        assert_eq!(decoded.control(), Some(control));
        assert!(decoded.skip_project_autoload());
    }

    #[test]
    fn every_failure_mode_declines_to_skip_autoload() {
        for outcome in [
            DecodeOutcome::Blank,
            DecodeOutcome::Invalid,
            DecodeOutcome::CrcMismatch,
            DecodeOutcome::UnsupportedVersion { found: 99 },
        ] {
            assert!(
                !outcome.skip_project_autoload(),
                "{outcome:?} must fall back to a normal boot"
            );
            assert_eq!(outcome.control(), None);
        }
    }

    #[test]
    fn a_valid_but_empty_record_declines_to_skip_autoload() {
        let outcome = DecodeOutcome::Valid(BootControl::NONE);
        assert!(!outcome.skip_project_autoload());
        assert!(outcome.control().is_some());
    }

    #[test]
    fn reasons_are_distinguishable_in_logs() {
        assert_eq!(DecodeOutcome::Blank.as_str(), "blank");
        assert_eq!(DecodeOutcome::Invalid.as_str(), "invalid");
        assert_eq!(DecodeOutcome::CrcMismatch.as_str(), "crc-mismatch");
        assert_eq!(
            DecodeOutcome::UnsupportedVersion { found: 2 }.as_str(),
            "unsupported-version"
        );
        assert_eq!(DecodeOutcome::Valid(BootControl::NONE).as_str(), "valid");
    }
}
