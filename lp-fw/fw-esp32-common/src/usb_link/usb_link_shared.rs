//! The one USB host link, as the link task and the server transport share it.
//!
//! Every use of the link goes through [`UsbLinkShared::with_link`]: a
//! synchronous call whose [`RefCell`] borrow is dropped before any `.await`.
//! The one cross-task wake is the doorbell, a [`Signal`] — rung by the
//! transport when it queues a reply and by the log ring when a record lands
//! (see [`super::usb_link_task`]).
//!
//! **Two arrangements, one lock hook.**
//!
//! - *One executor* (the S3; the C6 without `io-thread`): the link task is a
//!   task the main task spawned, so the two users never run at once and the
//!   `RefCell` alone keeps them apart. [`UsbLinkShared::leak`] injects no
//!   lock. A critical-section mutex here would be wrong — it would mask
//!   interrupts for as long as a closure ran, and the RMT refill (the LEDs)
//!   cannot wait.
//! - *Two threads* (the C6's link thread, `fw-esp32c6/src/io_thread.rs`): the
//!   link thread can preempt the main thread inside a borrow, so the chip
//!   injects a [`LinkLock`] ([`UsbLinkShared::leak_locked`]) that every
//!   `with_link` runs inside. The C6's masks only the scheduler's interrupt
//!   priority, so no thread switch lands inside a borrow while the RMT refill
//!   still preempts it. The `RefCell` stays, as the reentrancy check.
//!
//! **Closures under `with_link` stay short.** With a real lock they hold off
//! the scheduler (and everything at its priority) for as long as they run,
//! so nothing copies a large buffer under one. Today's work is bounded: an
//! `on_bytes` of at most 64 B, one frame of at most 256 B cut into the task's
//! own buffer, an event pop, a reply queued by `send_external` without a copy
//! (the link reads it from the static frame buffer later, a frame at a time),
//! and a log pump of at most four records. Keep it that way.

use alloc::boxed::Box;
use core::cell::RefCell;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use lp_link::{CH_UPDATE, Link, LinkConfig, LinkState, SelectiveRepeat, SendError};

use super::usb_update_channel::UpdateSend;

/// The largest message the board sends or takes: the static frame buffer's
/// size, which bounds every reply it can serialize (the 16 KiB `ProjectRead`
/// frame budget plus its serial margin), and the wire's budget for a request.
#[cfg(feature = "server")]
pub const MAX_MESSAGE: usize = crate::serial::server_msg::SERVER_MSG_JSON_BUFFER_SIZE;
#[cfg(not(feature = "server"))]
pub const MAX_MESSAGE: usize = lp_link::MAX_MESSAGE;

/// Log records the link may hold queued at once. Each slot is a whole frame
/// payload (256 B) allocated for the link's life; the log ring (4 KB) holds
/// the rest, and the link task moves at most
/// [`super::usb_link_task`]'s per-pass count into it.
const LOG_DATAGRAMS: usize = 4;

/// Messages the send ring may hold queued. Replies go as external messages
/// (one at a time, from the frame buffer); the ring carries only the small
/// ones built elsewhere (a dropped reply's error notice).
const SEND_QUEUE: usize = 4;

/// The send ring: the small messages above, plus the transmit window's bytes,
/// which count against the same budget (8 frames x 256 B = 2 KiB while a reply is
/// going out).
const SEND_BUDGET: usize = 2560;

/// A reassembly buffer above this is released once its request is delivered
/// (an upload's chunk grows one to ~6 KB; it must not stay).
const KEEP_REASSEMBLY: usize = 1024;

/// The board's resend-timer floor, above the preset's 40 ms. While a shader
/// renders slowly (a palette cross-fade: ~80 ms ticks) the link task writes
/// one frame in 64-byte packets with a tick between them, so a frame's own
/// write outlasts 40 ms and the board resent frames the host was still
/// receiving: every frame twice in those stretches, half the throughput
/// (rehearsal `silicon:esp32c6 10:bd:a3:b0:8e:30`, 2026-09-27, e726f7083).
/// Real losses are still found early by SACK and the tail probe; this timer is
/// only the backstop.
const MIN_RTO_US: u64 = 200_000;

/// The lock hook (see the module docs), shared with the classic's UART link:
/// [`crate::link_lock`]. The default ([`UsbLinkShared::leak`]) is no lock.
pub use crate::link_lock::LinkLock;
use crate::link_lock::no_lock;

/// The link, and the doorbell that wakes its task.
pub struct UsbLinkShared {
    link: RefCell<Link<SelectiveRepeat>>,
    doorbell: Signal<CriticalSectionRawMutex, ()>,
    lock: LinkLock,
}

// SAFETY: `link` is the only field that is not `Sync`, and it is reached only
// through `with_link`, which borrows it inside `lock`. With the default lock
// (`leak`) both users share one executor and never run at once; a chip that
// puts them on different threads must inject a lock that keeps them apart
// (`leak_locked`), and the `RefCell` turns any overlap the lock lets through
// into a panic rather than an aliased `&mut`. The doorbell is a
// critical-section `Signal`, `Sync` on its own.
unsafe impl Sync for UsbLinkShared {}

impl UsbLinkShared {
    /// The board's link configuration: lp-link's USB preset, its buffers cut
    /// to what the board needs.
    ///
    /// Every buffer is allocated in `Link::new` for the link's life, and the
    /// preset's (24 KiB send ring, 32 log slots) would hold ~39 KB of a heap a
    /// loaded project leaves tight (a `ProjectRead` is refused under a 32 KB
    /// largest free block). Replies are not copied into the link at all: the
    /// transport serializes each into the static frame buffer and queues it
    /// as an external message, whose fragments the link cuts from there into
    /// its transmit window (`Link::send_external`). So the send ring is small
    /// ([`SEND_BUDGET`]), and the receive side is lazy (it grows with traffic,
    /// is capped at one largest request, and gives a large reassembly buffer
    /// back once its request is delivered).
    pub fn config() -> LinkConfig {
        let mut cfg = LinkConfig::usb();
        cfg.max_message = MAX_MESSAGE;
        cfg.send_budget = SEND_BUDGET;
        // Plus the inbox's 64 B queueing charge (`LinkConfig::validate`).
        cfg.rx_budget = MAX_MESSAGE + 64;
        cfg.keep_reassembly = KEEP_REASSEMBLY;
        cfg.send_queue = SEND_QUEUE;
        cfg.datagram_queue = LOG_DATAGRAMS;
        cfg.min_rto = MIN_RTO_US;
        cfg
    }

    /// A new link for this boot, leaked for the task and the transport to
    /// share. `nonce` must be random per boot (the chip's RNG): it is how the
    /// host learns the board restarted.
    pub fn leak(nonce: u32) -> &'static Self {
        Self::leak_locked(nonce, no_lock)
    }

    /// [`Self::leak`] for a chip whose link task and transport run on
    /// different threads: every [`Self::with_link`] runs inside `lock` (see
    /// the module docs).
    pub fn leak_locked(nonce: u32, lock: LinkLock) -> &'static Self {
        Box::leak(Box::new(Self {
            link: RefCell::new(Link::new(Self::config(), nonce)),
            doorbell: Signal::new(),
            lock,
        }))
    }

    /// Run `f` on the link, inside the link's lock. Keep `f` short, never
    /// call this from inside another `with_link`, and never hold what `f`
    /// returns across an `.await` (see the module docs).
    pub fn with_link<R>(&self, f: impl FnOnce(&mut Link<SelectiveRepeat>) -> R) -> R {
        let mut f = Some(f);
        let mut out = None;
        (self.lock)(&mut || {
            if let Some(f) = f.take() {
                out = Some(f(&mut self.link.borrow_mut()));
            }
        });
        match out {
            Some(out) => out,
            None => unreachable!("a LinkLock must run its argument"),
        }
    }

    /// Whether a host has the link up right now.
    pub fn is_established(&self) -> bool {
        self.with_link(|link| link.state() == LinkState::Established)
    }

    /// The link is still reading a reply out of the static frame buffer:
    /// nothing may serialize into it yet.
    pub fn frame_buf_in_use(&self) -> bool {
        self.with_link(|link| link.external_in_flight())
    }

    /// Queue the `len` bytes the caller serialized into the static frame
    /// buffer as one proto-channel message, without waiting, and wake the
    /// task; `false` if the link refused it (no session, or one still in
    /// flight). For a harness; the server's transport accounts for its sends
    /// itself. Check [`Self::frame_buf_in_use`] before serializing.
    pub fn try_send_frame_buf(&self, len: usize) -> bool {
        let queued = self.with_link(|link| {
            link.state() == LinkState::Established
                && link.send_external(lp_link::CH_PROTO, len).is_ok()
        });
        if queued {
            self.ring();
        }
        queued
    }

    /// Queue one channel-3 message (the over-the-air update protocol, see
    /// [`super::usb_update_channel`]) and wake the task: in the send ring
    /// when it fits ([`RING_MAX`](super::usb_update_channel::RING_MAX)),
    /// else as the external message out of the static frame buffer, when no
    /// reply holds it. Call from task context only: the frame buffer's
    /// writers (this, the server transport, the BLE mux, a radio link's
    /// `RadioLinkPort::send_update`) all run on the server loop's task or in
    /// core-only, which has no transport.
    ///
    /// It checks only **this** link's hold on the frame buffer. A caller
    /// with radio links beside it checks theirs first
    /// (`RadioLinkPort::frame_buf_in_use`; the C6's `ota::UpdateLinks`
    /// does), or a large answer could overwrite a reply a radio link is
    /// still reading.
    pub fn send_update(&self, bytes: &[u8]) -> UpdateSend {
        use super::usb_update_channel::RING_MAX;
        let ring = bytes.len() <= RING_MAX;
        let room = self.with_link(|link| {
            if link.state() != LinkState::Established {
                return Err(UpdateSend::NoSession);
            }
            if ring {
                return Ok(match link.send(CH_UPDATE, bytes) {
                    Ok(()) => UpdateSend::Queued,
                    Err(SendError::Full) => UpdateSend::Later,
                    Err(SendError::TooBig | SendError::BadChannel) => UpdateSend::TooBig,
                });
            }
            if bytes.len() > link.config().max_message {
                return Err(UpdateSend::TooBig);
            }
            Err(if link.external_in_flight() {
                UpdateSend::Later
            } else {
                // Free: copied outside the lock (below).
                UpdateSend::Queued
            })
        });
        let sent = match room {
            Ok(sent) => sent,
            Err(UpdateSend::Queued) => self.send_update_external(bytes),
            Err(other) => other,
        };
        if sent == UpdateSend::Queued {
            self.ring();
        }
        sent
    }

    /// A large channel-3 message through the frame buffer, which nothing
    /// holds: copied in (outside the lock, it is 4 KiB), then queued.
    #[cfg(feature = "server")]
    fn send_update_external(&self, bytes: &[u8]) -> UpdateSend {
        // SAFETY: the frame buffer's writers all run on this task (see
        // `send_update`), and the link is not reading it: no external
        // message is in flight (checked above, and nothing on this task
        // queued one since).
        let buf = unsafe { crate::serial::server_msg::frame_buf_mut() };
        let Some(dst) = buf.get_mut(..bytes.len()) else {
            return UpdateSend::TooBig;
        };
        dst.copy_from_slice(bytes);
        self.with_link(|link| {
            if link.state() != LinkState::Established {
                return UpdateSend::NoSession;
            }
            match link.send_external(CH_UPDATE, bytes.len()) {
                Ok(()) => UpdateSend::Queued,
                Err(SendError::Full) => UpdateSend::Later,
                Err(SendError::TooBig | SendError::BadChannel) => UpdateSend::TooBig,
            }
        })
    }

    #[cfg(not(feature = "server"))]
    fn send_update_external(&self, _bytes: &[u8]) -> UpdateSend {
        UpdateSend::TooBig
    }

    /// Something was queued: wake the link task to transmit it now rather
    /// than at its next timer.
    pub fn ring(&self) {
        self.doorbell.signal(());
    }

    /// The link task's side of [`Self::ring`].
    pub(crate) async fn doorbell(&self) {
        self.doorbell.wait().await;
    }

    /// The doorbell itself, for the log ring to ring when a record lands
    /// ([`crate::log_ring_logger::ring_on_record`]).
    pub(crate) fn doorbell_signal(&'static self) -> &'static Signal<CriticalSectionRawMutex, ()> {
        &self.doorbell
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};

    /// Every `with_link` runs inside the injected lock, exactly once each.
    #[test]
    fn every_with_link_runs_inside_the_injected_lock() {
        static ENTERED: AtomicU32 = AtomicU32::new(0);
        fn counting_lock(f: &mut dyn FnMut()) {
            ENTERED.fetch_add(1, Ordering::Relaxed);
            f()
        }
        let shared = UsbLinkShared::leak_locked(7, counting_lock);
        assert!(!shared.is_established());
        assert!(!shared.frame_buf_in_use());
        let state = shared.with_link(|link| link.state());
        assert_eq!(state, LinkState::Connecting);
        assert_eq!(ENTERED.load(Ordering::Relaxed), 3);
    }

    /// Channel 3: a small answer goes through the send ring, a 4 KiB one as
    /// the external message out of the frame buffer, and a second large one
    /// waits while the first is still being read.
    #[cfg(feature = "server")]
    #[test]
    fn update_messages_go_by_ring_or_by_frame_buffer() {
        use lp_link::LinkEvent;
        // The large answer goes out of the one static frame buffer, which the
        // transports' tests also serialize into: take the turn.
        let _turn = crate::serial::server_msg::frame_buf_turn();
        let shared = UsbLinkShared::leak(0x1111);
        assert_eq!(shared.send_update(b"R"), UpdateSend::NoSession);
        let mut host = Link::<SelectiveRepeat>::new(LinkConfig::usb(), 0x2222);
        let pump = |host: &mut Link<SelectiveRepeat>, t: u64| {
            for _ in 0..200 {
                let mut moved = false;
                if let Some(f) = shared.with_link(|l| {
                    l.poll_transmit_with(t, &mut super::super::usb_link_task::external_source)
                        .map(<[u8]>::to_vec)
                }) {
                    host.on_bytes(t, &f);
                    moved = true;
                }
                if let Some(f) = host.poll_transmit(t).map(<[u8]>::to_vec) {
                    shared.with_link(|l| l.on_bytes(t, &f));
                    moved = true;
                }
                if !moved {
                    break;
                }
            }
        };
        pump(&mut host, 0);
        assert!(shared.is_established());
        assert_eq!(shared.send_update(b"R\x45"), UpdateSend::Queued);
        let big: alloc::vec::Vec<u8> = (0..4102u32).map(|i| i as u8).collect();
        assert_eq!(shared.send_update(&big), UpdateSend::Queued);
        assert!(shared.frame_buf_in_use());
        assert_eq!(shared.send_update(&big), UpdateSend::Later);
        let mut got = alloc::vec::Vec::new();
        for t in 1..50u64 {
            pump(&mut host, t * 1_000);
            while let Some(ev) = host.recv() {
                if let LinkEvent::Message { channel, data } = ev {
                    assert_eq!(channel, CH_UPDATE);
                    got.push(data);
                }
            }
        }
        assert_eq!(got, alloc::vec![b"R\x45".to_vec(), big]);
        assert!(!shared.frame_buf_in_use());
    }

    /// The board's cut holds together, carries the largest reply, and costs
    /// well under the preset. (`--nocapture` prints the figures.)
    #[test]
    fn the_board_config_holds_one_largest_reply_and_costs_less_than_the_preset() {
        let cfg = UsbLinkShared::config();
        assert_eq!(cfg.validate(), Ok(()));
        assert!(cfg.max_message >= lpc_wire::PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES);
        assert!(
            cfg.send_budget >= cfg.tx_window as usize * cfg.max_payload as usize + 512,
            "the ring holds a full window plus a notice"
        );
        let board = Link::<SelectiveRepeat>::new(cfg.clone(), 1).ram_bytes();
        let preset = Link::<SelectiveRepeat>::new(LinkConfig::usb(), 1).ram_bytes();
        let bound = Link::<SelectiveRepeat>::ram_bound(&cfg);
        extern crate std;
        std::println!(
            "usb link RAM at rest: board {board} B, preset {preset} B; board bound {bound} B"
        );
        assert!(
            board + 8 * 1024 < preset,
            "board {board} B vs preset {preset} B"
        );
    }
}
