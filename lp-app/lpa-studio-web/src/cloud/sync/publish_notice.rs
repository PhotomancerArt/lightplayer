//! The driver's **conclusions, as a wake-up** — the one seam between the
//! auto-publish driver and a surface that must re-ask the service after a
//! trip lands.
//!
//! [`sync_status`](super::sync_status) is the notebook: it remembers what
//! every trip concluded so a person can read it, and nothing subscribes to
//! it (the `/account` page copies it once a second while somebody is
//! looking). That is the right shape for a diagnostic, and the wrong shape
//! for "the bar's face is stale until you reload": a face may not wait on a
//! poll, and the ledger must not grow a nervous system to give it one
//! (`docs/debt/relationship-face-stale-after-publish.md`).
//!
//! So this module carries the other half — the smallest push there is. The
//! driver records the uid of every trip that concluded `Published` or
//! `Pushed`; a reader parks on [`changed_since`] and is woken exactly when
//! one lands. No timer, no polling, and no Dioxus: the driver is not a
//! component and must never depend on the UI runtime, so the wake-up is a
//! plain [`Waker`] and the reader's own task is what touches signals.
//!
//! Per-tab and never persisted, like the queue and the ledger it sits
//! beside: it describes what THIS tab's driver just did.
//!
//! # Why a generation, not a flag
//!
//! A reader holds the generation it last saw and asks two questions of the
//! board: *has anything happened since* (the wake-up), and *was any of it
//! mine* ([`published_since`]). A bare flag would lose a notice that landed
//! between a wake and its handling, and a per-uid boolean would need
//! clearing — which reader gets to clear it? The counter answers both
//! without either problem: notices are never consumed, and each reader's
//! own watermark decides what is new to it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

/// What the driver has published, per project — the newest notice's
/// generation for each uid, and the tab-wide counter they are drawn from.
///
/// Pure and host-testable: the thread-local below is the only wasm-shaped
/// thing in this module, and it holds one of these.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PublishNotices {
    generation: u64,
    per_project: BTreeMap<String, u64>,
}

impl PublishNotices {
    /// The tab-wide counter. A reader's watermark.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Note that a trip for `uid` concluded published (or pushed), and
    /// answer with the generation it was filed under.
    pub fn record(&mut self, uid: &str) -> u64 {
        self.generation += 1;
        self.per_project.insert(uid.to_string(), self.generation);
        self.generation
    }

    /// Whether `uid` has published since a reader's watermark. A uid the
    /// driver never concluded a publish for is simply absent — not a
    /// failure, and not a notice.
    pub fn published_since(&self, uid: &str, seen: u64) -> bool {
        self.per_project
            .get(uid)
            .is_some_and(|generation| *generation > seen)
    }
}

// ---------------------------------------------------------------------------
// The tab's board. Thread-local like the driver it describes; compiled on
// every target so the pure half stays natively testable, while only the
// wasm driver ever writes it.

#[derive(Default)]
struct NoticeBoard {
    notices: PublishNotices,
    /// Readers parked in [`changed_since`], woken (and forgotten) by the
    /// next [`record`].
    waiting: Vec<Waker>,
}

thread_local! {
    static BOARD: RefCell<NoticeBoard> = RefCell::new(NoticeBoard::default());
}

/// File a published/pushed conclusion for `uid` and wake everyone waiting.
///
/// Called from the driver's trip, off any UI runtime. The wakers are taken
/// out from under the borrow before they are woken: a woken reader may poll
/// straight back into [`changed_since`], and re-entering the `RefCell`
/// would panic.
pub fn record(uid: &str) {
    let waiting = BOARD.with(|board| {
        let mut board = board.borrow_mut();
        board.notices.record(uid);
        std::mem::take(&mut board.waiting)
    });
    for waker in waiting {
        waker.wake();
    }
}

/// The tab's current generation — a reader's opening watermark.
pub fn generation() -> u64 {
    BOARD.with(|board| board.borrow().notices.generation())
}

/// Whether `uid` published since `seen`.
pub fn published_since(uid: &str, seen: u64) -> bool {
    BOARD.with(|board| board.borrow().notices.published_since(uid, seen))
}

/// Park until the board moves past `seen`, then answer with the new
/// generation. Already ahead? Ready immediately — a reader can never park
/// on a notice it has already been given.
pub fn changed_since(seen: u64) -> impl Future<Output = u64> {
    ChangedSince { seen }
}

struct ChangedSince {
    seen: u64,
}

impl Future for ChangedSince {
    type Output = u64;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u64> {
        BOARD.with(|board| {
            let mut board = board.borrow_mut();
            let generation = board.notices.generation();
            if generation > self.seen {
                return Poll::Ready(generation);
            }
            // A spurious re-poll must not grow the list without bound.
            if !board
                .waiting
                .iter()
                .any(|waker| waker.will_wake(cx.waker()))
            {
                board.waiting.push(cx.waker().clone());
            }
            Poll::Pending
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reader's two questions: did anything happen, and was it mine.
    #[test]
    fn a_notice_is_new_only_to_a_reader_behind_it() {
        let mut notices = PublishNotices::default();
        let seen = notices.generation();
        assert!(!notices.published_since("prj1", seen));

        let filed = notices.record("prj1");
        assert!(filed > seen);
        assert_eq!(notices.generation(), filed);
        assert!(notices.published_since("prj1", seen));
        // Same reader, watermark moved on: the notice is no longer new.
        assert!(!notices.published_since("prj1", filed));
    }

    /// Another project's publish moves the tab-wide counter — that is what
    /// wakes readers — but is nobody else's notice.
    #[test]
    fn someone_elses_publish_wakes_but_does_not_claim() {
        let mut notices = PublishNotices::default();
        let seen = notices.generation();
        notices.record("prj2");
        assert!(notices.generation() > seen);
        assert!(!notices.published_since("prj1", seen));
        assert!(notices.published_since("prj2", seen));
    }

    /// A second trip for the same project files a fresh notice: a reader
    /// that handled the first still learns about the second.
    #[test]
    fn a_later_trip_for_the_same_project_is_a_new_notice() {
        let mut notices = PublishNotices::default();
        let first = notices.record("prj1");
        assert!(!notices.published_since("prj1", first));
        let second = notices.record("prj1");
        assert!(second > first);
        assert!(notices.published_since("prj1", first));
    }

    /// The wake path itself, over the tab's board: a reader parks, a trip
    /// concludes, the reader is woken exactly once and finds its notice.
    /// Re-polling while parked must not enlist the same reader twice, and
    /// waking must not happen under the board's borrow.
    #[test]
    fn a_parked_reader_wakes_on_the_next_notice() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::task::Wake;

        struct Counting(AtomicUsize);
        impl Wake for Counting {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let counter = Arc::new(Counting(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&counter));
        let mut cx = Context::from_waker(&waker);

        let seen = generation();
        let mut reader = Box::pin(changed_since(seen));
        assert!(reader.as_mut().poll(&mut cx).is_pending());
        assert!(reader.as_mut().poll(&mut cx).is_pending());

        record("prj1");
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);
        assert!(published_since("prj1", seen));
        match reader.as_mut().poll(&mut cx) {
            Poll::Ready(generation) => assert!(generation > seen),
            Poll::Pending => panic!("a reader behind the board must not park"),
        }
    }
}
