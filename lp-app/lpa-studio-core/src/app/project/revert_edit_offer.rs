//! `project/revert-edit` (M6e): revert ONE entry of the changes list.
//!
//! Its `edit` is a choice of every revertible pending edit, by
//! [`UiPendingEdit::key`]; each row of the changes list presses it with its
//! own key, and the agent and ⌘K press the same verb. Published while a
//! revertible edit is pending. Undoable, like a node's Revert: the edit is
//! gone from the overlay, and making it again brings it back.

use crate::{
    OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath, REVERT_EDIT_PARAM,
    REVERT_EDIT_VERB, UiAction, UiOffer, UiPendingEdit, UiPendingEditPhase,
};

/// The offer over `rows` — each a pending edit with the action that
/// reverts it — or `None` when there is nothing to revert. A row without a
/// key cannot be named, so it is left out.
pub fn revert_edit_offer(rows: &[(UiPendingEdit, UiAction)]) -> Option<UiOffer> {
    let reverts: Vec<(String, String, UiAction)> = rows
        .iter()
        .filter_map(|(edit, revert)| Some((edit.key.clone()?, edit_label(edit), revert.clone())))
        .collect();
    let unbound = reverts
        .first()?
        .2
        .clone()
        .with_label("Revert one edit")
        .with_summary("Discard one pending edit of the changes list.")
        .undoable();
    let options = rows
        .iter()
        .filter_map(|(edit, _)| {
            let option = OfferChoice::new(edit.key.clone()?, edit_label(edit));
            Some(match &edit.phase {
                UiPendingEditPhase::Failed { .. } => option.with_detail("failed"),
                UiPendingEditPhase::Persisted => option.with_detail("unsaved"),
            })
        })
        .collect();
    let binder = OfferBinder::new(move |args: &OfferArgs| {
        let picked = args.choice(REVERT_EDIT_PARAM).unwrap_or_default();
        let (_, label, action) = reverts
            .iter()
            .find(|(key, _, _)| key == picked)
            .ok_or_else(|| OfferArgError::Missing {
                name: REVERT_EDIT_PARAM.to_string(),
                label: "edit".to_string(),
            })?;
        Ok(action
            .clone()
            .with_label(format!("Revert {label}"))
            .undoable())
    });
    Some(UiOffer::with_params(
        OfferPath::project().child(REVERT_EDIT_VERB),
        "revert",
        vec![OfferParam::choice(REVERT_EDIT_PARAM, "edit", options, None)],
        binder,
        unbound,
    ))
}

/// How an edit reads in the choice: its node and slot (`Orbit
/// entries[a]`).
fn edit_label(edit: &UiPendingEdit) -> String {
    format!("{} {}", edit.node_label, edit.slot_path_display)
}
