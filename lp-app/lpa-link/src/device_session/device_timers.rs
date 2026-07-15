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
/// This is the BACKSTOP, not the mechanism: every request gets a response
/// frame (including handler failures, which answer `ServerMsgBody::Error`),
/// so this only fires when the wire itself died mid-request.
pub const DEFAULT_REQUEST_IDLE_DEADLINE: Duration = Duration::from_secs(10);

/// Gap between readiness pump passes (matches the browser adapter's 10 ms
/// poll interval).
pub const READINESS_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Per-operation deadlines for one device session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceDeadlines {
    /// Connector connect + protocol open + connection handoff.
    pub connect: Duration,
    /// Boot output → wire hello (expiry ⇒ `Unresponsive`/`Incompatible`).
    pub ready: Duration,
    /// Quiet gap per app-protocol response frame (expiry ⇒ request error).
    pub request_idle: Duration,
}

impl Default for DeviceDeadlines {
    fn default() -> Self {
        Self {
            connect: DEFAULT_CONNECT_DEADLINE,
            ready: DEFAULT_READY_DEADLINE,
            request_idle: DEFAULT_REQUEST_IDLE_DEADLINE,
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
