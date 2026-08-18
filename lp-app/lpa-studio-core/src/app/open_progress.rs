//! The page-wide signals describing THE open in flight: which open is
//! current (supersede), how far it has got, and how it ended.
//!
//! # Why signals
//!
//! The same reason [`crate::app::open_priority`] is one: producer and
//! consumer never meet. The producer is [`crate::StudioController`]'s open
//! flow, parked inside the actor's serial action loop; the consumers are a
//! frame deep in the page's view tree and — for the supersede check — the
//! parked flow itself, which cannot receive anything through the queue it
//! is blocking. Everything here runs on the browser's single thread, so a
//! thread-local IS the shared state; native builds get one per test
//! thread, which keeps unit tests independent.
//!
//! # Supersede (D4)
//!
//! The newest click wins. A click ENQUEUES its open, and the enqueue —
//! not the open — bumps [`current_open_generation`]
//! ([`note_open_requested`], called from the command sender). The actor
//! processes actions one at a time, so the second click's action cannot
//! run until the first open yields; the generation bump is therefore the
//! one thing that reaches a parked open, and it reaches it *immediately*.
//!
//! The running open records its own generation at
//! [`note_open_started`] and asks [`open_superseded`] at each await
//! boundary it can afford to unwind from (entry, post-boot, post-lock).
//! A stale open abandons its `OpenReceipt` (releasing the project lock)
//! and returns quietly — nothing logged, nothing shown, because the user
//! did not fail at anything, they changed their mind.
//!
//! What it does NOT tear down is the browser worker: the engine binary is
//! identical for every open and projects deploy into a booted worker
//! later, so a superseded open leaves the sim session standing and the new
//! open reuses it. Tearing it down would make the newest click the slowest
//! one.

use core::cell::{Cell, RefCell};

use crate::UiAction;

/// How far the open in flight has got, as far as the CORE can see.
///
/// Deliberately coarse: the engine's own download/compile/boot phases are
/// observable at the platform edge (`lpa_link`'s engine cache and boot
/// wait), and the view layer folds those in. Core reports only the
/// milestones it owns.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum OpenStage {
    /// No open in flight and none has failed since.
    #[default]
    Idle,
    /// Dispatched; the runtime is being reached (boot, connect, attach).
    /// The platform's engine signals refine this into "downloading" /
    /// "starting".
    Starting,
    /// The runtime is up; the project is being read, locked and deployed.
    PreparingProject,
    /// The open ended in an error the user has to see, with the way back.
    Failed(OpenFailure),
}

/// A terminal open failure, with everything Retry needs.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenFailure {
    /// The mapped, user-facing message (`UiError::message`) — the same
    /// wording the console entry carries.
    pub message: String,
    /// Re-dispatching this action retries exactly the open that failed.
    pub retry: UiAction,
}

thread_local! {
    static STAGE: RefCell<OpenStage> = const { RefCell::new(OpenStage::Idle) };
    /// Bumped by every enqueued open request; the newest value is the
    /// current open.
    static REQUESTED: Cell<u64> = const { Cell::new(0) };
    /// The generation of the open the actor is running right now.
    static RUNNING: Cell<u64> = const { Cell::new(0) };
}

/// The stage the open in flight (or the last failed one) reports.
pub fn open_stage() -> OpenStage {
    STAGE.with(|stage| stage.borrow().clone())
}

/// The generation of the newest requested open.
pub fn current_open_generation() -> u64 {
    REQUESTED.with(Cell::get)
}

/// Record that a new open has been REQUESTED (enqueued), superseding any
/// open already in flight. Returns the new generation.
///
/// Called from the command sender, which is the one place every open
/// dispatch passes through — a card click, a `/p/…` route resolution, a
/// docs `open-in-studio` embed — and the one place that runs while an
/// earlier open is parked.
pub fn note_open_requested() -> u64 {
    // A standing failure is cleared HERE rather than when the open starts:
    // the queue can hold the new open for a moment, and in that gap a
    // frame would otherwise show the PREVIOUS project's error over the
    // route of the one the user just clicked.
    if matches!(open_stage(), OpenStage::Failed(_)) {
        set_stage(OpenStage::Idle);
    }
    REQUESTED.with(|generation| {
        let next = generation.get().saturating_add(1);
        generation.set(next);
        next
    })
}

/// The running open has begun: it adopts the newest requested generation.
pub(crate) fn note_open_started() {
    RUNNING.with(|running| running.set(current_open_generation()));
    set_stage(OpenStage::Starting);
}

/// Whether the open the actor is running has been superseded by a newer
/// click. Asked at await boundaries; `true` means unwind quietly.
pub fn open_superseded() -> bool {
    RUNNING.with(Cell::get) != current_open_generation()
}

/// The runtime is up; the remaining work is the project itself.
pub(crate) fn note_preparing_project() {
    if !open_superseded() {
        set_stage(OpenStage::PreparingProject);
    }
}

/// The open landed, was superseded, or otherwise ended without an error
/// the user must act on.
pub(crate) fn note_open_settled() {
    set_stage(OpenStage::Idle);
}

/// The open failed terminally. `retry` re-dispatches the same open.
pub(crate) fn note_open_failed(message: impl Into<String>, retry: UiAction) {
    set_stage(OpenStage::Failed(OpenFailure {
        message: message.into(),
        retry,
    }));
}

fn set_stage(next: OpenStage) {
    STAGE.with(|stage| *stage.borrow_mut() = next);
}

/// Forget everything (test-only): the signals are per-thread, and a test
/// that leaves a failure standing would colour the next one.
#[cfg(test)]
pub(crate) fn reset_for_test() {
    set_stage(OpenStage::Idle);
    REQUESTED.with(|generation| generation.set(0));
    RUNNING.with(|running| running.set(0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ControllerId, HOME_NODE_ID, HomeOp};

    fn open_action(key: &str) -> UiAction {
        UiAction::from_op(
            ControllerId::new(HOME_NODE_ID),
            HomeOp::OpenPackage {
                key: key.to_string(),
            },
        )
    }

    #[test]
    fn a_lone_open_is_never_stale() {
        reset_for_test();
        note_open_requested();
        note_open_started();
        assert!(!open_superseded());
        assert_eq!(open_stage(), OpenStage::Starting);
    }

    #[test]
    fn a_second_request_supersedes_the_running_open() {
        // The demo case: click A, then click B while A is still parked in
        // the actor. B's ENQUEUE is what reaches A.
        reset_for_test();
        note_open_requested();
        note_open_started();
        note_open_requested();
        assert!(open_superseded(), "the newest click wins");

        // …and when B's action finally runs, it is current again.
        note_open_started();
        assert!(!open_superseded());
    }

    #[test]
    fn a_superseded_open_never_overwrites_the_stage() {
        reset_for_test();
        note_open_requested();
        note_open_started();
        note_open_requested();
        note_preparing_project();
        assert_eq!(
            open_stage(),
            OpenStage::Starting,
            "a stale open must not narrate over the click that replaced it"
        );
    }

    #[test]
    fn a_failure_carries_its_own_retry_and_a_new_open_clears_it() {
        reset_for_test();
        note_open_requested();
        note_open_started();
        note_open_failed("the simulator did not connect", open_action("prjx"));
        let OpenStage::Failed(failure) = open_stage() else {
            panic!("failed stage expected");
        };
        assert_eq!(failure.message, "the simulator did not connect");
        assert_eq!(failure.retry, open_action("prjx"));

        // The REQUEST clears it, not the start: the action can sit in the
        // queue, and a stale error must not colour the new click's route.
        note_open_requested();
        assert_eq!(open_stage(), OpenStage::Idle, "Retry clears the error");
        note_open_started();
        assert_eq!(open_stage(), OpenStage::Starting);
    }
}
