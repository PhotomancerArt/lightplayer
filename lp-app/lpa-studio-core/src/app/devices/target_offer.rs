//! What the target pickers offer: Desktop, then the boards (PD16, D41, Q10).
//!
//! Two surfaces ask the same question and get almost the same answer, so
//! the answer is decided here rather than twice in the renderer:
//!
//! - **The Devices page's add slot** ("start a board here ▾", D44) offers
//!   only what can actually be started — Desktop and every catalog board
//!   with a checked-in runtime manifest — and tags each row with its
//!   [`Backing`] ("sim" in this build).
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

use crate::app::library::project_target::DESKTOP_BOARD_ID;

use super::runtime_backing::{Backing, backing_for};

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
    /// What this build would run it as. The add slot renders it as the
    /// row's tag; the Hardware row deliberately does not (D41).
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
    /// The sentence explaining the emu/sim choice — present only when the
    /// menu actually contains an `emu` row (A2). Inert text explaining a
    /// choice nobody has is noise, so it is `None` in this build.
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
const EMU_SIM_HINT: &str = "Emu runs the board's real firmware; sim runs the desktop firmware \
                            wearing the board — faster, less exact. Boards without an emulator \
                            yet get a sim. Hold ⌥ to simulate instead.";

/// Every target id the app knows: Desktop first, then the catalog boards in
/// their own order.
///
/// The `purchasable_boards` filter is what keeps Desktop from appearing
/// twice — it is a catalog board file, and the Boards page is the one
/// surface that hides it (P1's revised DD8).
pub fn every_target() -> impl Iterator<Item = &'static str> {
    core::iter::once(DESKTOP_BOARD_ID).chain(
        lpa_boards::purchasable_boards().map(|board| board.board_id.as_str()),
    )
}

/// The targets a menu of `scope` offers.
pub fn target_offer(scope: TargetScope) -> TargetOffer {
    let choices: Vec<TargetChoice> = every_target()
        .map(|board_id| TargetChoice {
            board_id: board_id.to_string(),
            title: crate::board_display_name(board_id),
            group: match board_id == DESKTOP_BOARD_ID {
                true => TargetGroup::Desktop,
                false => TargetGroup::Boards,
            },
            backing: backing_for(board_id),
            runnable: lpa_boards::runtime_manifest_json(board_id).is_some(),
        })
        .filter(|choice| match scope {
            TargetScope::Runnable => choice.runnable,
            TargetScope::Everything => true,
        })
        .collect();
    let hint = choices
        .iter()
        .any(|choice| choice.backing == Backing::Emu)
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
                !offer.choices.iter().any(|choice| choice.board_id == missing),
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

    /// A2: nothing is emulated, so the hint has nothing to explain and is
    /// not rendered. It is written out so the day a row flips to `emu` the
    /// sentence is already the right one.
    #[test]
    fn the_emu_sim_hint_stays_hidden_while_nothing_is_emulated() {
        for scope in [TargetScope::Runnable, TargetScope::Everything] {
            assert_eq!(target_offer(scope).hint, None, "{scope:?}");
        }
        assert!(EMU_SIM_HINT.contains("simulate instead"));
    }

    /// Every offered row wears a tag from the capability table, so no row
    /// can render tagless.
    #[test]
    fn every_row_is_tagged() {
        for choice in target_offer(TargetScope::Everything).choices {
            assert_eq!(choice.backing.tag(), "sim", "{}", choice.board_id);
        }
    }
}
