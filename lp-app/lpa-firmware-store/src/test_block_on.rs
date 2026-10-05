//! Test support: drive an immediately-ready future (AGENTS.md: tests count
//! as edges; a null-waker loop is fine there and nowhere else).

use std::future::Future;
use std::task::{Context, Poll, Waker};

pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("a memory cache / fake fetch future is immediately ready"),
    }
}
