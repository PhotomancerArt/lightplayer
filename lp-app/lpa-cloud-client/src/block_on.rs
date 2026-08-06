//! Driving an immediately-ready future to completion, without a runtime.

use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};

/// How many polls before we conclude the future is waiting on something.
///
/// A future that is genuinely waiting will never be woken here — there is no
/// reactor — so spinning forever would hang the caller. One poll is enough
/// for anything this crate produces over an in-process transport; the
/// headroom covers a chain of `.await`s that each need a poll to settle.
const MAX_POLLS: usize = 1024;

/// Drive a future that does not actually wait for anything.
///
/// This is the null-waker `block_on` AGENTS.md sanctions for tests and for
/// edges driving [`InProcessCloud`](crate::InProcessCloud), whose futures are
/// ready the moment they are polled. It is **not** a runtime: there is no
/// reactor to wake anything, so a future that returns `Poll::Pending`
/// forever panics rather than hangs.
///
/// The real transports (wasm `fetch`, and anything else with IO in it) are
/// driven by their platform's executor, never by this.
///
/// # Panics
///
/// If the future is still pending after [`MAX_POLLS`] polls — which means it
/// is waiting on IO and needs a real executor.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = pin!(future);
    for _ in 0..MAX_POLLS {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
    panic!(
        "block_on: future still pending after {MAX_POLLS} polls — it is waiting on IO and needs a real executor"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    #[test]
    fn runs_a_ready_future() {
        assert_eq!(block_on(async { 6 * 7 }), 42);
    }

    #[test]
    fn runs_a_chain_of_awaits() {
        async fn one() -> u32 {
            1
        }
        assert_eq!(
            block_on(async { one().await + one().await + one().await }),
            3
        );
    }

    /// A future that yields once still completes: the null waker never wakes
    /// it, but the loop polls again anyway.
    #[test]
    fn polls_again_after_a_yield() {
        let polled = Cell::new(0u32);
        let output = block_on(YieldOnce {
            done: false,
            polled: &polled,
        });
        assert_eq!(output, ());
        assert_eq!(polled.get(), 2);
    }

    struct YieldOnce<'a> {
        done: bool,
        polled: &'a Cell<u32>,
    }

    impl Future for YieldOnce<'_> {
        type Output = ();

        fn poll(self: core::pin::Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            let this = self.get_mut();
            this.polled.set(this.polled.get() + 1);
            if this.done {
                Poll::Ready(())
            } else {
                this.done = true;
                Poll::Pending
            }
        }
    }
}
