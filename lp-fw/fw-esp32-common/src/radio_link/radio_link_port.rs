//! The seam between the network edges (the chip crate's BLE task, and with
//! feature `wifi` its LAN endpoint) and the link mux
//! ([`super::LinkMuxTransport`]): one lp-link [`Link`] per open link, and
//! the rules for sharing it.
//!
//! This crate may not hold a radio stack (no esp-*, no BLE host, no IP
//! stack — see the seam rules in `Cargo.toml`), so the two halves meet here:
//!
//! - **Slots.** The first [`RADIO_LINK_SLOTS`] slots are Bluetooth's, the
//!   next [`LAN_LINK_SLOTS`] the LAN's (feature `wifi`; plan
//!   `lp2025/2026-10-05-1903-wifi-link-c6`, A2: at most two LAN links,
//!   separate from Bluetooth's two). A Bluetooth link is
//!   [`LinkTrust::Untrusted`]; a LAN link is a secure lp-link responder,
//!   [`LinkTrust::Keyed`] — its handshake's key decides its tier
//!   ([`RadioLinkSlot::trust`]).
//! - **Links.** The edge mints a [`LinkId`] per connection
//!   ([`RadioLinkPort::mint_link`]: monotonic, never reused, never
//!   [`LinkId::PRIMARY`]). Once it can deliver frames, it opens the
//!   connection's lp-link session on its slot ([`RadioLinkSlot::open`],
//!   sized to the connection's ATT MTU; [`RadioLinkSlot::open_lan`] for a
//!   LAN link) and announces it with [`RadioLinkEvent::Opened`]; when the
//!   connection is gone it closes the slot ([`RadioLinkSlot::close`], which
//!   frees the link) and announces [`RadioLinkEvent::Closed`].
//! - **Frames.** One lp-link frame is one datagram (Datagram framing): one
//!   ATT operation on Bluetooth, one binary WebSocket message on the LAN.
//!   Each received frame goes to [`RadioLinkSlot::on_datagram`] whole, and
//!   each frame [`RadioLinkSlot::poll_frame`] hands out is sent as one. The
//!   edge runs the link's timers ([`RadioLinkSlot::poll_timeout`]) and wakes
//!   on the mux's doorbell.
//! - **Messages.** The mux takes whole wire messages off the link and queues
//!   replies onto it (`with_link`, crate-internal). A long reply stays in the
//!   shared static frame buffer (`serial::server_msg`) as an lp-link
//!   *external* message and the link cuts its frames from there, so while a
//!   slot's link has one in flight ([`RadioLinkSlot::external_in_flight`]) no
//!   one may serialize into that buffer; the edge signals when it stops
//!   ([`RadioLinkSlot::released`]).
//! - **Close.** The mux asks the edge to drop a link (the login deadline, a
//!   reply the peer did not take in time, a secure session that reset) with
//!   [`RadioLinkSlot::request_close`]; the edge disconnects and reports
//!   [`RadioLinkEvent::Closed`] as for any other disconnect.
//!
//! **The lock, and the frame lease across threads.** Every borrow of a
//! slot's link is taken inside one synchronous call and dropped before any
//! `.await`. With Bluetooth alone everything runs on the one thread
//! executor, so a plain [`RefCell`] is the whole lock ([`RadioLinkPort::leak`]).
//! A LAN link is served from another thread (`lp-net` on the C6) while the
//! mux runs on the main one, so a port with LAN slots is made with
//! [`RadioLinkPort::leak_locked`] and every borrow — including the copy of
//! a frame out of the shared frame buffer in [`RadioLinkSlot::poll_frame`],
//! and every check and change of what the link has in flight — happens
//! under the lock it is given. The lease rule is then the same on both
//! threads: the mux serializes into the frame buffer only after every
//! slot's link has let go of it (checked under the lock), and a link reads
//! the buffer only under the lock while its external message is in flight;
//! a link dropped (revoked) by the mux is dropped under the lock, so no copy
//! can be under way when it goes. The lock must keep the other thread out
//! for its whole closure (the C6's is a priority-limited lock: no thread
//! switch lands while it is held) and must never be taken inside a critical
//! section. A critical-section mutex would mask every interrupt for each
//! frame's checksum, and the RMT refill (the LEDs) cannot wait.

use alloc::boxed::Box;
use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use lp_link::{Link as LpLink, Micros, SelectiveRepeat};
use lpc_shared::transport::{Link, LinkId, LinkTrust};

use super::radio_link_config::{MtuTooSmall, radio_link_config};

/// How many Bluetooth links can be open at once. The BLE task accepts at
/// most this many connections (DD12: two allowed, nothing gates on the
/// second). The GATT server's connection table, the host's resources and
/// the connection-task pool all follow this one constant.
pub const RADIO_LINK_SLOTS: usize = 2;

/// How many LAN links can be open at once (plan A2: Studio plus lp-cli),
/// separate from Bluetooth's. Zero without feature `wifi`.
#[cfg(feature = "wifi")]
pub const LAN_LINK_SLOTS: usize = 2;
/// How many LAN links can be open at once. Zero without feature `wifi`.
#[cfg(not(feature = "wifi"))]
pub const LAN_LINK_SLOTS: usize = 0;

/// Every slot: Bluetooth's first, then the LAN's.
pub const LINK_SLOTS: usize = RADIO_LINK_SLOTS + LAN_LINK_SLOTS;

/// Opened/closed notices. Two per slot outstanding is the worst case the
/// edges can produce before the server loop drains them.
const EVENT_DEPTH: usize = 2 * LINK_SLOTS + 2;

/// The cross-thread lock a port with LAN slots takes around every borrow:
/// it runs the closure with the other thread kept out.
pub type PortLock = fn(&mut dyn FnMut());

/// A link's lifecycle, as its edge reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioLinkEvent {
    /// `link` has an lp-link session on `slot` (not yet up: its handshake is
    /// ordinary traffic from here on).
    Opened { link: LinkId, slot: usize },
    /// `link` is gone (disconnected, or closed at the mux's request).
    Closed { link: LinkId },
}

/// Why the mux closed a link: a fixed phrase for the log line.
pub type CloseReason = &'static str;

/// The link a slot holds, and which connection it belongs to.
struct SlotLink {
    id: LinkId,
    link: LpLink<SelectiveRepeat>,
}

/// One connection slot: its link, and the signals both halves wait on.
pub struct RadioLinkSlot {
    /// Boxed: an idle slot costs a pointer, not a whole `Link` struct (the
    /// port is on the heap of every radio image, connected or not).
    link: RefCell<Option<Box<SlotLink>>>,
    /// What a link on this slot is trusted as.
    trust: LinkTrust,
    /// Taken around every borrow of `link` (see the module docs).
    lock: Option<PortLock>,
    /// Mux → edge: something was queued; transmit now rather than at the
    /// link's next timer.
    doorbell: Signal<CriticalSectionRawMutex, ()>,
    /// Edge → mux: this slot's link is no longer reading the frame buffer
    /// (possibly stale: the mux checks again).
    released: Signal<CriticalSectionRawMutex, ()>,
    close_request: Signal<CriticalSectionRawMutex, CloseReason>,
}

impl RadioLinkSlot {
    const fn new(trust: LinkTrust, lock: Option<PortLock>) -> Self {
        Self {
            link: RefCell::new(None),
            trust,
            lock,
            doorbell: Signal::new(),
            released: Signal::new(),
            close_request: Signal::new(),
        }
    }

    /// What a link on this slot is trusted as: a Bluetooth link is
    /// untrusted, a LAN link keyed (its secure handshake's key decides).
    pub fn trust(&self) -> LinkTrust {
        self.trust
    }

    /// Run `f` on the slot's link cell, under the port's lock when it has
    /// one.
    fn guarded<R>(&self, f: impl FnOnce(&mut Option<Box<SlotLink>>) -> R) -> R {
        match self.lock {
            None => f(&mut self.link.borrow_mut()),
            Some(lock) => {
                let mut f = Some(f);
                let mut out = None;
                lock(&mut || {
                    if let Some(f) = f.take() {
                        out = Some(f(&mut self.link.borrow_mut()));
                    }
                });
                match out {
                    Some(out) => out,
                    None => unreachable!("a port lock runs its closure"),
                }
            }
        }
    }

    // ---- edge side ----

    /// Forget anything a previous link on this slot left behind. Call before
    /// a new connection uses the slot.
    pub fn reset(&self) {
        self.guarded(|link| *link = None);
        self.doorbell.reset();
        self.close_request.reset();
        self.released.signal(());
    }

    /// Start `id`'s lp-link session on this slot, sized to the connection's
    /// ATT MTU, under `nonce` (random per connection: it is how the host
    /// learns this is a new session). `Err`: the MTU cannot carry a frame,
    /// and the caller disconnects (see [`MtuTooSmall`]).
    pub fn open(&self, id: LinkId, att_mtu: u16, nonce: u32) -> Result<u16, MtuTooSmall> {
        let cfg = radio_link_config(att_mtu)?;
        let max_payload = cfg.max_payload;
        let fresh = Box::new(SlotLink {
            id,
            link: LpLink::new(cfg, nonce),
        });
        self.guarded(|link| *link = Some(fresh));
        Ok(max_payload)
    }

    /// Start `id`'s secure lp-link session on this LAN slot
    /// ([`super::lan_link_config`]), the board as the Noise responder: no key
    /// up front, a [`lp_link::secure_channel::SecureEvent::KeyLookup`] the
    /// mux answers from the server's access store. `entropy` fills a buffer
    /// with fresh random bytes (32 per handshake). Returns the link's frame
    /// payload size.
    #[cfg(feature = "wifi")]
    pub fn open_lan(&self, id: LinkId, nonce: u32, entropy: fn(&mut [u8])) -> u16 {
        let cfg = super::lan_link_config::lan_link_config();
        let max_payload = cfg.max_payload;
        let fresh = Box::new(SlotLink {
            id,
            link: LpLink::new_secure(
                cfg,
                nonce,
                lp_link::secure_channel::SecureRole::Responder,
                entropy,
            ),
        });
        self.guarded(|link| *link = Some(fresh));
        max_payload
    }

    /// The connection is gone: free its link (its RAM goes back to the heap
    /// now) and wake a mux waiting for the frame buffer.
    pub fn close(&self) {
        let gone = self.guarded(Option::take);
        drop(gone);
        self.released.signal(());
    }

    /// One frame the peer sent: one whole lp-link frame. Ignored while the
    /// slot holds no link.
    pub fn on_datagram(&self, now: Micros, frame: &[u8]) {
        self.guarded(|link| {
            if let Some(slot) = link.as_mut() {
                slot.link.on_datagram(now, frame);
            }
        });
    }

    /// The next frame to send, handed to `take` (copy it out: the borrow —
    /// and the lock — end when `take` returns); `None` when the link has
    /// nothing to send now. A long reply's fragments are read from the frame
    /// buffer here, under the lock.
    pub fn poll_frame<R>(&self, now: Micros, take: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let (taken, let_go) = self.guarded(|link| {
            let Some(slot) = link.as_mut() else {
                return (None, false);
            };
            let taken = slot
                .link
                .poll_transmit_with(now, &mut read_frame_buf)
                .map(take);
            (taken, !slot.link.external_in_flight())
        });
        if let_go {
            self.released.signal(());
        }
        taken
    }

    /// When the link next needs [`Self::poll_frame`] for a timer (retransmit,
    /// delayed ACK, keepalive, SYN), or `None` with no link.
    pub fn poll_timeout(&self) -> Option<Micros> {
        self.guarded(|link| link.as_ref()?.link.poll_timeout())
    }

    /// The heap the slot's link holds right now (`Link::ram_bytes`), or `None`
    /// with no link.
    pub fn ram_bytes(&self) -> Option<usize> {
        self.guarded(|link| Some(link.as_ref()?.link.ram_bytes()))
    }

    /// The id of the link the slot holds, if any.
    pub fn link_id(&self) -> Option<LinkId> {
        self.guarded(|link| Some(link.as_ref()?.id))
    }

    /// The slot's link's counters (a secure link's handshakes, refusals,
    /// seal failures and replays among them), or `None` with no link.
    pub fn counters(&self) -> Option<lp_link::LinkCounters> {
        self.guarded(|link| Some(link.as_ref()?.link.counters().clone()))
    }

    /// Resolves when the mux queued something for this slot's link.
    pub async fn doorbell(&self) {
        self.doorbell.wait().await;
    }

    /// Resolves when the mux wants this slot's link dropped.
    pub async fn close_requested(&self) -> CloseReason {
        self.close_request.wait().await
    }

    // ---- mux side ----

    /// Ask the edge to drop this slot's link.
    pub fn request_close(&self, reason: CloseReason) {
        self.close_request.signal(reason);
    }

    /// Whatever link this slot holds is still reading a reply out of the
    /// frame buffer: nothing may serialize into it yet.
    pub fn external_in_flight(&self) -> bool {
        self.guarded(|link| {
            link.as_ref()
                .is_some_and(|slot| slot.link.external_in_flight())
        })
    }

    /// Run `f` on `id`'s link; `None` when the slot holds no link or another
    /// connection's. Never call it from inside another, and never hold what
    /// `f` returns across an `.await` (see the module docs).
    pub(crate) fn with_link<R>(
        &self,
        id: LinkId,
        f: impl FnOnce(&mut LpLink<SelectiveRepeat>) -> R,
    ) -> Option<R> {
        self.guarded(|link| {
            let slot = link.as_mut().filter(|slot| slot.id == id)?;
            Some(f(&mut slot.link))
        })
    }

    /// Run `f` on whatever link the slot holds.
    pub(crate) fn with_any_link<R>(
        &self,
        f: impl FnOnce(LinkId, &mut LpLink<SelectiveRepeat>) -> R,
    ) -> Option<R> {
        self.guarded(|link| {
            let slot = link.as_mut()?;
            Some(f(slot.id, &mut slot.link))
        })
    }

    /// Drop `id`'s link from the slot now (the mux closed it): it stops
    /// reading the frame buffer and frees its RAM before the edge has even
    /// disconnected.
    pub(crate) fn drop_link(&self, id: LinkId) {
        let gone = self.guarded(|link| {
            if link.as_ref().is_some_and(|slot| slot.id == id) {
                link.take()
            } else {
                None
            }
        });
        drop(gone);
        self.released.signal(());
    }

    /// Wake the edge to transmit what was just queued.
    pub(crate) fn ring(&self) {
        self.doorbell.signal(());
    }

    /// Resolves when the edge may have stopped reading the frame buffer
    /// (check [`Self::external_in_flight`] again).
    pub(crate) async fn released(&self) {
        self.released.wait().await;
    }

    #[cfg(test)]
    pub(crate) fn take_close_request(&self) -> Option<CloseReason> {
        self.close_request.try_take()
    }
}

/// Both halves' shared state. The firmware leaks one ([`RadioLinkPort::leak`],
/// or [`RadioLinkPort::leak_locked`] when an edge serves it from another
/// thread).
pub struct RadioLinkPort {
    radio: [RadioLinkSlot; RADIO_LINK_SLOTS],
    lan: [RadioLinkSlot; LAN_LINK_SLOTS],
    events: Channel<CriticalSectionRawMutex, RadioLinkEvent, EVENT_DEPTH>,
    next_link: AtomicU32,
}

impl Default for RadioLinkPort {
    fn default() -> Self {
        Self::new()
    }
}

impl RadioLinkPort {
    /// A port for one thread: no lock (see the module docs).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            radio: [const { RadioLinkSlot::new(LinkTrust::Untrusted, None) }; RADIO_LINK_SLOTS],
            lan: [const { RadioLinkSlot::new(LinkTrust::Keyed, None) }; LAN_LINK_SLOTS],
            events: Channel::new(),
            // 0 is `LinkId::PRIMARY` (the USB cable).
            next_link: AtomicU32::new(1),
        }
    }

    /// The firmware's one port, for the Bluetooth edge and the mux to share
    /// on one thread.
    #[must_use]
    pub fn leak() -> &'static Self {
        Box::leak(Box::new(Self::new()))
    }

    /// The firmware's one port when an edge runs on another thread (the
    /// LAN endpoint on `lp-net`): every borrow is taken under `lock`. The
    /// second half is what crosses to the other thread.
    #[must_use]
    pub fn leak_locked(lock: PortLock) -> (&'static Self, SharedPort) {
        let mut port = Box::new(Self::new());
        for slot in port.radio.iter_mut().chain(port.lan.iter_mut()) {
            slot.lock = Some(lock);
        }
        let port: &'static Self = Box::leak(port);
        (port, SharedPort(port))
    }

    /// A fresh id for a new connection: monotonic, never reused.
    pub fn mint_link(&self) -> LinkId {
        LinkId::new(self.next_link.fetch_add(1, Ordering::Relaxed))
    }

    /// The link as the server sees it on `slot`: untrusted on a Bluetooth
    /// slot, keyed on a LAN slot.
    #[must_use]
    pub fn link_on(&self, slot: usize, id: LinkId) -> Link {
        Link {
            id,
            trust: self.slot(slot).trust,
        }
    }

    /// A Bluetooth link as the server sees it: never trusted.
    #[must_use]
    pub const fn link(id: LinkId) -> Link {
        Link {
            id,
            trust: LinkTrust::Untrusted,
        }
    }

    /// Slot `index`.
    ///
    /// # Panics
    /// If `index >= LINK_SLOTS`.
    #[must_use]
    pub fn slot(&self, index: usize) -> &RadioLinkSlot {
        match index.checked_sub(RADIO_LINK_SLOTS) {
            None => &self.radio[index],
            Some(lan) => &self.lan[lan],
        }
    }

    /// Every slot, in index order.
    pub fn slots(&self) -> impl Iterator<Item = &RadioLinkSlot> {
        self.radio.iter().chain(self.lan.iter())
    }

    /// Edge side: announce a link event. Waits if the server loop is
    /// behind (it drains events every frame).
    pub async fn announce(&self, event: RadioLinkEvent) {
        self.events.send(event).await;
    }

    pub(crate) fn try_event(&self) -> Option<RadioLinkEvent> {
        self.events.try_receive().ok()
    }
}

/// A port made with [`RadioLinkPort::leak_locked`], as handed to the edge on
/// the other thread.
#[derive(Clone, Copy)]
pub struct SharedPort(&'static RadioLinkPort);

// SAFETY: a `SharedPort` exists only for a port built with a lock
// (`leak_locked`): every borrow of a slot's `RefCell` is taken under that
// lock, which keeps the other thread out for the whole borrow; the signals
// and the channel are critical-section types and the id counter is atomic.
unsafe impl Send for SharedPort {}

impl core::ops::Deref for SharedPort {
    type Target = RadioLinkPort;

    fn deref(&self) -> &RadioLinkPort {
        self.0
    }
}

/// Where a link reads an external message's bytes: the static frame buffer
/// the mux serialized the reply into, which it keeps unchanged while the
/// link has the message in flight.
fn read_frame_buf(offset: usize, out: &mut [u8]) {
    let bytes = crate::serial::server_msg::frame_bytes(offset + out.len());
    out.copy_from_slice(&bytes[offset..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize};

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static IN_LOCK: AtomicBool = AtomicBool::new(false);
    static OVERLAPS: AtomicUsize = AtomicUsize::new(0);

    fn test_lock(f: &mut dyn FnMut()) {
        let _held = TEST_LOCK.lock().unwrap();
        if IN_LOCK.swap(true, Ordering::SeqCst) {
            OVERLAPS.fetch_add(1, Ordering::SeqCst);
        }
        f();
        IN_LOCK.store(false, Ordering::SeqCst);
    }

    #[test]
    fn slots_are_bluetooth_then_lan_and_say_their_trust() {
        let port = RadioLinkPort::leak();
        for index in 0..RADIO_LINK_SLOTS {
            assert_eq!(port.slot(index).trust(), LinkTrust::Untrusted);
        }
        for index in RADIO_LINK_SLOTS..LINK_SLOTS {
            assert_eq!(port.slot(index).trust(), LinkTrust::Keyed);
        }
        let id = port.mint_link();
        assert_eq!(port.link_on(0, id).trust, LinkTrust::Untrusted);
    }

    /// The cross-thread lease: one thread drops (revokes) and reopens the
    /// slot's link while another polls frames and checks what is in flight;
    /// every borrow runs inside the lock, so no two ever overlap and a copy
    /// never sees a link being torn down.
    #[test]
    fn a_revoke_and_a_copy_never_interleave_under_the_lock() {
        let (port, shared) = RadioLinkPort::leak_locked(test_lock);
        let slot = 0;
        port.slot(slot)
            .open(port.mint_link(), 247, 1)
            .expect("open");
        let edge = std::thread::spawn(move || {
            let mut frames = 0usize;
            for now in 0..2_000u64 {
                shared
                    .slot(slot)
                    .poll_frame(now * 1_000, |frame| frames += frame.len());
                let _ = shared.slot(slot).external_in_flight();
                let _ = shared.slot(slot).poll_timeout();
            }
            frames
        });
        for round in 0..500u32 {
            let id = port.mint_link();
            port.slot(slot).open(id, 247, round).expect("reopen");
            port.slot(slot).drop_link(id);
            port.slot(slot).with_any_link(|_, link| link.ram_bytes());
        }
        // A borrow taken outside the lock would have overlapped (or panicked
        // the RefCell); how many SYNs the edge caught in between is luck.
        let _frames = edge.join().expect("no borrow panicked on the edge thread");
        assert_eq!(OVERLAPS.load(Ordering::SeqCst), 0, "two borrows overlapped");
    }
}
