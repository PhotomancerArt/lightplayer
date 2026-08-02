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

    /// The single boot decision, with the safe-mode precedence applied.
    ///
    /// This is THE place the skip-vs-clamp precedence lives — consumers ask
    /// for the action rather than re-deriving it from raw flags, so the rule
    /// ("a clamp-capable firmware loads the project dimmed and ignores the
    /// skip") cannot drift between implementations.
    pub const fn boot_action(self) -> BootAction {
        if let Some(level) = self.flags.safe_clamp() {
            return BootAction::LoadClamped { level };
        }
        if self.skip_project_autoload() {
            return BootAction::SkipAutoload;
        }
        BootAction::Normal
    }

    /// Encode to the on-flash record. Write it in ONE operation — see
    /// [`encode_record`](crate::encode_record) for why splitting the write
    /// destroys it.
    pub fn encode(self) -> [u8; RECORD_LEN] {
        encode_record(self.flags)
    }
}

/// What this boot should actually do, precedence already applied.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BootAction {
    /// Boot normally.
    Normal,
    /// Come up reachable with nothing loaded. The pre-clamp degradation,
    /// and the fallback for firmware that predates the clamp.
    SkipAutoload,
    /// Load the project, but ceiling every fixture's output at
    /// `level`/255. The preferred safe mode: the user sees their work
    /// running dim, connects, and fixes it.
    LoadClamped { level: u8 },
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
    /// Prefer [`Self::boot_action`], which applies the safe-mode precedence.
    /// Every failure mode answers `false`, so a corrupt sector can never
    /// *cause* a degraded boot — it can only fail to prevent one.
    pub fn skip_project_autoload(self) -> bool {
        self.control()
            .is_some_and(BootControl::skip_project_autoload)
    }

    /// The boot decision, with the safe-mode precedence applied. Every
    /// failure mode answers [`BootAction::Normal`].
    pub fn boot_action(self) -> BootAction {
        match self.control() {
            Some(control) => control.boot_action(),
            None => BootAction::Normal,
        }
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
    fn the_precedence_rule_lives_here_and_clamp_wins() {
        use crate::BootFlags;
        // Studio writes skip + clamp together; a clamp-capable firmware
        // must load-dimmed, not skip. This is the format-level rule the ADR
        // promises, so it is asserted at the format level.
        let both = BootControl::new(BootFlags::SKIP_PROJECT_AUTOLOAD.with_safe_clamp(26));
        assert_eq!(both.boot_action(), BootAction::LoadClamped { level: 26 });

        let skip_only = BootControl::new(BootFlags::SKIP_PROJECT_AUTOLOAD);
        assert_eq!(skip_only.boot_action(), BootAction::SkipAutoload);

        let clamp_only = BootControl::new(BootFlags::NONE.with_safe_clamp(128));
        assert_eq!(
            clamp_only.boot_action(),
            BootAction::LoadClamped { level: 128 }
        );

        assert_eq!(BootControl::NONE.boot_action(), BootAction::Normal);
    }

    #[test]
    fn every_failure_mode_boots_normally_via_boot_action() {
        for outcome in [
            DecodeOutcome::Blank,
            DecodeOutcome::Invalid,
            DecodeOutcome::CrcMismatch,
            DecodeOutcome::UnsupportedVersion { found: 9 },
        ] {
            assert_eq!(outcome.boot_action(), BootAction::Normal);
        }
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
