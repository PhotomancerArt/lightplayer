//! What a boot-control record can ask the next boot to do.

/// Bitfield of boot-time instructions.
///
/// # Layout of the underlying `u32`
///
/// | Bits | Meaning |
/// |---|---|
/// | `0..8` | Boolean instructions. Bit 0 is [`Self::SKIP_PROJECT_AUTOLOAD`]. |
/// | `8..16` | **Reserved** for a graduated output clamp level (follow-up plan). |
/// | `16..32` | **Reserved.** |
///
/// The reserved ranges exist so the follow-up safe-clamp work can add a
/// clamp level without a format version bump. Unknown bits in a record whose
/// version this build understands are **ignored, not rejected** — a newer
/// host asking for a clamp this firmware cannot apply should still get the
/// skip it also asked for, rather than falling back to a normal boot.
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

    /// Bits this build assigns meaning to. Everything else is reserved.
    const KNOWN: u32 = Self::SKIP_PROJECT_AUTOLOAD.0;

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
    fn reserved_bits_are_flagged_but_do_not_hide_known_ones() {
        // A newer host asking for a clamp level (reserved byte) plus a skip.
        let flags = BootFlags::from_bits(BootFlags::SKIP_PROJECT_AUTOLOAD.bits() | (0x7 << 8));
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
