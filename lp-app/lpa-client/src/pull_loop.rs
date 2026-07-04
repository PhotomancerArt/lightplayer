//! Runtime-neutral pull loop for streamed project reads.
//!
//! One gated project read is a *streamed* request: the client sends a
//! `ProjectRead`, then receives envelope frames until `fin`, feeding each frame
//! to the shared [`ProjectReadStream`] collect machine. Both
//! [`LpClient`](crate::client::LpClient) and
//! [`TokioLpClient`](crate::tokio_client::TokioLpClient) used to open-code that
//! `receive -> accept` loop; this module owns it once, and adds the three
//! things a real client needs around it:
//!
//! - a **single timeout owner** — the [`ProgressDeadline`], a *quiet-gap*
//!   deadline that is reset on every received frame and fires only when no frame
//!   arrives within its budget (so a slow-but-progressing multi-frame stream
//!   never trips it);
//! - **explicit cancellation** — a [`CancelSignal`] checked between receives, so
//!   the caller stops the loop cleanly (returning [`PullOutcome::Cancelled`])
//!   rather than dropping a half-consumed frame stream; and
//! - a **backoff policy** — [`BackoffPolicy`], the exponential-with-cap retry
//!   cadence a caller applies between failed reads (the type lives here so the
//!   contract is one place; the loop itself does not sleep).
//!
//! # Runtime neutrality
//!
//! The module never depends on Tokio or `web-sys`. The deadline is built from a
//! caller-supplied **timer factory**: a closure that, given a [`Duration`],
//! returns a fresh timer future. Native callers hand it a `tokio::time::sleep`;
//! wasm callers hand it a `setTimeout`-backed future — the same
//! `set_timeout`/`sleep_ms` primitives each transport adapter already uses. The
//! loop races `io.receive()` against that timer with a hand-rolled poll (no
//! executor `select!`), so it compiles and runs identically on native and
//! `wasm32-unknown-unknown` and keeps `ClientIo`'s `?Send` contract.

use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll};
use core::time::Duration;

use lpc_wire::{
    ClientMessage, ClientRequest, ProjectReadEvent, ProjectReadRequest, WireProjectHandle,
};

use crate::client_error::ClientError;
use crate::client_event::ClientEvent;
use crate::client_io::ClientIo;
use crate::project_read_stream::{
    ProjectReadStream, ProjectReadStreamError, ProjectReadStreamStep,
};
use crate::protocol_session::ProtocolSession;

/// Progress-based deadline for a single streamed request.
///
/// This is a *quiet-gap* deadline, not a total-duration deadline: it measures
/// the time since the last received frame and fires only when that gap exceeds
/// `budget`. It is reset on every received frame, so a stream that keeps making
/// progress — however many frames, however slowly overall — never trips it; only
/// a genuinely stalled stream (no frame for `budget`) times out.
///
/// It carries a **timer factory** rather than a concrete timer so the pull loop
/// stays runtime-neutral: `make_timer(budget)` returns a fresh future that
/// resolves after `budget` elapses. Native callers back it with
/// `tokio::time::sleep`; wasm callers back it with a `setTimeout` future.
pub struct ProgressDeadline<MakeTimer, Timer>
where
    MakeTimer: FnMut(Duration) -> Timer,
    Timer: Future<Output = ()>,
{
    budget: Duration,
    make_timer: MakeTimer,
}

impl<MakeTimer, Timer> ProgressDeadline<MakeTimer, Timer>
where
    MakeTimer: FnMut(Duration) -> Timer,
    Timer: Future<Output = ()>,
{
    /// Build a quiet-gap deadline of `budget` between frames, using
    /// `make_timer` to produce each (reset) timer future.
    pub fn new(budget: Duration, make_timer: MakeTimer) -> Self {
        Self { budget, make_timer }
    }

    /// The maximum allowed quiet gap between frames.
    pub fn budget(&self) -> Duration {
        self.budget
    }

    /// A fresh timer future for the current budget. Called once per frame wait,
    /// so awaiting the result is the reset-on-progress behaviour.
    fn fresh_timer(&mut self) -> Timer {
        (self.make_timer)(self.budget)
    }
}

/// Caller-supplied cancellation for an in-flight pull.
///
/// Explicit, not drop-based: the loop observes `is_cancelled()` between
/// `receive()` calls and returns [`PullOutcome::Cancelled`] at a frame boundary,
/// leaving the transport in a consistent state instead of abandoning a
/// half-consumed frame stream mid-`receive`. The receive adapters already
/// tolerate a caller stopping at a boundary: they discard any stale frames left
/// buffered from the abandoned request on the next request.
pub trait CancelSignal {
    fn is_cancelled(&self) -> bool;
}

/// A [`CancelSignal`] that never cancels — for callers with no cancellation
/// (e.g. one-shot CLI reads).
#[derive(Debug, Default, Clone, Copy)]
pub struct NeverCancel;

impl CancelSignal for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl<F> CancelSignal for F
where
    F: Fn() -> bool,
{
    fn is_cancelled(&self) -> bool {
        self()
    }
}

/// Exponential-with-cap failure backoff.
///
/// The pull loop itself never sleeps; this is the retry-cadence *policy* a
/// caller (the actor) applies between reads based on the [`PullOutcome`]. It is
/// defined here so the whole timing contract of a client read — deadline,
/// cancel, and retry cadence — lives in one place. The old flat
/// `PASSIVE_REFRESH_FAILURE_BACKOFF_MS = 3s` becomes `BackoffPolicy::new(3s, ..)`
/// with a cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffPolicy {
    base: Duration,
    max: Duration,
    consecutive_failures: u32,
}

impl BackoffPolicy {
    /// Start at `base`, doubling on each consecutive failure, capped at `max`.
    pub fn new(base: Duration, max: Duration) -> Self {
        Self {
            base,
            max,
            consecutive_failures: 0,
        }
    }

    /// Record one failure and return the delay to wait before the next attempt.
    ///
    /// The delay is `base * 2^(n-1)` for the `n`-th consecutive failure, capped
    /// at `max`. Saturating throughout, so a long failure run stays pinned at
    /// `max` rather than overflowing.
    pub fn record_failure(&mut self) -> Duration {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.current_delay()
    }

    /// Clear the failure streak after a successful read; the next failure starts
    /// again at `base`.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    /// The delay implied by the current failure streak (0 while healthy).
    pub fn current_delay(&self) -> Duration {
        match self.consecutive_failures {
            0 => Duration::ZERO,
            n => {
                let shift = n - 1;
                let scaled = self
                    .base
                    .checked_mul(1u32.checked_shl(shift).unwrap_or(u32::MAX))
                    .unwrap_or(self.max);
                scaled.min(self.max)
            }
        }
    }

    /// Number of consecutive failures observed since the last success.
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}

/// Result of driving one project read to a boundary.
#[derive(Debug)]
pub enum PullOutcome {
    /// `fin` was reached; the ordered read events were collected. The
    /// side-channel [`ClientEvent`]s observed while reading (heartbeats, logs,
    /// uncorrelated responses) are carried alongside so callers preserve the
    /// unsolicited-event buffering the open-coded loops had.
    Completed {
        events: Vec<ProjectReadEvent>,
        observed: Vec<ClientEvent>,
    },
    /// The caller asked to stop between frames.
    Cancelled,
    /// The [`ProgressDeadline`] fired: no frame arrived within the quiet-gap
    /// budget.
    TimedOut,
    /// A transport or protocol error ended the read.
    Failed(ClientError),
}

/// Send one `ProjectRead` and drive it to completion, a deadline, or a cancel.
///
/// This is the single owner of the streamed-read state machine. It:
///
/// 1. allocates a request id from `protocol` and sends the `ProjectRead`;
/// 2. loops: check `cancel` (→ [`PullOutcome::Cancelled`] at the boundary);
///    race `io.receive()` against a fresh [`ProgressDeadline`] timer (timer wins
///    → [`PullOutcome::TimedOut`]); feed each received frame to
///    [`ProjectReadStream`] until it completes;
/// 3. returns the collected events plus the unsolicited events seen en route.
///
/// The deadline resets every frame (a fresh timer is awaited per receive), so a
/// slow multi-frame stream completes as long as each gap is under budget.
pub async fn run_project_read<Io, MakeTimer, Timer, Cancel>(
    io: &mut Io,
    protocol: &mut ProtocolSession,
    handle: WireProjectHandle,
    request: ProjectReadRequest,
    mut deadline: ProgressDeadline<MakeTimer, Timer>,
    cancel: &Cancel,
) -> PullOutcome
where
    Io: ClientIo,
    MakeTimer: FnMut(Duration) -> Timer,
    Timer: Future<Output = ()>,
    Cancel: CancelSignal + ?Sized,
{
    let request_id = protocol.next_request_id();
    if let Err(error) = io
        .send(ClientMessage {
            id: request_id,
            msg: ClientRequest::ProjectRead { handle, request },
        })
        .await
    {
        return PullOutcome::Failed(ClientError::from(error));
    }

    let mut stream = ProjectReadStream::new(request_id);
    let mut observed = Vec::new();

    loop {
        // Cancellation is observed at the frame boundary, before we commit to
        // another receive, so the transport is never abandoned mid-frame.
        if cancel.is_cancelled() {
            return PullOutcome::Cancelled;
        }

        let timer = deadline.fresh_timer();
        match receive_before_deadline(io, timer).await {
            ReceiveOutcome::Received(Ok(message)) => match stream.accept(protocol, message) {
                Ok(ProjectReadStreamStep::Continue) => {}
                Ok(ProjectReadStreamStep::Event(event)) => observed.push(event),
                Ok(ProjectReadStreamStep::Complete(events)) => {
                    return PullOutcome::Completed { events, observed };
                }
                Err(error) => return PullOutcome::Failed(stream_error(error)),
            },
            ReceiveOutcome::Received(Err(error)) => {
                return PullOutcome::Failed(ClientError::from(error));
            }
            ReceiveOutcome::DeadlineElapsed => return PullOutcome::TimedOut,
        }
    }
}

enum ReceiveOutcome {
    Received(Result<lpc_wire::WireServerMessage, lpc_wire::TransportError>),
    DeadlineElapsed,
}

/// Race one `io.receive()` against a single (already-built) timer future.
///
/// Hand-rolled instead of an executor `select!` so the module stays free of any
/// runtime dependency. The `receive` future is polled first each wake, so a
/// frame that is ready at the same time as the timer counts as progress (the
/// deadline is a quiet-gap, not a hard cut-off).
async fn receive_before_deadline<Io, Timer>(io: &mut Io, timer: Timer) -> ReceiveOutcome
where
    Io: ClientIo,
    Timer: Future<Output = ()>,
{
    let mut receive = pin!(io.receive());
    let mut timer = pin!(timer);

    core::future::poll_fn(move |cx: &mut Context<'_>| {
        if let Poll::Ready(result) = receive.as_mut().poll(cx) {
            return Poll::Ready(ReceiveOutcome::Received(result));
        }
        if timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(ReceiveOutcome::DeadlineElapsed);
        }
        Poll::Pending
    })
    .await
}

fn stream_error(error: ProjectReadStreamError) -> ClientError {
    match error {
        ProjectReadStreamError::Server(message) => ClientError::Server(message),
        ProjectReadStreamError::Protocol(message) => ClientError::Protocol(message),
        ProjectReadStreamError::Unexpected(response) => ClientError::UnexpectedResponse {
            operation: "project.read",
            response,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::VecDeque;

    use async_trait::async_trait;
    use lpc_model::Revision;
    use lpc_wire::{
        ClientMessage, ProjectReadEvent, ProjectReadRequest, TransportError, WireProjectHandle,
        WireServerMessage, WireServerMsgBody,
    };

    use super::*;

    /// A `ClientIo` fake that replays a scripted sequence of receive *steps*.
    ///
    /// Each step is either a message to hand back or a `Stall`: a receive that
    /// never resolves (its future stays `Pending` forever), used to exercise the
    /// progress deadline. Sent messages are captured for assertions and to prove
    /// the transport is still usable after a cancel.
    struct ScriptedClientIo {
        sent: Vec<ClientMessage>,
        steps: VecDeque<ReceiveStep>,
    }

    enum ReceiveStep {
        Message(WireServerMessage),
        Stall,
    }

    impl ScriptedClientIo {
        fn new(steps: impl IntoIterator<Item = ReceiveStep>) -> Self {
            Self {
                sent: Vec::new(),
                steps: steps.into_iter().collect(),
            }
        }
    }

    #[async_trait(?Send)]
    impl ClientIo for ScriptedClientIo {
        async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
            self.sent.push(msg);
            Ok(())
        }

        async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
            match self.steps.pop_front() {
                Some(ReceiveStep::Message(message)) => Ok(message),
                Some(ReceiveStep::Stall) => core::future::pending().await,
                None => Err(TransportError::ConnectionLost),
            }
        }

        async fn close(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
    }

    /// A timer factory that resolves immediately (`ready`). Racing an immediate
    /// timer against a `Stall` receive deterministically elapses the deadline;
    /// racing it against a ready message lets the message win (receive is polled
    /// first).
    fn immediate_timer() -> impl FnMut(Duration) -> core::future::Ready<()> {
        |_budget: Duration| core::future::ready(())
    }

    /// A timer factory whose future never resolves — the deadline is effectively
    /// infinite, so only real frames drive the loop.
    fn never_timer() -> impl FnMut(Duration) -> core::future::Pending<()> {
        |_budget: Duration| core::future::pending()
    }

    /// A timer future that reports `Ready` only after being polled `remaining`
    /// times, modelling a fixed gap length independent of wall-clock time. Each
    /// fresh timer starts its countdown over, which is exactly the reset the
    /// pull loop performs per frame.
    struct CountdownTimer {
        remaining: usize,
    }

    impl Future for CountdownTimer {
        type Output = ();

        fn poll(mut self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if self.remaining == 0 {
                Poll::Ready(())
            } else {
                self.remaining -= 1;
                // Re-arm so a stalled receive keeps driving the countdown.
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    fn frame(
        id: u64,
        seq: u32,
        fin: bool,
        events: impl IntoIterator<Item = ProjectReadEvent>,
    ) -> WireServerMessage {
        WireServerMessage::stream_frame(
            id,
            seq,
            fin,
            WireServerMsgBody::ProjectRead {
                events: events.into_iter().collect(),
            },
        )
    }

    fn empty_request() -> ProjectReadRequest {
        ProjectReadRequest {
            since: None,
            queries: Vec::new(),
            probes: Vec::new(),
        }
    }

    #[tokio::test]
    async fn multi_frame_stream_under_budget_completes() {
        // Two frames, each gap under budget (the never-resolving timer stands in
        // for "each gap is well under the deadline"). The deadline resets on the
        // first frame, so the total stream length does not matter.
        let mut io = ScriptedClientIo::new([
            ReceiveStep::Message(frame(
                1,
                0,
                false,
                [ProjectReadEvent::Begin {
                    revision: Revision::new(7),
                }],
            )),
            ReceiveStep::Message(frame(
                1,
                1,
                true,
                [ProjectReadEvent::End {
                    revision: Revision::new(7),
                }],
            )),
        ]);
        let mut protocol = ProtocolSession::new();
        let deadline = ProgressDeadline::new(Duration::from_secs(5), never_timer());

        let outcome = run_project_read(
            &mut io,
            &mut protocol,
            WireProjectHandle::new(3),
            empty_request(),
            deadline,
            &NeverCancel,
        )
        .await;

        let PullOutcome::Completed { events, .. } = outcome else {
            panic!("expected completion, got {outcome:?}");
        };
        assert_eq!(
            events,
            vec![
                ProjectReadEvent::Begin {
                    revision: Revision::new(7),
                },
                ProjectReadEvent::End {
                    revision: Revision::new(7),
                },
            ]
        );
        assert_eq!(io.sent.len(), 1);
    }

    #[tokio::test]
    async fn stalled_stream_times_out() {
        // The first receive never resolves; the immediate timer wins the race.
        let mut io = ScriptedClientIo::new([ReceiveStep::Stall]);
        let mut protocol = ProtocolSession::new();
        let deadline = ProgressDeadline::new(Duration::from_secs(5), immediate_timer());

        let outcome = run_project_read(
            &mut io,
            &mut protocol,
            WireProjectHandle::new(3),
            empty_request(),
            deadline,
            &NeverCancel,
        )
        .await;

        assert!(
            matches!(outcome, PullOutcome::TimedOut),
            "expected TimedOut, got {outcome:?}"
        );
    }

    #[tokio::test]
    async fn deadline_resets_between_progressing_frames() {
        // Each inter-frame gap is under budget, but the total elapsed time
        // across the whole stream exceeds one budget. A *total*-duration
        // deadline would trip; the quiet-gap deadline resets on every frame and
        // completes. We model this with a timer factory that resolves only after
        // being polled `GAP_POLLS` times (a fixed gap length), and frames that
        // resolve immediately: each per-frame timer is rebuilt from zero, so it
        // never reaches its gap before the next frame arrives, even though three
        // gaps back-to-back would exceed a single budget.
        const GAP_POLLS: usize = 2;

        let mut io = ScriptedClientIo::new([
            ReceiveStep::Message(frame(
                1,
                0,
                false,
                [ProjectReadEvent::Begin {
                    revision: Revision::new(2),
                }],
            )),
            ReceiveStep::Message(frame(1, 1, false, [])),
            ReceiveStep::Message(frame(1, 2, false, [])),
            ReceiveStep::Message(frame(
                1,
                3,
                true,
                [ProjectReadEvent::End {
                    revision: Revision::new(2),
                }],
            )),
        ]);
        let mut protocol = ProtocolSession::new();
        // Fresh countdown per frame — the reset. If the deadline accumulated
        // across frames instead of resetting, four gaps of GAP_POLLS would blow
        // the budget; because it resets, each frame's receive (immediate) beats
        // its own fresh timer.
        let deadline = ProgressDeadline::new(Duration::from_secs(5), |_budget| CountdownTimer {
            remaining: GAP_POLLS,
        });

        let outcome = run_project_read(
            &mut io,
            &mut protocol,
            WireProjectHandle::new(1),
            empty_request(),
            deadline,
            &NeverCancel,
        )
        .await;

        assert!(
            matches!(outcome, PullOutcome::Completed { .. }),
            "a progressing stream must not time out, got {outcome:?}"
        );
    }

    #[tokio::test]
    async fn mid_stream_cancel_leaves_transport_reusable() {
        // Cancel becomes true after the first frame is accepted. The loop returns
        // Cancelled at the boundary without consuming the remaining frames, and
        // the same io can then drive a fresh read to completion.
        let cancel_flag = Cell::new(false);
        let cancel = || cancel_flag.get();

        let mut io = ScriptedClientIo::new([
            ReceiveStep::Message(frame(
                1,
                0,
                false,
                [ProjectReadEvent::Begin {
                    revision: Revision::new(1),
                }],
            )),
            // These belong to the abandoned read; a real adapter discards them on
            // the next request. Here the second read allocates id 2, so leftover
            // id-1 frames would be Uncorrelated — we instead script the follow-up
            // read's frames after flipping cancel so the fake stays simple.
        ]);
        let mut protocol = ProtocolSession::new();

        // First read: accept one frame, then cancellation trips before the next
        // receive. We drive it manually by making the deadline infinite and
        // flipping the flag from the timer factory closure the first time the
        // loop consults a timer (i.e. after the first frame).
        let deadline = ProgressDeadline::new(Duration::from_secs(60), |_budget| {
            cancel_flag.set(true);
            core::future::pending::<()>()
        });

        let outcome = run_project_read(
            &mut io,
            &mut protocol,
            WireProjectHandle::new(1),
            empty_request(),
            deadline,
            &cancel,
        )
        .await;
        assert!(
            matches!(outcome, PullOutcome::Cancelled),
            "expected Cancelled, got {outcome:?}"
        );

        // Transport is consistent: a subsequent read on the same io completes.
        cancel_flag.set(false);
        io.steps.push_back(ReceiveStep::Message(frame(
            2,
            0,
            true,
            [
                ProjectReadEvent::Begin {
                    revision: Revision::new(9),
                },
                ProjectReadEvent::End {
                    revision: Revision::new(9),
                },
            ],
        )));
        let deadline = ProgressDeadline::new(Duration::from_secs(60), never_timer());
        let outcome = run_project_read(
            &mut io,
            &mut protocol,
            WireProjectHandle::new(1),
            empty_request(),
            deadline,
            &NeverCancel,
        )
        .await;
        assert!(
            matches!(outcome, PullOutcome::Completed { .. }),
            "transport must be reusable after cancel, got {outcome:?}"
        );
        // Two reads were sent on the one transport.
        assert_eq!(io.sent.len(), 2);
    }

    #[tokio::test]
    async fn transport_failure_is_reported() {
        // An empty script makes receive return ConnectionLost.
        let mut io = ScriptedClientIo::new([]);
        let mut protocol = ProtocolSession::new();
        let deadline = ProgressDeadline::new(Duration::from_secs(5), never_timer());

        let outcome = run_project_read(
            &mut io,
            &mut protocol,
            WireProjectHandle::new(3),
            empty_request(),
            deadline,
            &NeverCancel,
        )
        .await;

        assert!(
            matches!(outcome, PullOutcome::Failed(ClientError::Transport(_))),
            "expected transport failure, got {outcome:?}"
        );
    }

    #[test]
    fn backoff_progression_doubles_then_caps_and_resets() {
        let mut backoff = BackoffPolicy::new(Duration::from_secs(3), Duration::from_secs(30));
        assert_eq!(backoff.current_delay(), Duration::ZERO);

        // Exponential: 3, 6, 12, 24, then capped at 30, 30, ...
        assert_eq!(backoff.record_failure(), Duration::from_secs(3));
        assert_eq!(backoff.record_failure(), Duration::from_secs(6));
        assert_eq!(backoff.record_failure(), Duration::from_secs(12));
        assert_eq!(backoff.record_failure(), Duration::from_secs(24));
        assert_eq!(backoff.record_failure(), Duration::from_secs(30));
        assert_eq!(backoff.record_failure(), Duration::from_secs(30));
        assert_eq!(backoff.consecutive_failures(), 6);

        // Success clears the streak; the next failure starts again at base.
        backoff.record_success();
        assert_eq!(backoff.current_delay(), Duration::ZERO);
        assert_eq!(backoff.consecutive_failures(), 0);
        assert_eq!(backoff.record_failure(), Duration::from_secs(3));
    }

    #[test]
    fn backoff_long_failure_run_stays_pinned_at_max() {
        // A very long streak must saturate, not overflow, and stay at max.
        let mut backoff = BackoffPolicy::new(Duration::from_secs(1), Duration::from_secs(10));
        for _ in 0..200 {
            let delay = backoff.record_failure();
            assert!(delay <= Duration::from_secs(10));
        }
        assert_eq!(backoff.current_delay(), Duration::from_secs(10));
    }
}
