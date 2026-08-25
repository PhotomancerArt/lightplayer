//! Injected, runtime-neutral timers and per-operation deadlines.
//!
//! `lpa-link` must not depend on a concrete executor (tokio timers on host,
//! gloo on wasm), so a [`DeviceSession`] receives a timer FACTORY at
//! construction — the same pattern as `StudioActor`'s `make_pull_timer`. The
//! owner supplies whatever sleep its platform has; the session only ever
//! awaits the returned futures.
//!
//! [`DeviceSession`]: super::DeviceSession

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::Poll;
use std::time::Duration;

/// A single caller-provided sleep, boxed for storage in `!Send` state.
pub type DeviceTimerFuture = Pin<Box<dyn Future<Output = ()>>>;

/// Budget for opening the device link (connector connect + protocol open +
/// connection handoff).
pub const DEFAULT_CONNECT_DEADLINE: Duration = Duration::from_secs(10);

/// Budget from "link open" to the wire hello. Boot can take seconds: this
/// mirrors the browser serial adapter's 500 × 10 ms readiness poll budget
/// (the fake-device test edge used 3 s; the larger browser budget wins).
pub const DEFAULT_READY_DEADLINE: Duration = Duration::from_secs(5);

/// Maximum quiet gap while waiting for one app-protocol response frame.
///
/// A frame-gap backstop for a DEAD wire, not a bound on any request: real
/// firmware heartbeats every 5 s, and each heartbeat restarts this budget —
/// so a device that drops a response while heartbeating passes it forever
/// (the 2026-08-24 request-idle defect). The per-request bound is
/// [`request_total`](DeviceDeadlines::request_total), enforced by
/// `lpa_client::RequestDeadline`. This one also bounds each receive inside
/// streamed project reads, which the total deadline deliberately skips.
pub const DEFAULT_REQUEST_IDLE_DEADLINE: Duration = Duration::from_secs(10);

/// Total budget for one single-response request: send plus the whole
/// response-correlation wait, unrelated frames included.
///
/// Sized for the interactive Studio path: the slowest legitimate
/// single-response requests are device-side work measured in seconds
/// (`LoadProject` compile, `HashPackage`); file writes chunk at 4 KiB per
/// request, and streamed project reads are bounded per frame by their own
/// quiet-gap deadline instead. Twice the idle backstop; the host CLI's
/// `TokioLpClient` keeps its separate 60 s batch-oriented total.
pub const DEFAULT_REQUEST_TOTAL_DEADLINE: Duration = Duration::from_secs(20);

/// Gap between readiness pump passes (matches the browser adapter's 10 ms
/// poll interval).
pub const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Gap between polls while a request is IN FLIGHT and its response frames
/// are streaming in. Deliberately tighter than
/// [`READINESS_POLL_INTERVAL`]: a multi-frame project read used to pay up
/// to 10 ms of poll latency per frame. Browsers clamp nested `setTimeout`
/// to ~4 ms, so the effective in-stream poll is ~4 ms — still less than
/// half the old per-frame tax, and only while a response is pending
/// (idle sessions never run this loop).
#[cfg_attr(
    not(all(feature = "browser-serial-esp32", target_arch = "wasm32")),
    allow(
        dead_code,
        reason = "consumed only by the wasm browser-wire receive loop"
    )
)]
pub const WIRE_FRAME_POLL_INTERVAL: Duration = Duration::from_millis(2);

/// Per-operation deadlines for one device session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceDeadlines {
    /// Connector connect + protocol open + connection handoff.
    pub connect: Duration,
    /// Boot output → wire hello (expiry ⇒ `Unresponsive`/`Incompatible`).
    pub ready: Duration,
    /// Quiet gap per app-protocol response frame (expiry ⇒ request error).
    pub request_idle: Duration,
    /// Total per single-response request, send included (expiry ⇒ request
    /// error + abandoned id).
    pub request_total: Duration,
}

impl Default for DeviceDeadlines {
    fn default() -> Self {
        Self {
            connect: DEFAULT_CONNECT_DEADLINE,
            ready: DEFAULT_READY_DEADLINE,
            request_idle: DEFAULT_REQUEST_IDLE_DEADLINE,
            request_total: DEFAULT_REQUEST_TOTAL_DEADLINE,
        }
    }
}

/// Timer factory + deadlines, injected at [`DeviceSession::connect`].
///
/// [`DeviceSession::connect`]: super::DeviceSession::connect
#[derive(Clone)]
pub struct DeviceTimers {
    make_timer: Rc<dyn Fn(Duration) -> DeviceTimerFuture>,
    deadlines: DeviceDeadlines,
}

impl DeviceTimers {
    /// Wrap a platform sleep factory (tokio `sleep` on host, gloo
    /// `TimeoutFuture` on wasm, a scripted timer in tests).
    pub fn new(make_timer: impl Fn(Duration) -> DeviceTimerFuture + 'static) -> Self {
        Self {
            make_timer: Rc::new(make_timer),
            deadlines: DeviceDeadlines::default(),
        }
    }

    /// Override the default per-operation deadlines.
    #[must_use]
    pub fn with_deadlines(mut self, deadlines: DeviceDeadlines) -> Self {
        self.deadlines = deadlines;
        self
    }

    pub fn deadlines(&self) -> DeviceDeadlines {
        self.deadlines
    }

    /// One sleep of `duration` from the injected factory.
    pub fn sleep(&self, duration: Duration) -> DeviceTimerFuture {
        (self.make_timer)(duration)
    }

    /// Race `future` against a `budget` sleep: `None` when the budget
    /// expires first. Runtime-neutral (hand-rolled poll, no `select!`).
    pub async fn with_deadline<F: Future>(&self, budget: Duration, future: F) -> Option<F::Output> {
        let mut timer = self.sleep(budget);
        let mut future = Box::pin(future);
        std::future::poll_fn(move |cx| {
            if let Poll::Ready(output) = future.as_mut().poll(cx) {
                return Poll::Ready(Some(output));
            }
            if timer.as_mut().poll(cx).is_ready() {
                return Poll::Ready(None);
            }
            Poll::Pending
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn with_deadline_returns_the_value_when_the_future_wins() {
        let timers = DeviceTimers::new(|duration| Box::pin(tokio::time::sleep(duration)));

        let outcome = timers
            .with_deadline(Duration::from_secs(5), async { 42 })
            .await;

        assert_eq!(outcome, Some(42));
    }

    #[tokio::test]
    async fn with_deadline_returns_none_when_the_budget_expires() {
        let timers = DeviceTimers::new(|duration| Box::pin(tokio::time::sleep(duration)));

        let outcome = timers
            .with_deadline(Duration::from_millis(10), std::future::pending::<()>())
            .await;

        assert_eq!(outcome, None);
    }
}
