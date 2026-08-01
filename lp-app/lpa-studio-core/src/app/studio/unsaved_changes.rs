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
//! Only **persisted** edits count. Live/transient edits apply to the
//! running project and are explicitly never written by Save, so warning
//! about them would train users to dismiss the dialog. Failed edits are
//! not pending work either — they never reached the overlay.
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
        assert!(has_unsaved_work(&dirty(1, 0, 0)));
        assert!(has_unsaved_work(&dirty(3, 2, 1)));
    }

    #[test]
    fn a_clean_project_never_gates() {
        assert!(!has_unsaved_work(&DirtySummary::clean()));
    }

    #[test]
    fn live_only_edits_never_gate() {
        // Live controls apply to the running project and are never written
        // by Save — warning about them would cry wolf on every knob turn.
        assert!(!has_unsaved_work(&dirty(0, 5, 0)));
    }

    #[test]
    fn failed_only_edits_never_gate() {
        // A rejected edit is not pending work: it never reached the
        // overlay, so there is nothing for Save to write.
        assert!(!has_unsaved_work(&dirty(0, 0, 2)));
    }

    fn dirty(persisted: usize, transient: usize, failed: usize) -> DirtySummary {
        DirtySummary {
            persisted,
            transient,
            failed,
        }
    }
}
