//! Tiny executor for browser runtime futures.
//!
//! The server/transport traits are async, but the wasm export boundary is
//! synchronous today. These futures complete immediately in this runtime, so a
//! no-op waker is enough until the browser target needs genuinely async IO.

/// Run an immediately-ready firmware future to completion.
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
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
}
