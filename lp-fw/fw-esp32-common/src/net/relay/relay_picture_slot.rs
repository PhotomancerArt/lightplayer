//! One picture between the main thread (which makes it, from the server)
//! and the relay (which sends it on the device leg): relay protocol 2's
//! hand-off, a pointer moved under a short critical section.
//!
//! Who calls what:
//!
//! - **The relay** (on `lp-net` on the C6; the harness's relay thread): the
//!   client's `TakePicture` is [`RelayPictureSlot::ask`]; each pass it
//!   turns [`RelayPictureSlot::news`] into `RelayEvent::PictureReady`; on
//!   `SendPicture` it [`take_ready`](RelayPictureSlot::take_ready)s the
//!   frame, sends it, and [`give_back`](RelayPictureSlot::give_back)s the
//!   buffer; on `DropPicture` it takes and gives back without sending.
//!   When its leg ends, or the board may no longer dial, it
//!   [`release`](RelayPictureSlot::release)s every buffer.
//! - **The main thread** (the frame hook, between ticks): when
//!   [`wanted`](RelayPictureSlot::wanted) (an atomic load, no critical
//!   section on the frame path), it takes the
//!   [`spare`](RelayPictureSlot::take_spare) buffer (or reserves one,
//!   fallibly), writes the whole `Picture` frame into it **outside** any
//!   critical section, and hands it over with
//!   [`put_ready`](RelayPictureSlot::put_ready) (the caller then wakes the
//!   relay).
//!
//! One buffer goes round in the steady state, so a picture costs no
//! allocation. A picture made after a release (the leg went while the main
//! thread was filling) is dropped at `put_ready`, so a board that is not
//! on the relay holds no picture buffer.

use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

use critical_section::Mutex;

/// See the module doc.
pub struct RelayPictureSlot {
    /// Set by the relay when the client asks; read by the main thread every
    /// frame. Cleared, inside the critical section, by `put_ready` and
    /// `release`.
    asked: AtomicBool,
    inner: Mutex<RefCell<SlotInner>>,
}

struct SlotInner {
    /// The frame the main thread finished, not yet taken by the relay.
    ready: Option<Vec<u8>>,
    /// Whether `ready` has been announced to the relay as `PictureReady`.
    announced: bool,
    /// The buffer back from the last send, for the next picture.
    spare: Option<Vec<u8>>,
}

impl RelayPictureSlot {
    /// An empty slot: nothing asked, no buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            asked: AtomicBool::new(false),
            inner: Mutex::new(RefCell::new(SlotInner {
                ready: None,
                announced: false,
                spare: None,
            })),
        }
    }

    // --- the relay's side -------------------------------------------------

    /// The client asked for a picture (`TakePicture`). Asking again before
    /// the last one was made is fine: one picture answers both.
    pub fn ask(&self) {
        self.asked.store(true, Ordering::Release);
    }

    /// A ready picture not yet announced: `true` once per picture (it is
    /// marked announced).
    pub fn news(&self) -> bool {
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow_ref_mut(cs);
            let news = inner.ready.is_some() && !inner.announced;
            inner.announced |= news;
            news
        })
    }

    /// The ready picture's frame, to send or drop.
    pub fn take_ready(&self) -> Option<Vec<u8>> {
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow_ref_mut(cs);
            inner.announced = false;
            inner.ready.take()
        })
    }

    /// A buffer back from a send (or a drop): cleared and kept for the next
    /// picture. A second one (a picture made while another was in flight)
    /// is dropped.
    pub fn give_back(&self, mut buf: Vec<u8>) {
        buf.clear();
        let extra = critical_section::with(|cs| {
            let mut inner = self.inner.borrow_ref_mut(cs);
            if inner.spare.is_none() {
                inner.spare = Some(buf);
                None
            } else {
                Some(buf)
            }
        });
        drop(extra);
    }

    /// Drop every buffer and forget the ask: the leg ended, or the board
    /// may not dial. The buffers are freed outside the critical section.
    pub fn release(&self) {
        let held = critical_section::with(|cs| {
            self.asked.store(false, Ordering::Release);
            let mut inner = self.inner.borrow_ref_mut(cs);
            inner.announced = false;
            (inner.ready.take(), inner.spare.take())
        });
        drop(held);
    }

    // --- the main thread's side -------------------------------------------

    /// Whether the relay wants a picture. One atomic load: cheap on every
    /// frame.
    #[must_use]
    pub fn wanted(&self) -> bool {
        self.asked.load(Ordering::Acquire)
    }

    /// The spare buffer, if the last picture's came back.
    pub fn take_spare(&self) -> Option<Vec<u8>> {
        critical_section::with(|cs| self.inner.borrow_ref_mut(cs).spare.take())
    }

    /// The picture `buf` is made: it is the ready one now, and the ask is
    /// answered. Dropped instead when the slot was released while it was
    /// being made (nothing is asked any more). A ready picture it replaces
    /// (never announced, or announced and not yet taken) becomes the spare.
    /// The caller wakes the relay.
    pub fn put_ready(&self, buf: Vec<u8>) {
        let unused = critical_section::with(|cs| {
            if !self.asked.swap(false, Ordering::AcqRel) {
                return Some(buf);
            }
            let mut inner = self.inner.borrow_ref_mut(cs);
            inner.announced = false;
            let replaced = inner.ready.replace(buf);
            match replaced {
                Some(mut old) if inner.spare.is_none() => {
                    old.clear();
                    inner.spare = Some(old);
                    None
                }
                other => other,
            }
        });
        drop(unused);
    }

    /// Whether the slot holds any buffer (ready or spare), for tests.
    #[cfg(test)]
    pub(crate) fn holds_a_buffer(&self) -> bool {
        critical_section::with(|cs| {
            let inner = self.inner.borrow_ref(cs);
            inner.ready.is_some() || inner.spare.is_some()
        })
    }
}

impl Default for RelayPictureSlot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_buffer_goes_round_with_no_allocation() {
        let slot = RelayPictureSlot::new();
        assert!(!slot.wanted());
        assert!(!slot.news());

        slot.ask();
        assert!(slot.wanted());
        assert!(
            slot.take_spare().is_none(),
            "the first picture reserves one"
        );
        let mut buf = Vec::with_capacity(836);
        buf.extend_from_slice(&[0x0b, 0, 0, 0]);
        slot.put_ready(buf);
        assert!(!slot.wanted(), "answered");
        assert!(slot.news(), "announced once");
        assert!(!slot.news());

        let frame = slot.take_ready().expect("the ready picture");
        assert_eq!(frame, [0x0b, 0, 0, 0]);
        let pointer = frame.as_ptr();
        slot.give_back(frame);

        slot.ask();
        let spare = slot.take_spare().expect("the buffer came back");
        assert_eq!(spare.as_ptr(), pointer, "the same buffer, no allocation");
        assert!(spare.is_empty());
        assert!(spare.capacity() >= 836);
    }

    #[test]
    fn release_drops_both_and_forgets_the_ask() {
        let slot = RelayPictureSlot::new();
        slot.ask();
        slot.put_ready(alloc::vec![1]);
        slot.give_back(alloc::vec![2]);
        slot.ask();
        assert!(slot.holds_a_buffer());
        slot.release();
        assert!(!slot.holds_a_buffer());
        assert!(!slot.wanted());
        assert!(!slot.news());
        assert!(slot.take_ready().is_none());
    }

    #[test]
    fn a_picture_made_after_a_release_is_dropped() {
        let slot = RelayPictureSlot::new();
        slot.ask();
        // The main thread saw `wanted`, and fills a buffer...
        let buf = alloc::vec![0x0b, 0, 0, 0];
        // ...while the leg ends.
        slot.release();
        slot.put_ready(buf);
        assert!(!slot.holds_a_buffer(), "a board off the relay holds none");
        assert!(!slot.news());
    }

    #[test]
    fn a_newer_picture_replaces_an_unsent_one_and_keeps_its_buffer() {
        let slot = RelayPictureSlot::new();
        slot.ask();
        slot.put_ready(alloc::vec![1]);
        assert!(slot.news());
        // Asked again before the relay took it (the edge was slow).
        slot.ask();
        slot.put_ready(alloc::vec![2]);
        assert!(slot.news(), "the newer one is news");
        assert_eq!(slot.take_ready(), Some(alloc::vec![2]));
        assert_eq!(slot.take_spare(), Some(alloc::vec![]), "the old buffer");
    }
}
