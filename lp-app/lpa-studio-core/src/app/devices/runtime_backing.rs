//! What actually runs a target: the one-row capability table (PD16, A2).
//!
//! A **backing** is how this build can run a piece of hardware that is not
//! on the desk:
//!
//! - **sim** — the desktop firmware wearing the target's hardware manifest.
//!   Fast, and exact about the hardware's *shape* (its endpoints, its
//!   limits) rather than about its silicon.
//! - **emu** — the target's own firmware image on an emulated SoC. Exact,
//!   and slower. No target has one in this build.
//!
//! It is a table rather than a `match` on the board id because the answer
//! is a property of what Studio SHIPS, not of the board: the day
//! `lp-emu-esp32c6` is wired in behind a device (mode A on the emulator
//! roadmap), one row here flips and every surface that asks — the picker's
//! row tag, the hint line, the runtime band's kind word — follows without a
//! second decision anywhere.
//!
//! [`EMULATED_TARGETS`] is empty today, so every row says `sim` — which is
//! also why the picker's "simulate instead" hint stays hidden (A2): a
//! sentence explaining a choice nobody has is noise, and the offer asks the
//! table rather than hard-coding "hidden for now".

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

/// The targets this build ships an emulator for.
///
/// **Empty on purpose.** Mode A on the emulator roadmap (`lp-emu-esp32c6`
/// behind a device) adds ids here, and the picker's tags, the hint line and
/// the band's kind word all follow from that one edit.
pub const EMULATED_TARGETS: &[&str] = &[];

/// The backing this build has for `board_id`.
///
/// One row per target, and today every row says the same thing. The
/// argument is the board id (Desktop included — it is a board file like any
/// other) so callers never have to hold a [`ProjectTarget`] to ask.
///
/// [`ProjectTarget`]: crate::app::library::ProjectTarget
pub fn backing_for(board_id: &str) -> Backing {
    match EMULATED_TARGETS.contains(&board_id) {
        true => Backing::Emu,
        false => Backing::Sim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table answers for every target the picker can offer, Desktop
    /// included — an unanswered row would render a tagless option.
    #[test]
    fn every_target_has_a_backing() {
        for board_id in crate::app::devices::target_offer::every_target() {
            assert_eq!(
                backing_for(board_id),
                Backing::Sim,
                "{board_id} is simulated in this build"
            );
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

    /// A2: nothing is emulated, so the picker's emu/sim hint has nothing to
    /// explain. When this test starts failing, the hint has earned its place.
    #[test]
    fn nothing_is_emulated_in_this_build() {
        assert!(EMULATED_TARGETS.is_empty());
    }
}
