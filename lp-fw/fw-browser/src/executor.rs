//! Tiny executor for browser runtime futures.
//!
//! The server/transport traits are async, but the wasm export boundary is
//! synchronous, and every future this runtime drives (in-memory transport
//! queues) is **immediately ready by construction**. Per the sans-IO core
//! ADR (`docs/adr/2026-07-06-sans-io-core.md`), that invariant is loud, not
//! spun through: a `Pending` here means someone introduced genuinely async
//! IO into the worker's dispatch path, which needs a real executor decision
//! — panicking immediately beats busy-looping a single-threaded worker
//! forever (the poll loop this replaces could never make progress anyway:
//! nothing else runs to wake the future).

/// Run an immediately-ready firmware future to completion.
///
/// Panics if the future is not ready on the first poll — see module docs.
pub(crate) fn block_on<F: core::future::Future>(future: F) -> F::Output {
    use core::pin::pin;
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    let waker = unsafe {
        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|data| RawWaker::new(data, &VTABLE), |_| {}, |_| {}, |_| {});
        Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE))
    };
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!(
            "fw-browser dispatch future returned Pending; these futures must be \
             immediately ready (sans-IO ADR) — a real async source needs a real executor"
        ),
    }
}
