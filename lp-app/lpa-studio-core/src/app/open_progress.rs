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
//! # A board (2026-09-24)
//!
//! An open onto real silicon has nothing the engine signals can see: no
//! download, no worker boot. What it has is a board that may not be here
//! yet ([`OpenStage::WaitingForDevice`]) and then a conversation over its
//! wire ([`OpenStage::OnDevice`]) — stop, upload, load (the board compiles
//! every shader here), read back. Before these the frame said "Opening
//! project…" for the whole of it, which on a C6 is long enough to read as
//! a crash (Yona, JSON Pack sitting). A failure while the board was
//! mid-step names the step, so a timeout says what it was waiting for.
//!
//! # Cancel
//!
//! [`cancel_open`] is supersede without a replacement, plus one thing a
//! supersede cannot do: it wakes the request the open is parked on
//! ([`cancelled_since`], raced against every device request deadline), so
//! the actor is free for the Reset or the navigation that follows instead
//! of sitting out a 20 s deadline behind a board that is not answering.
//!
//! What it does NOT tear down is the browser worker: the engine binary is
//! identical for every open and projects deploy into a booted worker
//! later, so a superseded open leaves the sim session standing and the new
//! open reuses it. Tearing it down would make the newest click the slowest
//! one.

use core::cell::{Cell, RefCell};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

use crate::{DeviceId, UiAction};

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
    /// The open is HELD on a board that is not ready for it. The actor is
    /// free meanwhile (this is the pending-lens hold), so nothing else
    /// would tell the frame.
    WaitingForDevice(DeviceWait),
    /// The project is going onto a board, one wire step at a time.
    OnDevice(DeviceOpenProgress),
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
    /// The board the open was on when it failed, when it was on one — the
    /// failure page then offers to reset it.
    pub device: Option<OpenDevice>,
}

/// The board an open is aimed at, as the opening frame names it.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenDevice {
    /// The roster device, when the fold has one — what Reset and Connect
    /// aim at. `None` while the registry row is still loading.
    pub id: Option<DeviceId>,
    /// Its registry uid (the `/device/<uid>` address).
    pub uid: String,
    /// Its title, for the sentence.
    pub name: String,
}

/// Why a held open is waiting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceWaitReason {
    /// No port for it in this page. A fresh page in a browser that does not
    /// keep Web Serial grants across reloads (Brave) always starts here, and
    /// only a click can fix it: `requestPort()` needs a user gesture.
    NotConnected,
    /// The port is here but closed.
    PortClosed,
    /// Connected; it has not said hello yet.
    Identifying,
    /// Busy with an activity (a flash, a push).
    Busy,
    /// The registry has not loaded the row yet.
    Unknown,
}

/// A held open, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceWait {
    pub device: OpenDevice,
    pub reason: DeviceWaitReason,
}

/// Where an open onto a board has got.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceOpenProgress {
    pub device: OpenDevice,
    pub step: DeviceOpenStep,
}

/// One wire step of an open onto a board, in the order they happen.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceOpenStep {
    /// Taking the board's wire and asking what it runs.
    Connecting,
    /// Stopping what it was running and clearing the project directory.
    Clearing,
    /// Writing the project's files. Byte counts are the payload's, so the
    /// bar is a real quantity.
    Uploading { sent_bytes: u64, total_bytes: u64 },
    /// `LoadProject`: the board parses the project and compiles every
    /// shader on its own JIT. The long one, and the one with no quantity.
    Loading,
    /// Verifying the hash and reading the project back for the editor.
    Reading,
}

impl DeviceOpenStep {
    /// What the board is doing, as a phrase that also completes "while
    /// …" in a failure.
    pub fn doing(&self) -> &'static str {
        match self {
            Self::Connecting => "connecting to the board",
            Self::Clearing => "clearing the board's old project",
            Self::Uploading { .. } => "sending the project to the board",
            Self::Loading => "loading the project on the board (compiling its shaders)",
            Self::Reading => "reading the project back from the board",
        }
    }
}

thread_local! {
    static STAGE: RefCell<OpenStage> = const { RefCell::new(OpenStage::Idle) };
    /// Bumped by every enqueued open request; the newest value is the
    /// current open.
    static REQUESTED: Cell<u64> = const { Cell::new(0) };
    /// The generation of the open the actor is running right now.
    static RUNNING: Cell<u64> = const { Cell::new(0) };
    /// Bumped by [`cancel_open`] only.
    static CANCEL_EPOCH: Cell<u64> = const { Cell::new(0) };
    /// Requests parked on a cancel that has not come.
    static CANCEL_WAKERS: RefCell<Vec<Waker>> = const { RefCell::new(Vec::new()) };
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
///
/// An open that was mid-step on a board says which step: "the device did
/// not respond within 20.0s" alone does not tell anyone whether the board
/// was receiving files or compiling them.
pub(crate) fn note_open_failed(message: impl Into<String>, retry: UiAction) {
    let message = message.into();
    let (message, device) = match open_stage() {
        OpenStage::OnDevice(progress) => (
            format!(
                "{} stopped while {}: {message}",
                progress.device.name,
                progress.step.doing()
            ),
            Some(progress.device),
        ),
        OpenStage::WaitingForDevice(wait) => (message, Some(wait.device)),
        _ => (message, None),
    };
    set_stage(OpenStage::Failed(OpenFailure {
        message,
        retry,
        device,
    }));
}

/// A wire step of the open in flight, IF it is an open onto a board —
/// called from the client seam every open shares, and a no-op for a sim's
/// (whose stage is [`OpenStage::PreparingProject`], never `OnDevice`).
pub(crate) fn note_deploy_step(step: DeviceOpenStep) {
    if let OpenStage::OnDevice(progress) = open_stage() {
        note_device_step(&progress.device, step);
    }
}

/// The open is held on a board that is not ready.
pub(crate) fn note_waiting_for_device(wait: DeviceWait) {
    if !open_superseded() {
        set_stage(OpenStage::WaitingForDevice(wait));
    }
}

/// The open onto a board has reached `step`.
pub(crate) fn note_device_step(device: &OpenDevice, step: DeviceOpenStep) {
    if !open_superseded() {
        set_stage(OpenStage::OnDevice(DeviceOpenProgress {
            device: device.clone(),
            step,
        }));
    }
}

/// The person gave up on the open in flight (the opening frame's Cancel,
/// or its Reset). Supersedes it like a newer click would, clears whatever
/// it was showing, and wakes the device request it is parked on.
pub fn cancel_open() {
    note_open_requested();
    set_stage(OpenStage::Idle);
    CANCEL_EPOCH.with(|epoch| epoch.set(epoch.get().wrapping_add(1)));
    let wakers = CANCEL_WAKERS.with(|wakers| core::mem::take(&mut *wakers.borrow_mut()));
    for waker in wakers {
        waker.wake();
    }
}

/// The cancel counter now. Capture it when a request starts and hand it to
/// [`cancelled_since`].
pub fn cancel_epoch() -> u64 {
    CANCEL_EPOCH.with(Cell::get)
}

/// A future that resolves once [`cancel_open`] runs after `epoch` was
/// read — raced against a device request's deadline so a cancelled open
/// lets go of its wire now rather than when the deadline expires.
pub fn cancelled_since(epoch: u64) -> impl Future<Output = ()> {
    CancelledSince { epoch }
}

struct CancelledSince {
    epoch: u64,
}

impl Future for CancelledSince {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if cancel_epoch() != self.epoch {
            return Poll::Ready(());
        }
        CANCEL_WAKERS.with(|wakers| {
            let mut wakers = wakers.borrow_mut();
            if !wakers.iter().any(|waker| waker.will_wake(cx.waker())) {
                wakers.push(cx.waker().clone());
            }
        });
        Poll::Pending
    }
}

fn set_stage(next: OpenStage) {
    STAGE.with(|stage| *stage.borrow_mut() = next);
}

/// Forget everything (test-only): the signals are per-thread, and a test
/// that leaves a failure standing would colour the next one.
#[cfg(test)]
pub(crate) fn reset_for_test() {
    set_stage(OpenStage::Idle);
    CANCEL_WAKERS.with(|wakers| wakers.borrow_mut().clear());
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
                prefer: None,
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
    fn a_failure_on_a_board_names_the_step_it_was_waiting_on() {
        reset_for_test();
        note_open_requested();
        note_open_started();
        note_device_step(&board(), DeviceOpenStep::Connecting);
        note_deploy_step(DeviceOpenStep::Loading);
        note_open_failed("device did not respond within 20.0s", open_action("prjx"));
        let OpenStage::Failed(failure) = open_stage() else {
            panic!("failed stage expected");
        };
        assert_eq!(
            failure.message,
            "Choker stopped while loading the project on the board (compiling its shaders): \
             device did not respond within 20.0s"
        );
        assert_eq!(failure.device, Some(board()), "the page offers to reset it");
    }

    #[test]
    fn a_sim_open_never_narrates_board_steps() {
        reset_for_test();
        note_open_requested();
        note_open_started();
        note_preparing_project();
        note_deploy_step(DeviceOpenStep::Uploading {
            sent_bytes: 1,
            total_bytes: 2,
        });
        assert_eq!(open_stage(), OpenStage::PreparingProject);
    }

    #[test]
    fn cancel_supersedes_the_open_and_wakes_its_parked_request() {
        reset_for_test();
        note_open_requested();
        note_open_started();
        note_device_step(&board(), DeviceOpenStep::Loading);

        let epoch = cancel_epoch();
        let woke = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let waker = Waker::from(std::sync::Arc::new(Flag(std::sync::Arc::clone(&woke))));
        let mut cx = Context::from_waker(&waker);
        let mut parked = core::pin::pin!(cancelled_since(epoch));
        assert!(parked.as_mut().poll(&mut cx).is_pending());

        cancel_open();
        assert!(woke.load(std::sync::atomic::Ordering::SeqCst), "the waker fired");
        assert!(parked.as_mut().poll(&mut cx).is_ready());
        assert!(open_superseded(), "the running open unwinds quietly");
        assert_eq!(open_stage(), OpenStage::Idle, "and the frame stops narrating it");
        // A request that starts after the cancel is not cancelled by it.
        let mut fresh = core::pin::pin!(cancelled_since(cancel_epoch()));
        assert!(fresh.as_mut().poll(&mut cx).is_pending());
    }

    #[test]
    fn a_failure_carries_its_own_retry_and_a_new_open_clears_it() {
        reset_for_test();
        note_open_requested();
        note_open_started();
        note_open_failed("the device did not start", open_action("prjx"));
        let OpenStage::Failed(failure) = open_stage() else {
            panic!("failed stage expected");
        };
        assert_eq!(failure.message, "the device did not start");
        assert_eq!(failure.retry, open_action("prjx"));

        // The REQUEST clears it, not the start: the action can sit in the
        // queue, and a stale error must not colour the new click's route.
        note_open_requested();
        assert_eq!(open_stage(), OpenStage::Idle, "Retry clears the error");
        note_open_started();
        assert_eq!(open_stage(), OpenStage::Starting);
    }

    fn board() -> OpenDevice {
        OpenDevice {
            id: Some(DeviceId(7)),
            uid: "devchoker".to_string(),
            name: "Choker".to_string(),
        }
    }

    struct Flag(std::sync::Arc<std::sync::atomic::AtomicBool>);

    impl std::task::Wake for Flag {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
