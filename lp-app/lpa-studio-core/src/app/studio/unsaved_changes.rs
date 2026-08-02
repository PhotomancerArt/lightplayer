//! What counts as "you would lose work" — the one decision behind the
//! Studio's unload gate.
//!
//! The gate guards **loss**, not navigation. Three paths look similar and
//! are not:
//!
//! | Path | Loses edits? | Gated |
//! |---|---|---|
//! | Page reload / tab close | yes — the wasm sim dies with the page | yes |
//! | Detaching the lens to the gallery | no — the runtime session keeps running | **no** |
//! | Loading a different project onto the sim | yes — it replaces the loaded one | yes, by confirm |
//!
//! Gating lens-detach would be pure noise: per `router.rs`, navigating to
//! `Home` while the editor is open detaches the lens and every runtime
//! session survives, edits included.
//!
//! Only **persisted** edits count — and since D7 that is structural, not a
//! filter applied here: Debug overrides are transient by nature, so they
//! never enter the [`DirtySummary`] at all. Warning about them would train
//! users to dismiss the dialog. Failed edits are counted but are not pending
//! work either — they never reached the overlay.
//!
//! The browser plumbing lives in the web edge
//! (`lpa-studio-web/src/unsaved_gate.rs`); core stays sans-IO and only
//! answers the question.

use crate::DirtySummary;

/// Whether losing the current runtime would lose authored work.
///
/// This is the predicate behind both `beforeunload` and the
/// swap-the-loaded-project confirm.
pub fn has_unsaved_work(dirty: &DirtySummary) -> bool {
    dirty.persisted > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_persisted_edits_gate() {
        assert!(has_unsaved_work(&dirty(1, 0)));
        assert!(has_unsaved_work(&dirty(3, 1)));
    }

    #[test]
    fn a_clean_project_never_gates() {
        assert!(!has_unsaved_work(&DirtySummary::clean()));
    }

    #[test]
    fn debug_only_edits_are_absent_from_the_summary_and_never_gate() {
        // D7 made this structural rather than a filter applied here: a
        // project whose ONLY pending edits are debug overrides produces the
        // clean summary — there is no live bucket left for the gate to
        // ignore. (`DirtySummary::for_slot` proves the classification side;
        // this asserts the gate's half: clean means no warning.)
        let debug_only = DirtySummary::clean();
        assert!(debug_only.is_clean());
        assert!(!has_unsaved_work(&debug_only));
    }

    #[test]
    fn failed_only_edits_never_gate() {
        // A rejected edit is not pending work: it never reached the
        // overlay, so there is nothing for Save to write.
        assert!(!has_unsaved_work(&dirty(0, 2)));
    }

    fn dirty(persisted: usize, failed: usize) -> DirtySummary {
        DirtySummary { persisted, failed }
    }
}
