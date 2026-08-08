//! The browser unload gate for unsaved project edits.
//!
//! A reload or tab close takes the wasm sim with it, so every unsaved
//! overlay edit dies with the page. Nothing warned about that before
//! 2026-07-28 — it was easy to demo for an hour and close the tab.
//!
//! **What is and is not gated** lives in core, not here:
//! [`lpa_studio_core::has_unsaved_work`] is the single predicate (persisted
//! edits only — live edits are never written by Save, so warning about them
//! would train the dialog away). This module is only the browser plumbing.
//!
//! The listener installs **once** for the app's lifetime and reads an
//! `Rc<Cell<bool>>` the shell updates on each view reconciliation. Adding
//! and removing a listener per render would be a lot of DOM churn for a
//! boolean, and `beforeunload` handlers are cheap to leave attached.
//!
//! Modern browsers ignore custom text and show their own generic message,
//! so there is deliberately no copy to write: the handler only calls
//! `preventDefault()` and sets `returnValue`, which is what actually arms
//! the prompt.

use std::cell::Cell;
use std::rc::Rc;

/// Shared arming flag: the shell writes it, the listener reads it.
pub(crate) type UnsavedFlag = Rc<Cell<bool>>;

/// Install the `beforeunload` listener. Keep the returned guard alive for
/// the app's lifetime (a `use_hook`) — dropping it removes the listener.
#[cfg(target_arch = "wasm32")]
pub(crate) fn install_unsaved_gate(flag: UnsavedFlag) -> Option<Rc<UnsavedGate>> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let window = web_sys::window()?;
    let callback = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::wrap(Box::new(
        move |event: web_sys::BeforeUnloadEvent| {
            if !flag.get() {
                return;
            }
            // Both are required: `preventDefault` is the modern signal,
            // `returnValue` the legacy one, and browsers still disagree
            // about which arms the prompt.
            event.prevent_default();
            event.set_return_value("");
        },
    ));
    window
        .add_event_listener_with_callback("beforeunload", callback.as_ref().unchecked_ref())
        .ok()?;
    Some(Rc::new(UnsavedGate { window, callback }))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn install_unsaved_gate(_flag: UnsavedFlag) -> Option<Rc<UnsavedGate>> {
    None
}

pub(crate) struct UnsavedGate {
    #[cfg(target_arch = "wasm32")]
    window: web_sys::Window,
    #[cfg(target_arch = "wasm32")]
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::BeforeUnloadEvent)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for UnsavedGate {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        let _ = self.window.remove_event_listener_with_callback(
            "beforeunload",
            self.callback.as_ref().unchecked_ref(),
        );
    }
}

/// Whether dispatching `action` would replace the project loaded on the
/// sim, discarding its unsaved edits.
///
/// The three gallery opens all land on the same `open_from_home` path in
/// the controller and all swap the sim's loaded project. Detaching the lens
/// is deliberately absent: it keeps every runtime session running, so
/// nothing is lost.
pub(crate) fn action_replaces_loaded_project(action: &lpa_studio_core::UiAction) -> bool {
    use lpa_studio_core::HomeOp;

    matches!(
        action.op_as::<HomeOp>(),
        Some(
            HomeOp::OpenPackage { .. } | HomeOp::OpenExample { .. } | HomeOp::CreateProject { .. }
        )
    )
}

/// Ask the user to confirm an action that would discard unsaved edits.
///
/// Returns `true` when the action may proceed. Host builds (and any
/// context without a `window`) proceed silently — the gate is a browser
/// affordance, and refusing to act without one would break tests.
#[cfg(target_arch = "wasm32")]
pub(crate) fn confirm_discarding_unsaved(message: &str) -> bool {
    web_sys::window()
        .and_then(|window| window.confirm_with_message(message).ok())
        .unwrap_or(true)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn confirm_discarding_unsaved(_message: &str) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{HOME_NODE_ID, HomeOp, ProjectController, ProjectOp, UiAction};

    use super::action_replaces_loaded_project;

    fn home(op: HomeOp) -> UiAction {
        UiAction::from_op(HOME_NODE_ID, op)
    }

    #[test]
    fn every_gallery_open_replaces_the_loaded_project() {
        assert!(action_replaces_loaded_project(&home(HomeOp::OpenPackage {
            key: "prjabc".to_string(),
        })));
        assert!(action_replaces_loaded_project(&home(HomeOp::OpenExample {
            id: "basic".to_string(),
        })));
        // Every template creates AND opens, so all three replace the
        // loaded project — a new arm on `ProjectTemplate` must not be able
        // to slip past the discard prompt.
        for template in [
            lpa_studio_core::ProjectTemplate::Blank,
            lpa_studio_core::ProjectTemplate::Pattern1d,
            lpa_studio_core::ProjectTemplate::Pattern2d,
        ] {
            assert!(action_replaces_loaded_project(&home(
                HomeOp::CreateProject { template }
            )));
        }
    }

    #[test]
    fn detaching_the_lens_is_not_a_replacement() {
        // The whole point of the narrow gate: going back to the gallery
        // keeps every runtime session running, edits included, so it must
        // never raise the discard prompt.
        let detach = UiAction::from_op(ProjectController::NODE_ID, ProjectOp::DetachLens);
        assert!(!action_replaces_loaded_project(&detach));
    }

    #[test]
    fn library_housekeeping_is_not_a_replacement() {
        // Renaming or duplicating a card does not touch what the sim has
        // loaded, so these must not prompt either.
        assert!(!action_replaces_loaded_project(&home(
            HomeOp::RenamePackage {
                uid: "prjabc".to_string(),
                name: "Renamed".to_string(),
            }
        )));
        assert!(!action_replaces_loaded_project(&home(
            HomeOp::DuplicatePackage {
                uid: "prjabc".to_string(),
            }
        )));
    }
}
