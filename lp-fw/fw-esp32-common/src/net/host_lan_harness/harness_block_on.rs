//! The harness's executor: a null-waker `block_on` that polls, sleeps a
//! moment, and polls again — the edge-and-test-only loop AGENTS.md's sans-IO
//! rule allows ("tests count as edges") — and the two clocks it waits on.
//!
//! Nothing in the code under test registers a waker the harness could use
//! (embassy signals, a non-blocking socket), so a waking executor would buy
//! nothing: every future is simply polled again after [`POLL_EVERY`].

extern crate std;

use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use std::time::Duration;

use embedded_hal_async::delay::DelayNs;

use crate::radio_link::now_us;

/// How long `block_on` sleeps between two polls of a pending future.
const POLL_EVERY: Duration = Duration::from_micros(200);

/// Run `future` to completion on this thread.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return out;
        }
        std::thread::sleep(POLL_EVERY);
    }
}

/// Resolves once the device clock ([`now_us`], the one the links run on)
/// reaches `at` µs, or at once when `stop` says the harness is stopping.
pub async fn until_micros(at: u64, stop: &dyn Fn() -> bool) {
    core::future::poll_fn(|_| {
        if now_us() >= at || stop() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

/// The mux's deadline clock: real time, on the device clock.
pub struct StdDelay;

impl DelayNs for StdDelay {
    async fn delay_ns(&mut self, ns: u32) {
        let at = now_us() + u64::from(ns).div_ceil(1_000);
        until_micros(at, &|| false).await;
    }
}
