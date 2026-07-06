//! Single-consumer, `?Send` async channels for the studio actor.
//!
//! The actor is single-threaded (`ClientIo` is `?Send`; on wasm it runs under
//! `spawn_local`), so these channels are built on `Rc<RefCell<..>>` rather than
//! a `Send` primitive — no atomics, no runtime dependency, and they compile on
//! `wasm32-unknown-unknown`. Each channel is an ordered queue plus a single
//! registered waker; sending pushes and wakes, receiving awaits.
//!
//! Two typed channels are exposed:
//!
//! - the **command channel** ([`command_channel`]) feeds `StudioCommand`s to the
//!   actor. `RefreshTick`s are coalescable, so its receiver has a
//!   [`CommandReceiver::recv_coalesced`] that drains everything currently queued
//!   and returns the batch, letting the actor drop redundant ticks and run
//!   pending actions first;
//! - the **view channel** ([`studio_view_channel`]) pushes change-gated
//!   `UiStudioView` snapshots back to the UI. It keeps only the latest snapshot
//!   (a slow reader coalesces to the newest view), matching a Dioxus
//!   `Signal<UiStudioView>`.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::UiStudioView;
use crate::app::studio::studio_command::StudioCommand;

struct QueueInner<T> {
    items: VecDeque<T>,
    waker: Option<Waker>,
    senders: usize,
}

impl<T> QueueInner<T> {
    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

/// A sender half of the command channel.
///
/// Cloneable: the web shell holds one for user actions and one for the refresh
/// timer. Dropping the last sender lets the receiver observe channel closure.
pub struct CommandSender {
    inner: Rc<RefCell<QueueInner<StudioCommand>>>,
}

impl Clone for CommandSender {
    fn clone(&self) -> Self {
        self.inner.borrow_mut().senders += 1;
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl Drop for CommandSender {
    fn drop(&mut self) {
        let mut inner = self.inner.borrow_mut();
        inner.senders = inner.senders.saturating_sub(1);
        if inner.senders == 0 {
            inner.wake();
        }
    }
}

impl CommandSender {
    /// Enqueue a command and wake the actor.
    pub fn send(&self, command: StudioCommand) {
        let mut inner = self.inner.borrow_mut();
        inner.items.push_back(command);
        inner.wake();
    }
}

/// The receiver half of the command channel, owned by the actor.
pub struct CommandReceiver {
    inner: Rc<RefCell<QueueInner<StudioCommand>>>,
}

impl CommandReceiver {
    /// Await at least one queued command, then drain **everything** currently
    /// queued and return the batch (oldest first). Returns `None` only when all
    /// senders have dropped and the queue is empty (channel closed).
    ///
    /// Draining the whole batch is what makes tick coalescing possible: the
    /// actor sees all pending commands at once and can drop redundant
    /// `RefreshTick`s and order actions ahead of ticks.
    pub async fn recv_coalesced(&mut self) -> Option<Vec<StudioCommand>> {
        RecvCoalesced { inner: &self.inner }.await
    }

    /// Whether any currently-queued command satisfies `predicate`, without
    /// consuming anything. Used by the actor to spot a preempting command while
    /// a pull is in flight.
    pub fn peek_any(&self, predicate: impl Fn(&StudioCommand) -> bool) -> bool {
        self.inner.borrow().items.iter().any(predicate)
    }

    /// Register `waker` to be woken on the next send. Used by the actor's
    /// preempt watcher so a command arriving mid-pull wakes it.
    pub fn register_waker(&self, waker: &Waker) {
        self.inner.borrow_mut().waker = Some(waker.clone());
    }
}

struct RecvCoalesced<'a> {
    inner: &'a Rc<RefCell<QueueInner<StudioCommand>>>,
}

impl Future for RecvCoalesced<'_> {
    type Output = Option<Vec<StudioCommand>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut inner = self.inner.borrow_mut();
        if !inner.items.is_empty() {
            let batch = inner.items.drain(..).collect();
            return Poll::Ready(Some(batch));
        }
        if inner.senders == 0 {
            return Poll::Ready(None);
        }
        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

/// Build a command channel, returning the first sender and the receiver.
pub fn command_channel() -> (CommandSender, CommandReceiver) {
    let inner = Rc::new(RefCell::new(QueueInner {
        items: VecDeque::new(),
        waker: None,
        senders: 1,
    }));
    (
        CommandSender {
            inner: Rc::clone(&inner),
        },
        CommandReceiver { inner },
    )
}

struct ViewInner {
    latest: Option<UiStudioView>,
    waker: Option<Waker>,
    sender_open: bool,
}

/// Pushes change-gated `UiStudioView` snapshots to the UI.
///
/// Only the latest snapshot is retained: a UI that reads slower than the actor
/// emits simply sees the newest view, never a backlog. Owned by the actor.
pub struct StudioViewSender {
    inner: Rc<RefCell<ViewInner>>,
}

impl StudioViewSender {
    /// Publish a snapshot, replacing any unread one, and wake the reader.
    pub fn send(&self, view: UiStudioView) {
        publish(&self.inner, view);
    }

    /// A cloneable publish handle that can push snapshots but does **not** close
    /// the channel when dropped (only the owning [`StudioViewSender`] does).
    ///
    /// The actor hands one to the `UxUpdateSink` for a dispatched action so
    /// progressive `UxUpdate::View` snapshots during a long op still reach the
    /// UI, without the sink's `'static` closure taking ownership of the sender.
    pub fn publisher(&self) -> ViewPublisher {
        ViewPublisher {
            inner: Rc::clone(&self.inner),
        }
    }
}

/// A non-closing, cloneable handle for publishing view snapshots (see
/// [`StudioViewSender::publisher`]).
#[derive(Clone)]
pub struct ViewPublisher {
    inner: Rc<RefCell<ViewInner>>,
}

impl ViewPublisher {
    /// Publish a snapshot, replacing any unread one, and wake the reader.
    pub fn send(&self, view: UiStudioView) {
        publish(&self.inner, view);
    }
}

fn publish(inner: &Rc<RefCell<ViewInner>>, view: UiStudioView) {
    let mut inner = inner.borrow_mut();
    inner.latest = Some(view);
    if let Some(waker) = inner.waker.take() {
        waker.wake();
    }
}

impl Drop for StudioViewSender {
    fn drop(&mut self) {
        let mut inner = self.inner.borrow_mut();
        inner.sender_open = false;
        if let Some(waker) = inner.waker.take() {
            waker.wake();
        }
    }
}

/// The UI-side half of the view channel.
///
/// The web crate drives a Dioxus `Signal<UiStudioView>` from this: await
/// [`Self::recv`] and write each snapshot into the signal.
pub struct StudioViewReceiver {
    inner: Rc<RefCell<ViewInner>>,
}

impl StudioViewReceiver {
    /// Await the next published snapshot. Returns `None` when the actor's sender
    /// has dropped and no snapshot remains.
    pub async fn recv(&mut self) -> Option<UiStudioView> {
        RecvView { inner: &self.inner }.await
    }

    /// Take the currently-published snapshot without awaiting, if any.
    pub fn try_recv(&mut self) -> Option<UiStudioView> {
        self.inner.borrow_mut().latest.take()
    }
}

struct RecvView<'a> {
    inner: &'a Rc<RefCell<ViewInner>>,
}

impl Future for RecvView<'_> {
    type Output = Option<UiStudioView>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut inner = self.inner.borrow_mut();
        if let Some(view) = inner.latest.take() {
            return Poll::Ready(Some(view));
        }
        if !inner.sender_open {
            return Poll::Ready(None);
        }
        inner.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

/// Build a view channel, returning the actor-side sender and the UI-side
/// receiver.
pub fn studio_view_channel() -> (StudioViewSender, StudioViewReceiver) {
    let inner = Rc::new(RefCell::new(ViewInner {
        latest: None,
        waker: None,
        sender_open: true,
    }));
    (
        StudioViewSender {
            inner: Rc::clone(&inner),
        },
        StudioViewReceiver { inner },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_channel_keeps_only_latest_snapshot() {
        let (tx, mut rx) = studio_view_channel();
        tx.send(UiStudioView::empty());
        tx.send(UiStudioView::empty());

        // Two sends, one retained (the newest), then empty.
        assert!(rx.try_recv().is_some());
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn command_channel_drains_in_order() {
        let (tx, mut rx) = command_channel();
        tx.send(StudioCommand::RefreshTick);
        tx.send(StudioCommand::Shutdown);

        // recv_coalesced resolves synchronously here because the queue is
        // non-empty; drive it with a noop waker.
        let batch = crate::app::studio::studio_actor::poll_now(rx.recv_coalesced())
            .expect("ready")
            .expect("open");
        assert_eq!(batch.len(), 2);
        assert!(batch[0].is_refresh_tick());
        assert!(matches!(batch[1], StudioCommand::Shutdown));
    }
}
