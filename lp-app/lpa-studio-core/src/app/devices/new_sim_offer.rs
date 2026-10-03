//! `devices/new-sim`: the add slot's "start a board here", as an offer that
//! takes which board and which runtime.
//!
//! The rows have always been core's ([`target_offer`] in the
//! [`TargetScope::Runnable`] scope: Desktop and every board with a runtime
//! manifest, a board this build can emulate offered as an emu AND as a
//! sim); only the picked row was the web's. A row is two values, so the
//! offer takes two Choice params — `board` and `backing` — and the binder
//! builds the same [`SimCreateOp`] the row dispatches. Neither is
//! preselected: emu or sim is the user's choice (D1), never a default.
//!
//! A pair no row offers (an emu of a board this build cannot emulate) is
//! refused by the binder with the reason, as the controller would refuse
//! the op.

use super::runtime_backing::Backing;
use super::sim_create_op::SimCreateOp;
use super::target_offer::{TargetChoice, TargetScope, target_offer};
use crate::{OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath, UiOffer};

/// The New sim offer's board parameter.
pub const NEW_SIM_BOARD_PARAM: &str = "board";
/// The New sim offer's runtime parameter.
pub const NEW_SIM_BACKING_PARAM: &str = "backing";

/// What the offer is called: the add slot's detour, said whole.
const LABEL: &str = "Start a board here";

/// `devices/new-sim`.
pub fn new_sim_offer() -> UiOffer {
    let rows = target_offer(TargetScope::Runnable).choices;
    let mut boards: Vec<OfferChoice> = Vec::new();
    for row in &rows {
        if boards.iter().any(|board| board.value == row.board_id) {
            continue;
        }
        let backings: Vec<&str> = rows
            .iter()
            .filter(|other| other.board_id == row.board_id)
            .map(|other| other.backing.tag())
            .collect();
        boards.push(
            OfferChoice::new(&row.board_id, &row.title).with_detail(format!(
                "{} \u{b7} {}",
                row.group.label(),
                backings.join(" or ")
            )),
        );
    }
    let backings = vec![
        OfferChoice::new(Backing::Emu.tag(), "Emulated")
            .with_detail("The board's real firmware on an emulated chip: exact, slower."),
        OfferChoice::new(Backing::Sim.tag(), "Simulated")
            .with_detail("The desktop firmware wearing the board: faster, less exact."),
    ];
    let params = vec![
        OfferParam::choice(NEW_SIM_BOARD_PARAM, "board", boards, None),
        OfferParam::choice(NEW_SIM_BACKING_PARAM, "runtime", backings, None),
    ];
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let board = args.choice(NEW_SIM_BOARD_PARAM).unwrap_or_default();
        let backing = args.choice(NEW_SIM_BACKING_PARAM).unwrap_or_default();
        let row = rows
            .iter()
            .find(|row| row.board_id == board && row.backing.tag() == backing)
            .ok_or_else(|| OfferArgError::OptionDisabled {
                name: NEW_SIM_BACKING_PARAM.to_string(),
                value: backing.to_string(),
                reason: format!(
                    "this build has no emulator for {}",
                    crate::board_display_name(board)
                ),
            })?;
        Ok(create_action(row))
    });
    let unbound = SimCreateOp {
        target: String::new(),
        name: None,
        backing: Backing::Sim,
    }
    .into_action()
    .with_label(LABEL);
    UiOffer::with_params(
        OfferPath::devices().child("new-sim"),
        "add",
        params,
        binder,
        unbound,
    )
}

/// The creation op one row starts.
fn create_action(row: &TargetChoice) -> crate::UiAction {
    SimCreateOp::action_for(row.board_id.clone(), row.backing).with_label(LABEL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::library::project_target::DESKTOP_BOARD_ID;

    #[test]
    fn new_sim_lists_the_runnable_targets_and_binds_a_row() {
        let offer = new_sim_offer();
        assert_eq!(offer.path.to_string(), "devices/new-sim");
        assert!(!offer.is_enabled(), "nothing is preselected (D1)");
        assert!(offer.consequence().is_routine());

        let crate::OfferParamKind::Choice { options, .. } = &offer.params()[0].kind else {
            panic!("{:?}", offer.params());
        };
        let rows = target_offer(TargetScope::Runnable).choices;
        let mut ids: Vec<&str> = rows.iter().map(|row| row.board_id.as_str()).collect();
        ids.dedup();
        assert_eq!(
            options.iter().map(|o| o.value.as_str()).collect::<Vec<_>>(),
            ids,
            "one option per target, Desktop first"
        );
        assert_eq!(options[0].value, DESKTOP_BOARD_ID);

        for row in &rows {
            let bound = offer
                .press(
                    &OfferArgs::new()
                        .with("board", &row.board_id)
                        .with("backing", row.backing.tag()),
                )
                .unwrap_or_else(|error| panic!("{row:?}: {error}"));
            assert_eq!(
                bound.op_as::<SimCreateOp>(),
                Some(&SimCreateOp {
                    target: row.board_id.clone(),
                    name: None,
                    backing: row.backing,
                })
            );
        }
    }

    #[test]
    fn an_emu_of_a_board_this_build_cannot_emulate_is_refused() {
        let refused = new_sim_offer()
            .press(
                &OfferArgs::new()
                    .with("board", DESKTOP_BOARD_ID)
                    .with("backing", "emu"),
            )
            .unwrap_err();
        assert!(
            refused.to_string().contains("no emulator for Desktop"),
            "{refused}"
        );
        assert!(
            new_sim_offer()
                .press(&OfferArgs::new().with("board", DESKTOP_BOARD_ID))
                .is_err(),
            "the runtime is the user's choice, never a default"
        );
    }
}
