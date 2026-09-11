//! What the target pickers offer: Desktop, then the boards (PD16, D41, Q10).
//!
//! Two surfaces ask the same question and get almost the same answer, so
//! the answer is decided here rather than twice in the renderer:
//!
//! - **The Devices page's add slot** ("start a board here ▾", D44) offers
//!   only what can actually be started — Desktop and every catalog board
//!   with a checked-in runtime manifest — and tags each row with its
//!   [`Backing`]. A board this build can emulate gets **two** rows (D1).
//! - **A project's Hardware row** (D41) offers *targets*, so it lists every
//!   catalog board; the ones with no runtime manifest are listed
//!   **disabled** with the reason (Q10), because a target the sim cannot
//!   wear would silently open as Desktop with a notice. Nothing in that row
//!   says emu or sim: "a board is just a board", and emu/sim is something a
//!   *device* is.
//!
//! Both read the catalog in its own order, with Desktop pulled out into its
//! own group and placed first — it is the default target, and the one every
//! new project gets.
//!
//! # Two rows, and only where the choice exists (D1)
//!
//! Sim vs emu is the USER's choice, not a default this build flips: a board
//! with an emulator is offered as an emu **and** as a sim, both plainly
//! tagged, and neither is preselected. The emu row comes first within the
//! board's own group — exact, then fast — and the hint below the menu
//! explains the two words the moment either row exists.
//!
//! The second row belongs to [`TargetScope::Runnable`] alone. D41 is the
//! reason and it is not a detail: the Hardware row picks *hardware*, says
//! nothing about emu or sim, and renders no tags — so a second, visually
//! identical row of the same board there would be a choice with no visible
//! difference and no meaning.

use crate::app::library::project_target::DESKTOP_BOARD_ID;

use super::runtime_backing::{Backing, backing_for, emu_offered_for};

/// Which half of the menu a choice sits in.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TargetGroup {
    /// A computer. One row, always first.
    Desktop,
    /// The catalog boards.
    Boards,
}

impl TargetGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Desktop => "Desktop",
            Self::Boards => "Boards",
        }
    }
}

/// How much of the catalog an offer covers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetScope {
    /// Only what this build can start: Desktop and the boards with a
    /// runtime manifest. The Devices-page picker's scope — offering a row
    /// that cannot be started is offering a button that does nothing.
    Runnable,
    /// Every catalog target, with the unrunnable ones marked. A project's
    /// Hardware row's scope: `target` names hardware, and hardware Studio
    /// cannot simulate yet is still hardware a project can be for.
    Everything,
}

/// One target a picker can offer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetChoice {
    /// The catalog board id — what a manifest's `target` holds, and what a
    /// sim record wears. Desktop's is [`DESKTOP_BOARD_ID`].
    pub board_id: String,
    /// The catalog's display name ("Desktop", "XIAO ESP32-C6").
    pub title: String,
    pub group: TargetGroup,
    /// What picking this row starts.
    ///
    /// In [`TargetScope::Runnable`] it is the row's OWN runtime — a board
    /// with an emulator has one row of each (D1) — and the add slot renders
    /// it as the row's tag. In [`TargetScope::Everything`] there is one row
    /// per board, it carries the advisory `backing_for` answer, and the
    /// Hardware row deliberately renders nothing from it (D41).
    ///
    /// It is therefore the second half of a row's identity: `board_id`
    /// alone no longer names one row, and a renderer keying rows must key
    /// on both.
    pub backing: Backing,
    /// Whether a runtime for this target can actually be started — a
    /// checked-in runtime manifest exists (Q10). `false` rows appear only
    /// in [`TargetScope::Everything`], disabled, wearing [`Self::unavailable`].
    pub runnable: bool,
}

impl TargetChoice {
    /// Why this row is disabled, when it is.
    pub fn unavailable(&self) -> Option<&'static str> {
        (!self.runnable).then_some("no hardware manifest yet")
    }
}

/// Everything a target menu draws.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetOffer {
    /// Desktop first, then the boards in catalog order.
    pub choices: Vec<TargetChoice>,
    /// The sentence explaining the emu/sim choice — present only in a menu
    /// that actually offers one (A2). Inert text explaining a choice nobody
    /// has is noise.
    pub hint: Option<&'static str>,
}

impl TargetOffer {
    /// The choices in one group, in offer order.
    pub fn group(&self, group: TargetGroup) -> impl Iterator<Item = &TargetChoice> {
        self.choices
            .iter()
            .filter(move |choice| choice.group == group)
    }
}

/// The sentence under the rows when a build can both emulate and simulate.
///
/// No modifier key anywhere (D1): the two rows ARE the choice, and a hidden
/// `⌥` gesture would be a second way to say the same thing that nobody can
/// see. The sentence explains the two words and stops.
const EMU_SIM_HINT: &str = "Emu runs the board's real firmware; sim runs the desktop firmware \
                            wearing the board — faster, less exact. Boards without an emulator \
                            yet get a sim.";

/// Every target id the app knows: Desktop first, then the catalog boards in
/// their own order.
///
/// The `purchasable_boards` filter is what keeps Desktop from appearing
/// twice — it is a catalog board file, and the Boards page is the one
/// surface that hides it (P1's revised DD8).
pub fn every_target() -> impl Iterator<Item = &'static str> {
    core::iter::once(DESKTOP_BOARD_ID)
        .chain(lpa_boards::purchasable_boards().map(|board| board.board_id.as_str()))
}

/// The targets a menu of `scope` offers.
pub fn target_offer(scope: TargetScope) -> TargetOffer {
    let choices: Vec<TargetChoice> = every_target()
        .flat_map(|board_id| {
            let group = match board_id == DESKTOP_BOARD_ID {
                true => TargetGroup::Desktop,
                false => TargetGroup::Boards,
            };
            let runnable = lpa_boards::runtime_manifest_json(board_id).is_some();
            let row = |backing| TargetChoice {
                board_id: board_id.to_string(),
                title: crate::board_display_name(board_id),
                group,
                backing,
                runnable,
            };
            // Emu first within the board's own group: exact, then fast.
            // Only where the choice is visible (see the module doc) and
            // only where it is real — `emu_offered_for` is D21's join.
            match scope {
                TargetScope::Runnable => {
                    let emu = emu_offered_for(board_id).then(|| row(Backing::Emu));
                    emu.into_iter().chain(Some(row(Backing::Sim)))
                }
                // One row per target, wearing the advisory "what this build
                // would run it as" — which nothing renders here (D41).
                TargetScope::Everything => None
                    .into_iter()
                    .chain(Some(row(backing_for(board_id)))),
            }
        })
        .filter(|choice| match scope {
            TargetScope::Runnable => choice.runnable,
            TargetScope::Everything => true,
        })
        .collect();
    // The hint explains a CHOICE, so it appears exactly where one is on
    // offer: a menu that put an emu row beside a sim row. The wide scope
    // has one row per board and says nothing about backings (D41), so it
    // has nothing to explain even though its rows carry the advisory word.
    let hint = (scope == TargetScope::Runnable
        && choices
            .iter()
            .any(|choice| choice.backing == Backing::Emu))
    .then_some(EMU_SIM_HINT);
    TargetOffer { choices, hint }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Desktop leads, alone in its group, and is never repeated among the
    /// boards.
    #[test]
    fn desktop_comes_first_and_only_once() {
        let offer = target_offer(TargetScope::Runnable);

        assert_eq!(offer.choices[0].board_id, DESKTOP_BOARD_ID);
        assert_eq!(offer.choices[0].title, "Desktop");
        assert_eq!(offer.choices[0].group, TargetGroup::Desktop);
        assert_eq!(offer.group(TargetGroup::Desktop).count(), 1);
        assert!(
            offer
                .group(TargetGroup::Boards)
                .all(|choice| choice.board_id != DESKTOP_BOARD_ID),
            "Desktop is a board file, but it is not one of the Boards"
        );
    }

    /// The picker offers only what it can start (Q10): the two display-only
    /// catalog boards are absent from the runnable scope.
    #[test]
    fn the_picker_offers_only_startable_targets() {
        let offer = target_offer(TargetScope::Runnable);

        assert!(offer.choices.iter().all(|choice| choice.runnable));
        for missing in ["quinled/dig-uno", "espressif/esp32-devkitc-v4"] {
            assert!(
                !offer
                    .choices
                    .iter()
                    .any(|choice| choice.board_id == missing),
                "{missing} has no runtime manifest and cannot be started"
            );
        }
        assert!(
            offer
                .choices
                .iter()
                .any(|choice| choice.board_id == "seeed/xiao-esp32-c6"),
            "a board with a manifest is offered"
        );
    }

    /// The Hardware row lists every target — `target` names hardware, and a
    /// project may be FOR a board Studio cannot simulate yet — but says so
    /// on the rows it cannot start.
    #[test]
    fn the_hardware_row_lists_everything_and_marks_what_it_cannot_run() {
        let offer = target_offer(TargetScope::Everything);

        let dig_uno = offer
            .choices
            .iter()
            .find(|choice| choice.board_id == "quinled/dig-uno")
            .expect("a display-only board is still a target");
        assert!(!dig_uno.runnable);
        assert_eq!(dig_uno.unavailable(), Some("no hardware manifest yet"));

        let c6 = offer
            .choices
            .iter()
            .find(|choice| choice.board_id == "seeed/xiao-esp32-c6")
            .expect("a simulatable board");
        assert!(c6.runnable);
        assert_eq!(c6.unavailable(), None);

        assert!(
            offer.choices.len() > target_offer(TargetScope::Runnable).choices.len(),
            "the wider scope is wider"
        );
    }

    /// D1: a board this build can emulate is offered BOTH ways, plainly
    /// tagged, emu first — and neither row is a default.
    #[test]
    fn an_emulated_board_is_offered_twice_emu_first() {
        let offer = target_offer(TargetScope::Runnable);
        let c6: Vec<&TargetChoice> = offer
            .choices
            .iter()
            .filter(|choice| choice.board_id == "seeed/xiao-esp32-c6")
            .collect();

        assert_eq!(c6.len(), 2, "the C6 is offered as an emu and as a sim");
        assert_eq!(c6[0].backing, Backing::Emu, "exact first, then fast");
        assert_eq!(c6[1].backing, Backing::Sim);
        assert_eq!(
            c6.iter().map(|row| row.backing.tag()).collect::<Vec<_>>(),
            vec!["emu", "sim"],
            "both plainly tagged"
        );
        assert_eq!(c6[0].title, c6[1].title, "one board, two runtimes");

        // The two rows are adjacent: the choice reads as one board's, not
        // as two boards that happen to share a name.
        let first = offer
            .choices
            .iter()
            .position(|choice| choice.board_id == "seeed/xiao-esp32-c6")
            .expect("the C6 is offered");
        assert_eq!(offer.choices[first + 1].board_id, "seeed/xiao-esp32-c6");
    }

    /// Every other target keeps its one row: Desktop (no emulator) and a
    /// board this build emulates nothing for.
    #[test]
    fn a_target_with_no_emulator_keeps_one_row() {
        let offer = target_offer(TargetScope::Runnable);

        for board_id in ["lightplayer/desktop", "seeed/xiao-esp32-s3-plus"] {
            let rows: Vec<&TargetChoice> = offer
                .choices
                .iter()
                .filter(|choice| choice.board_id == board_id)
                .collect();
            assert_eq!(rows.len(), 1, "{board_id}");
            assert_eq!(rows[0].backing, Backing::Sim, "{board_id}");
        }
    }

    /// D41: the Hardware row picks HARDWARE. One row per board, whatever
    /// this build could run it as, and no sentence explaining a choice it
    /// does not offer.
    #[test]
    fn the_hardware_row_never_doubles_a_board_and_has_no_hint() {
        let offer = target_offer(TargetScope::Everything);

        assert_eq!(offer.hint, None);
        let mut seen: Vec<&str> = offer
            .choices
            .iter()
            .map(|choice| choice.board_id.as_str())
            .collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), before, "one row per board id");
    }

    /// The hint has earned its place (A2's inverse), and it names no
    /// modifier key: the two rows ARE the choice (D1).
    #[test]
    fn the_hint_explains_the_choice_the_menu_offers() {
        assert_eq!(target_offer(TargetScope::Runnable).hint, Some(EMU_SIM_HINT));
        assert!(EMU_SIM_HINT.contains("real firmware"));
        assert!(EMU_SIM_HINT.contains("desktop firmware"));
        for modifier in ['⌥', '⌘', '⇧'] {
            assert!(
                !EMU_SIM_HINT.contains(modifier),
                "no modifier key anywhere: {modifier}"
            );
        }
    }

    /// Every offered row wears a tag from the capability table, so no row
    /// can render tagless — and the tag is always one of the two words.
    #[test]
    fn every_row_is_tagged() {
        for scope in [TargetScope::Runnable, TargetScope::Everything] {
            for choice in target_offer(scope).choices {
                assert!(
                    ["emu", "sim"].contains(&choice.backing.tag()),
                    "{} in {scope:?}",
                    choice.board_id
                );
            }
        }
    }
}
