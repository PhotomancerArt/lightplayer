//! Simple no_std compatible channels using alloc

extern crate alloc;

use alloc::{collections::VecDeque, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll, Waker};

/// Simple unbounded sender for no_std environments
pub struct UnboundedSender<T> {
    inner: Arc<ChannelInner<T>>,
}

/// Simple unbounded receiver for no_std environments
pub struct UnboundedReceiver<T> {
    inner: Arc<ChannelInner<T>>,
}

struct ChannelInner<T> {
    queue: spin::Mutex<VecDeque<T>>,
    wakers: spin::Mutex<Vec<Waker>>,
    closed: AtomicBool,
}

impl<T> ChannelInner<T> {
    fn new() -> Self {
        Self {
            queue: spin::Mutex::new(VecDeque::new()),
            wakers: spin::Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
        }
    }
}

impl<T> UnboundedSender<T> {
    pub fn send(&self, item: T) -> Result<(), ()> {
        if self.inner.closed.load(Ordering::Relaxed) {
            return Err(());
        }

        let mut queue = self.inner.queue.lock();
        queue.push_back(item);

        // Wake all waiting receivers
        let mut wakers = self.inner.wakers.lock();
        for waker in wakers.drain(..) {
            waker.wake();
        }

        Ok(())
    }
}

impl<T> UnboundedReceiver<T> {
    pub fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let mut queue = self.inner.queue.lock();
        if let Some(item) = queue.pop_front() {
            return Poll::Ready(Some(item));
        }

        // Queue is empty, check if closed
        if self.inner.closed.load(Ordering::Relaxed) {
            return Poll::Ready(None);
        }

        // No items available, register waker
        let mut wakers = self.inner.wakers.lock();
        wakers.push(cx.waker().clone());
        Poll::Pending
    }

    pub fn close(&self) {
        self.inner.closed.store(true, Ordering::Relaxed);
        let mut wakers = self.inner.wakers.lock();
        for waker in wakers.drain(..) {
            waker.wake();
        }
    }
}

pub fn unbounded<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let inner = Arc::new(ChannelInner::new());
    let sender = UnboundedSender {
        inner: Arc::clone(&inner),
    };
    let receiver = UnboundedReceiver { inner };
    (sender, receiver)
}

// Implement Future for UnboundedReceiver to work with async/await
impl<T> core::future::Future for UnboundedReceiver<T> {
    type Output = Option<T>;

    fn poll(mut self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.poll_next(cx)
    }
}

// Simple oneshot channel
pub mod oneshot {
    use super::*;

    pub struct OneshotSender<T> {
        pub(super) inner: Arc<OneshotInner<T>>,
    }

    pub struct OneshotReceiver<T> {
        pub(super) inner: Arc<OneshotInner<T>>,
    }

    impl<T> OneshotSender<T> {
        pub fn send(self, value: T) -> Result<(), T> {
            if self.inner.closed.load(Ordering::Relaxed) {
                return Err(value);
            }

            let mut inner_value = self.inner.value.lock();
            if inner_value.is_some() {
                return Err(value);
            }

            *inner_value = Some(value);
            self.inner.closed.store(true, Ordering::Relaxed);

            // Wake the receiver
            let mut waker = self.inner.waker.lock();
            if let Some(w) = waker.take() {
                w.wake();
            }

            Ok(())
        }
    }

    impl<T> OneshotReceiver<T> {
        pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Result<T, ()>> {
            if self.inner.closed.load(Ordering::Relaxed) {
                let mut value = self.inner.value.lock();
                if let Some(v) = value.take() {
                    return Poll::Ready(Ok(v));
                }
                return Poll::Ready(Err(()));
            }

            // Register waker
            let mut waker = self.inner.waker.lock();
            *waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }

    impl<T> core::future::Future for OneshotReceiver<T> {
        type Output = Result<T, ()>;

        fn poll(mut self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.inner.closed.load(Ordering::Relaxed) {
                let mut value = self.inner.value.lock();
                if let Some(v) = value.take() {
                    return Poll::Ready(Ok(v));
                }
                return Poll::Ready(Err(()));
            }

            // Register waker
            let mut waker = self.inner.waker.lock();
            *waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

pub use oneshot::{OneshotReceiver, OneshotSender};

struct OneshotInner<T> {
    value: spin::Mutex<Option<T>>,
    waker: spin::Mutex<Option<Waker>>,
    closed: AtomicBool,
}

impl<T> OneshotSender<T> {
    pub fn send(self, value: T) -> Result<(), T> {
        if self.inner.closed.load(Ordering::Relaxed) {
            return Err(value);
        }

        let mut inner_value = self.inner.value.lock();
        if inner_value.is_some() {
            return Err(value);
        }

        *inner_value = Some(value);
        self.inner.closed.store(true, Ordering::Relaxed);

        // Wake the receiver
        let mut waker = self.inner.waker.lock();
        if let Some(w) = waker.take() {
            w.wake();
        }

        Ok(())
    }
}

impl<T> OneshotReceiver<T> {
    pub fn poll(&mut self, cx: &mut Context<'_>) -> Poll<Result<T, ()>> {
        if self.inner.closed.load(Ordering::Relaxed) {
            let mut value = self.inner.value.lock();
            if let Some(v) = value.take() {
                return Poll::Ready(Ok(v));
            }
            return Poll::Ready(Err(()));
        }

        // Register waker
        let mut waker = self.inner.waker.lock();
        *waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

pub fn oneshot<T>() -> (OneshotSender<T>, OneshotReceiver<T>) {
    let inner = Arc::new(OneshotInner {
        value: spin::Mutex::new(None),
        waker: spin::Mutex::new(None),
        closed: AtomicBool::new(false),
    });

    let sender = OneshotSender {
        inner: Arc::clone(&inner),
    };
    let receiver = OneshotReceiver { inner };
    (sender, receiver)
}

// Implement Future for OneshotReceiver
impl<T> core::future::Future for OneshotReceiver<T> {
    type Output = Result<T, ()>;

    fn poll(mut self: core::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.inner.closed.load(Ordering::Relaxed) {
            let mut value = self.inner.value.lock();
            if let Some(v) = value.take() {
                return Poll::Ready(Ok(v));
            }
            return Poll::Ready(Err(()));
        }

        // Register waker
        let mut waker = self.inner.waker.lock();
        *waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
