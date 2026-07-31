//! What a boot-control record can ask the next boot to do.

/// Bitfield of boot-time instructions.
///
/// # Layout of the underlying `u32`
///
/// | Bits | Meaning |
/// |---|---|
/// | `0..8` | Boolean instructions. Bit 0 is [`Self::SKIP_PROJECT_AUTOLOAD`]. |
/// | `8..16` | Safe-mode output clamp level: `0` = none, else a brightness ceiling out of 255. |
/// | `16..32` | **Reserved.** |
///
/// # Safe-mode precedence
///
/// A record may carry BOTH the skip bit and a clamp level — that is what
/// Studio's "Start in safe mode" writes. **A firmware that implements the
/// clamp loads the project dimmed and IGNORES the skip bit**: seeing the
/// user's work at low current is a strictly better degradation than a dark
/// board. A firmware that predates the clamp ignores the unknown bits and
/// honors the skip. Same record, best available behavior on each — that is
/// why the host sets both rather than choosing per firmware version.
///
/// Unknown bits in a record whose version this build understands are
/// **ignored, not rejected** — a newer host asking for a clamp this
/// firmware cannot apply still gets the skip it also asked for, rather
/// than falling back to a normal boot.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct BootFlags(u32);

impl BootFlags {
    /// No instructions: equivalent to having no record at all.
    pub const NONE: Self = Self(0);

    /// Skip the project auto-load for this boot.
    ///
    /// The device comes up reachable with nothing loaded, so a project that
    /// kills the board on load can be replaced or deleted over the link.
    pub const SKIP_PROJECT_AUTOLOAD: Self = Self(1 << 0);

    /// Default safe-mode clamp: ~10% brightness. Bright enough to see the
    /// project running, far below brownout territory.
    pub const DEFAULT_SAFE_CLAMP: u8 = 26;

    const CLAMP_SHIFT: u32 = 8;
    const CLAMP_MASK: u32 = 0xFF << Self::CLAMP_SHIFT;

    /// Bits this build assigns meaning to. Everything else is reserved.
    const KNOWN: u32 = Self::SKIP_PROJECT_AUTOLOAD.0 | Self::CLAMP_MASK;

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every bit in `other` is set here.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Attach a safe-mode output clamp level (`0` clears it).
    pub const fn with_safe_clamp(self, level: u8) -> Self {
        Self((self.0 & !Self::CLAMP_MASK) | ((level as u32) << Self::CLAMP_SHIFT))
    }

    /// The safe-mode clamp level, when one is set: a brightness ceiling out
    /// of 255. See the type docs for the precedence rule against
    /// [`Self::SKIP_PROJECT_AUTOLOAD`].
    pub const fn safe_clamp(self) -> Option<u8> {
        let level = ((self.0 & Self::CLAMP_MASK) >> Self::CLAMP_SHIFT) as u8;
        if level == 0 { None } else { Some(level) }
    }

    /// Whether the record carries instructions this build does not
    /// understand. Diagnostic only — unknown bits are never a decode
    /// failure.
    pub const fn has_unknown_bits(self) -> bool {
        self.0 & !Self::KNOWN != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_empty_and_contains_nothing() {
        assert!(BootFlags::NONE.is_empty());
        assert!(!BootFlags::NONE.contains(BootFlags::SKIP_PROJECT_AUTOLOAD));
    }

    #[test]
    fn skip_autoload_round_trips_through_bits() {
        let flags = BootFlags::SKIP_PROJECT_AUTOLOAD;
        assert_eq!(BootFlags::from_bits(flags.bits()), flags);
        assert!(flags.contains(BootFlags::SKIP_PROJECT_AUTOLOAD));
        assert!(!flags.is_empty());
    }

    #[test]
    fn union_accumulates() {
        let flags = BootFlags::NONE.union(BootFlags::SKIP_PROJECT_AUTOLOAD);
        assert!(flags.contains(BootFlags::SKIP_PROJECT_AUTOLOAD));
    }

    #[test]
    fn the_safe_clamp_rides_bits_8_to_15() {
        let flags = BootFlags::SKIP_PROJECT_AUTOLOAD.with_safe_clamp(26);
        assert_eq!(flags.safe_clamp(), Some(26));
        assert!(flags.contains(BootFlags::SKIP_PROJECT_AUTOLOAD));
        // Round-trips through raw bits (the wire carries a plain u32).
        assert_eq!(BootFlags::from_bits(flags.bits()).safe_clamp(), Some(26));
        // Clamp bits are KNOWN now — a clamp-carrying record is not flagged
        // as from-the-future.
        assert!(!flags.has_unknown_bits());
    }

    #[test]
    fn a_zero_clamp_means_none_and_clears() {
        assert_eq!(BootFlags::NONE.safe_clamp(), None);
        let cleared = BootFlags::SKIP_PROJECT_AUTOLOAD
            .with_safe_clamp(26)
            .with_safe_clamp(0);
        assert_eq!(cleared.safe_clamp(), None);
        assert!(cleared.contains(BootFlags::SKIP_PROJECT_AUTOLOAD));
    }

    #[test]
    fn reserved_bits_are_flagged_but_do_not_hide_known_ones() {
        // A newer host setting a bit in the still-reserved 16..32 range.
        let flags = BootFlags::from_bits(BootFlags::SKIP_PROJECT_AUTOLOAD.bits() | (0x1 << 20));
        assert!(flags.has_unknown_bits());
        assert!(
            flags.contains(BootFlags::SKIP_PROJECT_AUTOLOAD),
            "unknown bits must not suppress instructions this build understands"
        );
    }

    #[test]
    fn known_bits_alone_are_not_flagged_as_unknown() {
        assert!(!BootFlags::SKIP_PROJECT_AUTOLOAD.has_unknown_bits());
        assert!(!BootFlags::NONE.has_unknown_bits());
    }
}
