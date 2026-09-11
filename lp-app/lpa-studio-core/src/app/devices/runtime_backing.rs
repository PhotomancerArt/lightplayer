//! What actually runs a target: the one-row capability table (PD16, A2).
//!
//! A **backing** is how this build can run a piece of hardware that is not
//! on the desk:
//!
//! - **sim** — the desktop firmware wearing the target's hardware manifest.
//!   Fast, and exact about the hardware's *shape* (its endpoints, its
//!   limits) rather than about its silicon.
//! - **emu** — the target's own firmware image on an emulated SoC, in this
//!   tab. Exact, and slower.
//!
//! It is a table rather than a `match` on the board id because the answer
//! is a property of what Studio SHIPS, not of the board. **The day has
//! come:** `lp-emu-esp32c6` is wired in behind a device (mode A), the XIAO
//! ESP32-C6 row says `Emu`, and every surface that asks — the picker's row
//! tag, the hint line, the runtime band's kind word — followed from that
//! one edit without a second decision anywhere.
//!
//! # Two questions, not one (D21)
//!
//! [`backing_for`] answers "what would this build run this board as", which
//! is what a tag on a row means. [`emu_offered_for`] answers the narrower
//! "may a person actually pick an emu of this board here", and that is a
//! **join**, not a table lookup: an emu is born flashed (D22), so it needs
//! a served firmware build as well as a place in the table. It is the same
//! join `flash_offer` makes (`device_flash.rs`), reused rather than
//! re-decided.
//!
//! The third half of D21 — whether this build ships an emulator *module* —
//! is deliberately NOT asked here. The module is a hashed sidecar the page
//! resolves at power-on, so only the page can answer honestly, and it does:
//! `BrowserEmuLinkSource` fails the link with the reason when the manifest
//! has no emulator entry. A table in the core that guessed would be a
//! second source of truth for a fact it cannot see.

/// How this build runs a target that is not silicon on the desk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backing {
    /// The target's own firmware on an emulated SoC.
    Emu,
    /// The desktop firmware wearing the target's hardware manifest.
    Sim,
}

impl Backing {
    /// The lowercase word a picker row is tagged with (spike 2b: a WORD,
    /// not a chip and not a sentence).
    pub fn tag(self) -> &'static str {
        match self {
            Self::Emu => "emu",
            Self::Sim => "sim",
        }
    }
}

/// The two enums are the same distinction seen from two sides: [`Backing`]
/// is what this build OFFERS, [`RuntimeKind`] is what a record IS. A record
/// always came from an offer, so this direction is total and lossless; the
/// other direction is not, and is deliberately absent.
///
/// [`RuntimeKind`]: super::sim_record::RuntimeKind
impl From<super::sim_record::RuntimeKind> for Backing {
    fn from(kind: super::sim_record::RuntimeKind) -> Self {
        match kind {
            super::sim_record::RuntimeKind::Emu => Self::Emu,
            super::sim_record::RuntimeKind::Sim => Self::Sim,
        }
    }
}

/// The targets this build ships an emulator for.
///
/// One id per emulated SoC module. Adding a row is the whole of "Studio can
/// emulate this board": the picker's tags, the hint line and the band's
/// kind word all follow from it.
pub const EMULATED_TARGETS: &[&str] = &["seeed/xiao-esp32-c6"];

/// The backing this build has for `board_id`.
///
/// One row per target. The argument is the board id (Desktop included — it
/// is a board file like any other) so callers never have to hold a
/// [`ProjectTarget`] to ask.
///
/// This is "what this build ships", not "what a person may pick right
/// now" — see [`emu_offered_for`] for the offer.
///
/// [`ProjectTarget`]: crate::app::library::ProjectTarget
pub fn backing_for(board_id: &str) -> Backing {
    match EMULATED_TARGETS.contains(&board_id) {
        true => Backing::Emu,
        false => Backing::Sim,
    }
}

/// Whether an **emu row** may be offered for `board_id` (D21).
///
/// Two conditions, both necessary:
///
/// 1. the table says this build has an emulator for the board, and
/// 2. a served firmware build resolves for it — the same
///    `provisioning_build_id` join `flash_offer` makes.
///
/// The second is what "born flashed" needs (D22): an emu with no served
/// build comes up on a blank chip, which is a legible state for a board
/// already in the roster but a poor thing to offer as a new device.
pub fn emu_offered_for(board_id: &str) -> bool {
    if !EMULATED_TARGETS.contains(&board_id) {
        return false;
    }
    let Some(board) = lpa_boards::board_by_id(board_id) else {
        return false;
    };
    lpa_boards::provisioning_build_id(Some(board), Some(board.family.as_str())).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table answers for every target the picker can offer, Desktop
    /// included — an unanswered row would render a tagless option.
    #[test]
    fn every_target_has_a_backing() {
        for board_id in crate::app::devices::target_offer::every_target() {
            let expected = match board_id {
                "seeed/xiao-esp32-c6" => Backing::Emu,
                _ => Backing::Sim,
            };
            assert_eq!(backing_for(board_id), expected, "{board_id}");
        }
    }

    /// The tags are the words the rows wear, lowercase and single.
    #[test]
    fn the_tag_is_one_lowercase_word() {
        assert_eq!(Backing::Sim.tag(), "sim");
        assert_eq!(Backing::Emu.tag(), "emu");
        for tag in [Backing::Sim.tag(), Backing::Emu.tag()] {
            assert!(!tag.contains(' '), "{tag} is a word, not a sentence");
            assert_eq!(tag, tag.to_lowercase());
        }
    }

    /// A2's inverse: the C6 IS emulated in this build, so the picker's
    /// emu/sim hint has something to explain and the emu row exists.
    #[test]
    fn the_c6_is_emulated_in_this_build() {
        assert_eq!(EMULATED_TARGETS, &["seeed/xiao-esp32-c6"]);
        assert_eq!(backing_for("seeed/xiao-esp32-c6"), Backing::Emu);
        assert!(emu_offered_for("seeed/xiao-esp32-c6"));
    }

    /// D21's join: the table alone is not the offer. A board that is not in
    /// the table is never offered, and neither is one the table names but
    /// this build serves no firmware build for.
    #[test]
    fn the_offer_needs_a_served_build_as_well_as_a_table_row() {
        assert!(!emu_offered_for("lightplayer/desktop"));
        assert!(!emu_offered_for("acme/not-a-board"));
        for board_id in crate::app::devices::target_offer::every_target() {
            if emu_offered_for(board_id) {
                assert_eq!(
                    backing_for(board_id),
                    Backing::Emu,
                    "{board_id} is offered but not in the table"
                );
                let board = lpa_boards::board_by_id(board_id).expect("an offered board is real");
                assert!(
                    lpa_boards::provisioning_build_id(Some(board), Some(board.family.as_str()))
                        .is_some(),
                    "{board_id} is offered with no served build"
                );
            }
        }
    }
}
