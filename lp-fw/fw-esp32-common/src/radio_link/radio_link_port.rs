//! The seam between the network edges (the chip crate's BLE task, and with
//! feature `wifi` its LAN endpoint and the relay driver) and the link mux
//! ([`super::LinkMuxTransport`]): one lp-link [`Link`] per open link, and
//! the rules for sharing it.
//!
//! This crate may not hold a radio stack (no esp-*, no BLE host, no IP
//! stack — see the seam rules in `Cargo.toml`), so the two halves meet here:
//!
//! - **Slots.** The first [`RADIO_LINK_SLOTS`] slots are Bluetooth's, the
//!   next [`NETWORK_LINK_SLOTS`] the network's (feature `wifi`): one secure
//!   session the LAN endpoint and the cloud relay take turns at (Wi-Fi relay
//!   plan D2). A Bluetooth link is [`LinkTrust::Untrusted`]; a network link
//!   is a secure lp-link responder, [`LinkTrust::Keyed`] on the LAN and
//!   [`LinkTrust::Relayed`] through the relay — its handshake's key decides
//!   its tier. Each link records its trust and the edge serving it
//!   ([`SlotEdge`]).
//! - **Mode.** The boot decides once what its links are for
//!   ([`RadioLinkPort::decide_mode`]: serving the wire, or taking an update
//!   in core-only — [`super::radio_link_mode`]). No Bluetooth link opens
//!   before that ([`OpenRefused::ModeUndecided`]); the BLE task waits for it
//!   ([`RadioLinkPort::wait_for_mode`]), and so does the LAN endpoint. A LAN
//!   link opened in update mode advertises the wider LAN window
//!   ([`super::lan_link_config::lan_link_config_in`]); a relayed link keeps
//!   the serve configuration whatever the mode.
//! - **Links.** The edge mints a [`LinkId`] per connection
//!   ([`RadioLinkPort::mint_link`]: monotonic, never reused, never
//!   [`LinkId::PRIMARY`]). Once it can deliver frames, it opens the
//!   connection's lp-link session on its slot ([`RadioLinkPort::open`],
//!   sized to the connection's ATT MTU and configured for the boot's mode;
//!   [`RadioLinkSlot::open_network`] for a network link) and announces it
//!   with [`RadioLinkEvent::Opened`]; when the connection is gone it closes
//!   the slot ([`RadioLinkSlot::close`], or [`RadioLinkSlot::close_link`] on
//!   the network slot, which frees only its own link) and announces
//!   [`RadioLinkEvent::Closed`].
//! - **A busy network slot.** A connection that finds the network slot
//!   held parks its first frame ([`RadioLinkSlot::park_challenge`]) and
//!   announces [`RadioLinkEvent::Challenged`]; the mux takes the slot from
//!   the holder only for a handshake that proves the holder's own key
//!   ([`super::parked_handshake`]), and answers through
//!   [`RadioLinkSlot::verdict`]. A granted newcomer opens with
//!   [`RadioLinkSlot::take_over`].
//! - **Frames.** One lp-link frame is one datagram (Datagram framing): one
//!   ATT operation on Bluetooth, one binary WebSocket message on the LAN,
//!   one relay `Frame` on a route. Each received frame goes to
//!   [`RadioLinkSlot::on_datagram`] whole, and each frame
//!   [`RadioLinkSlot::poll_frame`] hands out is sent as one. The edge runs
//!   the link's timers ([`RadioLinkSlot::poll_timeout`]) and wakes on the
//!   mux's doorbell. On the network slot every one of these is asked by
//!   link id (`*_for`), so an edge whose link was taken over never touches
//!   its successor's.
//! - **Messages.** The mux takes whole wire messages off the link and queues
//!   replies onto it (`with_link`, crate-internal); core-only takes the
//!   link's events itself ([`RadioLinkSlot::recv`]). Channel 3, the update
//!   protocol, is answered through [`RadioLinkPort::send_update`] by either
//!   (the running engine's update hook, or core-only), on a Bluetooth link
//!   or a LAN link (a relayed link's channel 3 is not served yet). Core-only
//!   also answers a LAN link's key lookup itself
//!   ([`RadioLinkSlot::poll_key_event`], [`RadioLinkSlot::answer_key`]),
//!   since no server runs there. A long reply stays in the
//!   shared static frame buffer (`serial::server_msg`) as an lp-link
//!   *external* message and the link cuts its frames from there, so while a
//!   slot's link has one in flight ([`RadioLinkSlot::external_in_flight`]) no
//!   one may serialize into that buffer; the edge signals when it stops
//!   ([`RadioLinkSlot::released`]).
//! - **Close.** The mux drops a link and asks its edge to disconnect (the
//!   login deadline, a reply the peer did not take in time, a secure session
//!   that reset, a takeover) with `revoke`, addressed to the edge that
//!   served it; the edge disconnects and reports [`RadioLinkEvent::Closed`]
//!   as for any other disconnect.
//!
//! **The lock, and the frame lease across threads.** Every borrow of a
//! slot's link is taken inside one synchronous call and dropped before any
//! `.await`. With Bluetooth alone everything runs on the one thread
//! executor, so a plain [`RefCell`] is the whole lock ([`RadioLinkPort::leak`]).
//! A network link is served from another thread (`lp-net` on the C6) while
//! the mux runs on the main one, so a port with network slots is made with
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
use core::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use lp_link::{
    CH_UPDATE, Link as LpLink, LinkEvent, LinkState, Micros, SelectiveRepeat, SendError,
};
use lpc_shared::transport::{Link, LinkId, LinkTrust};

#[cfg(feature = "wifi")]
use super::parked_handshake::{ChallengeVerdict, Msg1, ParkRefused, ParkedHandshake};
use super::radio_link_config::{MtuTooSmall, SMALL_REPLY_BYTES, radio_link_config};
use super::radio_link_mode::RadioLinkMode;
use super::slot_edge::SlotEdge;
use crate::update_send::UpdateSend;

/// How many Bluetooth links can be open at once. The BLE task accepts at
/// most this many connections (DD12: two allowed, nothing gates on the
/// second). The GATT server's connection table, the host's resources and
/// the connection-task pool all follow this one constant.
pub const RADIO_LINK_SLOTS: usize = 2;

/// How many network links (the LAN's and the relay's, together) can be
/// open at once, separate from Bluetooth's. Zero without feature `wifi`.
///
/// One, not the plan's two (A2: Studio plus lp-cli). An open secure network
/// link holds about 14 KB (its session and its connection), and on the
/// emulated C6 two open while a project loads either refused the
/// post-deploy read (free 47,692 B, largest block 14,724 B) or, at the
/// first cut's window of 4, ran the shader compile out of memory and reset
/// the board. One slot and its boot buffers left the same upload passing
/// every time, with 67,216 B free and an 18,148 B block while the link
/// stayed open (PR B's memory gate; `lp-emu:esp32c6:t1+net=lan`). The
/// relay shares it (Wi-Fi relay plan D2): one network session, whichever
/// path it came by.
#[cfg(feature = "wifi")]
pub const NETWORK_LINK_SLOTS: usize = 1;
/// How many network links can be open at once. Zero without feature `wifi`.
#[cfg(not(feature = "wifi"))]
pub const NETWORK_LINK_SLOTS: usize = 0;

/// Every slot: Bluetooth's first, then the network's.
pub const LINK_SLOTS: usize = RADIO_LINK_SLOTS + NETWORK_LINK_SLOTS;

/// Opened/closed/challenged notices. Two per slot outstanding is the worst
/// case the edges can produce before the server loop drains them, and the
/// network slot's challenger adds two more.
const EVENT_DEPTH: usize = 2 * LINK_SLOTS + 4;

/// The cross-thread lock a port with network slots takes around every
/// borrow: it runs the closure with the other thread kept out.
pub type PortLock = fn(&mut dyn FnMut());

/// A link's lifecycle, as its edge reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioLinkEvent {
    /// `link` has an lp-link session on `slot` (not yet up: its handshake is
    /// ordinary traffic from here on).
    Opened { link: LinkId, slot: usize },
    /// `link` is gone (disconnected, or closed at the mux's request).
    Closed { link: LinkId },
    /// `link` found the network `slot` held and parked its first frame
    /// there: the mux decides whether it takes the slot over
    /// ([`RadioLinkSlot::verdict`]).
    Challenged { link: LinkId, slot: usize },
}

/// Why the mux closed a link: a fixed phrase for the log line.
pub type CloseReason = &'static str;

/// The network slot is held (or reserved for a granted newcomer): the
/// caller parks a challenge instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotHeld;

/// The slot no longer holds the asking edge's link: it was closed by the
/// mux, or taken over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotHeld;
/// Why a slot did not open a link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenRefused {
    /// The connection's ATT MTU cannot carry a frame: the caller disconnects.
    MtuTooSmall(MtuTooSmall),
    /// The boot has not decided what its radio links are for yet
    /// ([`RadioLinkPort::decide_mode`]): wait for
    /// [`RadioLinkPort::wait_for_mode`].
    ModeUndecided,
}

/// The link a slot holds, and which connection it belongs to.
struct SlotLink {
    id: LinkId,
    link: LpLink<SelectiveRepeat>,
    /// What the server trusts this link as.
    trust: LinkTrust,
    /// Who serves it.
    edge: SlotEdge,
}

/// Everything about a slot that the lock guards.
struct SlotState {
    /// Boxed: an idle slot costs a pointer, not a whole `Link` struct (the
    /// port is on the heap of every radio image, connected or not).
    link: Option<Box<SlotLink>>,
    /// A newcomer's first frame while the network slot is held.
    #[cfg(feature = "wifi")]
    parked: ParkedHandshake,
}

/// One connection slot: its link, and the signals both halves wait on.
pub struct RadioLinkSlot {
    state: RefCell<SlotState>,
    /// What a link opened on this slot without a trust of its own is
    /// trusted as (a Bluetooth link: untrusted; a LAN link: keyed).
    trust: LinkTrust,
    /// Taken around every borrow of `state` (see the module docs).
    lock: Option<PortLock>,
    /// Mux → edge, one per [`SlotEdge`]: something was queued; transmit now
    /// rather than at the link's next timer.
    doorbell: [Signal<CriticalSectionRawMutex, ()>; SlotEdge::COUNT],
    /// Edge → mux: this slot's link is no longer reading the frame buffer
    /// (possibly stale: the mux checks again).
    released: Signal<CriticalSectionRawMutex, ()>,
    /// Mux → edge, one per [`SlotEdge`]: drop your link.
    close_request: [Signal<CriticalSectionRawMutex, CloseReason>; SlotEdge::COUNT],
    /// Mux → the parked newcomer's edge: its verdict.
    #[cfg(feature = "wifi")]
    verdict: Signal<CriticalSectionRawMutex, ChallengeVerdict>,
    /// The boot's [`RadioLinkMode`] as [`RadioLinkMode::code`] (0: not yet
    /// decided), copied here by [`RadioLinkPort::decide_mode`] so a network
    /// link opened on this slot is configured for it.
    mode: AtomicU8,
}

impl RadioLinkSlot {
    const fn new(trust: LinkTrust, lock: Option<PortLock>) -> Self {
        Self {
            state: RefCell::new(SlotState {
                link: None,
                #[cfg(feature = "wifi")]
                parked: ParkedHandshake::new(),
            }),
            trust,
            lock,
            doorbell: [const { Signal::new() }; SlotEdge::COUNT],
            released: Signal::new(),
            close_request: [const { Signal::new() }; SlotEdge::COUNT],
            #[cfg(feature = "wifi")]
            verdict: Signal::new(),
            mode: AtomicU8::new(0),
        }
    }

    /// The mode a network link opened on this slot with `trust` is
    /// configured for: the boot's on the LAN (serve until decided), always
    /// serve through the relay.
    #[cfg(feature = "wifi")]
    fn network_mode(&self, trust: LinkTrust) -> RadioLinkMode {
        match (trust, RadioLinkMode::from_code(self.mode.load(Ordering::Acquire))) {
            (LinkTrust::Keyed, Some(mode)) => mode,
            _ => RadioLinkMode::Serve,
        }
    }

    /// What a link on this slot is trusted as by default: a Bluetooth link
    /// is untrusted, a network link keyed (a relayed one says so itself).
    pub fn trust(&self) -> LinkTrust {
        self.trust
    }

    /// Run `f` on the slot's state, under the port's lock when it has one.
    fn guarded<R>(&self, f: impl FnOnce(&mut SlotState) -> R) -> R {
        match self.lock {
            None => f(&mut self.state.borrow_mut()),
            Some(lock) => {
                let mut f = Some(f);
                let mut out = None;
                lock(&mut || {
                    if let Some(f) = f.take() {
                        out = Some(f(&mut self.state.borrow_mut()));
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
    /// a new connection uses the slot (a Bluetooth slot: one edge).
    pub fn reset(&self) {
        self.guarded(|state| state.link = None);
        self.reset_edge(SlotEdge::Local);
        self.released.signal(());
    }

    fn reset_edge(&self, edge: SlotEdge) {
        self.doorbell[edge.index()].reset();
        self.close_request[edge.index()].reset();
    }

    /// Start `id`'s lp-link session on this slot in `mode` (the port's
    /// [`RadioLinkPort::open`] is the way in: it supplies the boot's).
    fn open_in(
        &self,
        mode: RadioLinkMode,
        id: LinkId,
        att_mtu: u16,
        nonce: u32,
    ) -> Result<u16, MtuTooSmall> {
        let cfg = radio_link_config(att_mtu, mode)?;
        let max_payload = cfg.max_payload;
        let fresh = Box::new(SlotLink {
            id,
            link: LpLink::new(cfg, nonce),
            trust: self.trust,
            edge: SlotEdge::Local,
        });
        self.guarded(|state| state.link = Some(fresh));
        Ok(max_payload)
    }

    /// Start `id`'s secure lp-link session on the network slot
    /// ([`super::lan_link_config`], in the boot's mode on the LAN: see
    /// [`RadioLinkPort::wait_for_mode`]) for `edge`, trusted as `trust`
    /// ([`LinkTrust::Keyed`] on the LAN, [`LinkTrust::Relayed`] through the
    /// relay), the board as the Noise responder: no key up front, a
    /// [`lp_link::secure_channel::SecureEvent::KeyLookup`] the mux answers
    /// from the server's access store. `entropy` fills a buffer with fresh
    /// random bytes (32 per handshake). Returns the link's frame payload
    /// size, or [`SlotHeld`] when another link holds the slot (or a granted
    /// newcomer has it reserved): park a challenge instead.
    #[cfg(feature = "wifi")]
    pub fn open_network(
        &self,
        id: LinkId,
        nonce: u32,
        entropy: fn(&mut [u8]),
        trust: LinkTrust,
        edge: SlotEdge,
    ) -> Result<u16, SlotHeld> {
        // Built before the lock: the session is ~12 KB of allocation, and
        // nothing of it should happen with the other thread held out.
        let fresh = network_link(id, nonce, entropy, trust, edge, self.network_mode(trust));
        let max_payload = fresh.link.config().max_payload;
        let opened = self.guarded(|state| {
            if state.link.is_some() || state.parked.is_occupied() {
                return Err(fresh);
            }
            state.link = Some(fresh);
            Ok(())
        });
        match opened {
            Ok(()) => {
                self.reset_edge(edge);
                Ok(max_payload)
            }
            Err(unused) => {
                drop(unused);
                Err(SlotHeld)
            }
        }
    }

    /// The network slot is held: park `id`'s first frame (`edge` serves it)
    /// for the mux to judge, then announce [`RadioLinkEvent::Challenged`]
    /// and wait for [`Self::verdict`].
    #[cfg(feature = "wifi")]
    pub fn park_challenge(
        &self,
        id: LinkId,
        edge: SlotEdge,
        first_frame: &[u8],
    ) -> Result<(), ParkRefused> {
        let parked = self.guarded(|state| state.parked.park(id, edge, first_frame));
        if parked.is_ok() {
            self.verdict.reset();
        }
        parked
    }

    /// The mux's verdict on the parked challenge. Only the parked
    /// newcomer's edge waits on it.
    #[cfg(feature = "wifi")]
    pub async fn verdict(&self) -> ChallengeVerdict {
        self.verdict.wait().await
    }

    /// The parked newcomer's edge gives up on `id` (it left, or waited too
    /// long): the slot is no longer reserved for it. Announce
    /// [`RadioLinkEvent::Closed`] for it after, so the mux forgets it too.
    #[cfg(feature = "wifi")]
    pub fn withdraw_challenge(&self, id: LinkId) {
        self.guarded(|state| state.parked.clear(id));
    }

    /// Open the network slot for `id`, whose challenge the mux granted:
    /// the same as [`Self::open_network`], then the parked first frame is
    /// fed to the new session at `now`. [`SlotHeld`] if `id` was not
    /// granted, or the old link has not gone.
    #[cfg(feature = "wifi")]
    pub fn take_over(
        &self,
        id: LinkId,
        now: Micros,
        nonce: u32,
        entropy: fn(&mut [u8]),
        trust: LinkTrust,
    ) -> Result<u16, SlotHeld> {
        let edge = self.guarded(|state| state.parked.granted_edge(id));
        let Some(edge) = edge else {
            return Err(SlotHeld);
        };
        let fresh = network_link(id, nonce, entropy, trust, edge, self.network_mode(trust));
        let max_payload = fresh.link.config().max_payload;
        let opened = self.guarded(|state| {
            if state.link.is_some() {
                return Err(fresh);
            }
            let Some((_, first)) = state.parked.take_granted(id) else {
                return Err(fresh);
            };
            let mut fresh = fresh;
            fresh.link.on_datagram(now, first);
            state.link = Some(fresh);
            Ok(())
        });
        match opened {
            Ok(()) => {
                self.reset_edge(edge);
                Ok(max_payload)
            }
            Err(unused) => {
                drop(unused);
                Err(SlotHeld)
            }
        }
    }

    /// The connection is gone: free its link (its RAM goes back to the heap
    /// now) and wake a mux waiting for the frame buffer. A Bluetooth slot's
    /// edge (one connection per slot).
    pub fn close(&self) {
        let gone = self.guarded(|state| state.link.take());
        drop(gone);
        self.released.signal(());
    }

    /// `id`'s connection is gone: free its link if the slot still holds it
    /// (a link the mux revoked, or one taken over, is someone else's now);
    /// whether it did.
    pub fn close_link(&self, id: LinkId) -> bool {
        let gone = self.guarded(|state| take_if(&mut state.link, id));
        let freed = gone.is_some();
        drop(gone);
        self.released.signal(());
        freed
    }

    /// One frame the peer sent: one whole lp-link frame. Ignored while the
    /// slot holds no link.
    pub fn on_datagram(&self, now: Micros, frame: &[u8]) {
        self.guarded(|state| {
            if let Some(slot) = state.link.as_mut() {
                slot.link.on_datagram(now, frame);
            }
        });
    }

    /// [`Self::on_datagram`] for `id`'s link only: [`NotHeld`] when the
    /// slot no longer holds it.
    pub fn on_datagram_for(&self, id: LinkId, now: Micros, frame: &[u8]) -> Result<(), NotHeld> {
        self.guarded(|state| {
            let slot = state
                .link
                .as_mut()
                .filter(|slot| slot.id == id)
                .ok_or(NotHeld)?;
            slot.link.on_datagram(now, frame);
            Ok(())
        })
    }

    /// The next frame to send, handed to `take` (copy it out: the borrow —
    /// and the lock — end when `take` returns); `None` when the link has
    /// nothing to send now. A long reply's fragments are read from the frame
    /// buffer here, under the lock.
    pub fn poll_frame<R>(&self, now: Micros, take: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let (taken, let_go) = self.guarded(|state| {
            let Some(slot) = state.link.as_mut() else {
                return (None, false);
            };
            poll_link(slot, now, take)
        });
        if let_go {
            self.released.signal(());
        }
        taken
    }

    /// [`Self::poll_frame`] for `id`'s link only: [`NotHeld`] when the slot
    /// no longer holds it.
    pub fn poll_frame_for<R>(
        &self,
        id: LinkId,
        now: Micros,
        take: impl FnOnce(&[u8]) -> R,
    ) -> Result<Option<R>, NotHeld> {
        let polled = self.guarded(|state| {
            let slot = state.link.as_mut().filter(|slot| slot.id == id)?;
            Some(poll_link(slot, now, take))
        });
        let (taken, let_go) = polled.ok_or(NotHeld)?;
        if let_go {
            self.released.signal(());
        }
        Ok(taken)
    }

    /// When the link next needs [`Self::poll_frame`] for a timer (retransmit,
    /// delayed ACK, keepalive, SYN), or `None` with no link.
    pub fn poll_timeout(&self) -> Option<Micros> {
        self.guarded(|state| state.link.as_ref()?.link.poll_timeout())
    }

    /// [`Self::poll_timeout`] for `id`'s link only: `None` also when the
    /// slot no longer holds it.
    pub fn poll_timeout_for(&self, id: LinkId) -> Option<Micros> {
        self.guarded(|state| {
            state
                .link
                .as_ref()
                .filter(|slot| slot.id == id)?
                .link
                .poll_timeout()
        })
    }

    /// The heap the slot's link holds right now (`Link::ram_bytes`), or `None`
    /// with no link.
    pub fn ram_bytes(&self) -> Option<usize> {
        self.guarded(|state| Some(state.link.as_ref()?.link.ram_bytes()))
    }

    /// The id of the link the slot holds, if any.
    pub fn link_id(&self) -> Option<LinkId> {
        self.guarded(|state| Some(state.link.as_ref()?.id))
    }

    /// The slot's link's counters (a secure link's handshakes, refusals,
    /// seal failures and replays among them), or `None` with no link.
    pub fn counters(&self) -> Option<lp_link::LinkCounters> {
        self.guarded(|state| Some(state.link.as_ref()?.link.counters().clone()))
    }

    /// Resolves when the mux queued something for this slot's link (the
    /// slot's own edge).
    pub async fn doorbell(&self) {
        self.doorbell_for(SlotEdge::Local).await;
    }

    /// Resolves when the mux queued something for a link `edge` serves.
    pub async fn doorbell_for(&self, edge: SlotEdge) {
        self.doorbell[edge.index()].wait().await;
    }

    /// Resolves when the mux wants this slot's link dropped (the slot's own
    /// edge).
    pub async fn close_requested(&self) -> CloseReason {
        self.close_requested_for(SlotEdge::Local).await
    }

    /// Resolves when the mux wants the link `edge` serves dropped.
    pub async fn close_requested_for(&self, edge: SlotEdge) -> CloseReason {
        self.close_request[edge.index()].wait().await
    }

    /// The mux's close request for `edge`, if one is waiting: why an edge
    /// whose link is no longer held lost it.
    pub fn try_close_request_for(&self, edge: SlotEdge) -> Option<CloseReason> {
        self.close_request[edge.index()].try_take()
    }

    // ---- mux and core-only side ----

    /// The next event of `id`'s link (a message, its session coming up or
    /// resetting); `None` when there is none, or the slot holds no link or
    /// another connection's. Core-only reads its links here; while the
    /// engine runs, the mux does.
    pub fn recv(&self, id: LinkId) -> Option<LinkEvent> {
        self.with_link(id, LpLink::recv).flatten()
    }

    /// The lp-link session `id`'s link is in, while the slot holds it (a
    /// message queued for one session must never reach the next).
    pub fn generation(&self, id: LinkId) -> Option<u32> {
        self.with_link(id, |link| link.generation())
    }

    /// Ask the slot's own edge to drop its link (a slot the mux cannot use).
    pub fn request_close(&self, reason: CloseReason) {
        self.close_request[SlotEdge::Local.index()].signal(reason);
    }

    /// Drop `id`'s link from the slot now and ask the edge that served it to
    /// disconnect: it stops reading the frame buffer and frees its RAM
    /// before the edge has even noticed. A link the slot no longer holds
    /// (already dropped) still has its edge asked, on the slot's own edge.
    pub fn revoke(&self, id: LinkId, reason: CloseReason) {
        let gone = self.guarded(|state| take_if(&mut state.link, id));
        let edge = gone.as_ref().map_or(SlotEdge::Local, |slot| slot.edge);
        drop(gone);
        self.released.signal(());
        self.close_request[edge.index()].signal(reason);
        self.doorbell[edge.index()].signal(());
    }

    /// Whatever link this slot holds is still reading a reply out of the
    /// frame buffer: nothing may serialize into it yet.
    pub fn external_in_flight(&self) -> bool {
        self.guarded(|state| {
            state
                .link
                .as_ref()
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
        self.guarded(|state| {
            let slot = state.link.as_mut().filter(|slot| slot.id == id)?;
            Some(f(&mut slot.link))
        })
    }

    /// Run `f` on whatever link the slot holds.
    pub(crate) fn with_any_link<R>(
        &self,
        f: impl FnOnce(LinkId, &mut LpLink<SelectiveRepeat>) -> R,
    ) -> Option<R> {
        self.guarded(|state| {
            let slot = state.link.as_mut()?;
            Some(f(slot.id, &mut slot.link))
        })
    }

    /// The trust `id`'s link was opened with, while the slot holds it.
    pub(crate) fn trust_of(&self, id: LinkId) -> Option<LinkTrust> {
        self.guarded(|state| Some(state.link.as_ref().filter(|slot| slot.id == id)?.trust))
    }

    /// Drop `id`'s link from the slot now (the mux closed it): it stops
    /// reading the frame buffer and frees its RAM before the edge has even
    /// disconnected.
    pub(crate) fn drop_link(&self, id: LinkId) {
        let gone = self.guarded(|state| take_if(&mut state.link, id));
        drop(gone);
        self.released.signal(());
    }

    /// Wake the edge serving the slot's link to transmit what was just
    /// queued.
    pub(crate) fn ring(&self) {
        let edge = self.guarded(|state| state.link.as_ref().map_or(SlotEdge::Local, |s| s.edge));
        self.doorbell[edge.index()].signal(());
    }

    /// `id`'s parked first frame, parsed, while the mux has not decided:
    /// `Some(None)` when it is parked but is not a msg1.
    #[cfg(feature = "wifi")]
    pub(crate) fn parked_msg1(&self, id: LinkId) -> Option<Option<Msg1>> {
        self.guarded(|state| state.parked.waiting_msg1(id))
    }

    /// Tell the parked newcomer `id` the slot is its (the holder is already
    /// revoked).
    #[cfg(feature = "wifi")]
    pub(crate) fn grant_challenge(&self, id: LinkId) {
        if self.guarded(|state| state.parked.grant(id)) {
            self.verdict.signal(ChallengeVerdict::TakeOver);
        }
    }

    /// Core-only: `id`'s next secure-handshake event — a key lookup to
    /// answer ([`Self::answer_key`]) or a wrong key to charge — while the
    /// slot holds it. The mux takes them itself while the engine runs.
    #[cfg(feature = "wifi")]
    pub fn poll_key_event(&self, id: LinkId) -> Option<lp_link::secure_channel::SecureEvent> {
        self.with_link(id, LpLink::poll_secure_event).flatten()
    }

    /// Core-only: answer `id`'s key lookup for `key_id`, and wake its edge
    /// to send what the handshake wrote.
    #[cfg(feature = "wifi")]
    pub fn answer_key(
        &self,
        id: LinkId,
        key_id: lp_link::secure_channel::KeyId,
        answer: lpc_shared::transport::KeyAnswer,
    ) {
        self.with_link(id, |l| {
            super::network_key_answer::answer_key_lookup(l, key_id, answer);
        });
        self.ring();
    }

    /// The key `id`'s secure session authenticated, and which candidate
    /// matched, once it is up.
    #[cfg(feature = "wifi")]
    pub fn session_auth(&self, id: LinkId) -> Option<lp_link::secure_channel::SessionAuth> {
        self.with_link(id, |l| l.session_auth()).flatten()
    }

    /// Tell the parked newcomer `id` it is turned away.
    #[cfg(feature = "wifi")]
    pub fn refuse_challenge(&self, id: LinkId) {
        if self.guarded(|state| state.parked.clear(id)) {
            self.verdict.signal(ChallengeVerdict::Busy);
        }
    }

    /// Resolves when the edge may have stopped reading the frame buffer
    /// (check [`Self::external_in_flight`] again).
    pub(crate) async fn released(&self) {
        self.released.wait().await;
    }

    #[cfg(test)]
    pub(crate) fn take_close_request(&self) -> Option<CloseReason> {
        self.close_request[SlotEdge::Local.index()].try_take()
    }

    #[cfg(test)]
    pub(crate) fn take_close_request_for(&self, edge: SlotEdge) -> Option<CloseReason> {
        self.close_request[edge.index()].try_take()
    }

    #[cfg(all(test, feature = "wifi"))]
    pub(crate) fn try_verdict(&self) -> Option<ChallengeVerdict> {
        self.verdict.try_take()
    }
}

/// A network link for `edge`, configured for `mode`: secure, the board as
/// the responder.
#[cfg(feature = "wifi")]
fn network_link(
    id: LinkId,
    nonce: u32,
    entropy: fn(&mut [u8]),
    trust: LinkTrust,
    edge: SlotEdge,
    mode: RadioLinkMode,
) -> Box<SlotLink> {
    Box::new(SlotLink {
        id,
        link: LpLink::new_secure(
            super::lan_link_config::lan_link_config_in(mode),
            nonce,
            lp_link::secure_channel::SecureRole::Responder,
            entropy,
        ),
        trust,
        edge,
    })
}

/// Take the slot's link if it is `id`'s.
fn take_if(link: &mut Option<Box<SlotLink>>, id: LinkId) -> Option<Box<SlotLink>> {
    if link.as_ref().is_some_and(|slot| slot.id == id) {
        link.take()
    } else {
        None
    }
}

/// The next frame of `slot`'s link handed to `take`, and whether the link
/// has let go of the frame buffer.
fn poll_link<R>(
    slot: &mut SlotLink,
    now: Micros,
    take: impl FnOnce(&[u8]) -> R,
) -> (Option<R>, bool) {
    let taken = slot
        .link
        .poll_transmit_with(now, &mut read_frame_buf)
        .map(take);
    (taken, !slot.link.external_in_flight())
}

/// Both halves' shared state. The firmware leaks one ([`RadioLinkPort::leak`],
/// or [`RadioLinkPort::leak_locked`] when an edge serves it from another
/// thread).
pub struct RadioLinkPort {
    radio: [RadioLinkSlot; RADIO_LINK_SLOTS],
    network: [RadioLinkSlot; NETWORK_LINK_SLOTS],
    events: Channel<CriticalSectionRawMutex, RadioLinkEvent, EVENT_DEPTH>,
    next_link: AtomicU32,
    /// The boot's [`RadioLinkMode`] as [`RadioLinkMode::code`] (0: not yet
    /// decided), set once by [`Self::decide_mode`].
    mode: AtomicU8,
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
            network: [const { RadioLinkSlot::new(LinkTrust::Keyed, None) }; NETWORK_LINK_SLOTS],
            events: Channel::new(),
            // 0 is `LinkId::PRIMARY` (the USB cable).
            next_link: AtomicU32::new(1),
            mode: AtomicU8::new(0),
        }
    }

    /// The firmware's one port, for the Bluetooth edge and the mux to share
    /// on one thread.
    #[must_use]
    pub fn leak() -> &'static Self {
        Box::leak(Box::new(Self::new()))
    }

    /// The firmware's one port when an edge runs on another thread (the
    /// network edges on `lp-net`): every borrow is taken under `lock`. The
    /// second half is what crosses to the other thread.
    #[must_use]
    pub fn leak_locked(lock: PortLock) -> (&'static Self, SharedPort) {
        let mut port = Box::new(Self::new());
        for slot in port.radio.iter_mut().chain(port.network.iter_mut()) {
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
    /// slot; on the network slot keyed, or relayed when it came through the
    /// relay.
    #[must_use]
    pub fn link_on(&self, slot: usize, id: LinkId) -> Link {
        let slot = self.slot(slot);
        Link {
            id,
            trust: slot.trust_of(id).unwrap_or(slot.trust),
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
            Some(network) => &self.network[network],
        }
    }

    /// Every slot, in index order.
    pub fn slots(&self) -> impl Iterator<Item = &RadioLinkSlot> {
        self.radio.iter().chain(self.network.iter())
    }

    /// Decide what this boot's radio links are for. Once per boot, before
    /// the radio side may open a link; a connection waiting for it
    /// ([`Self::wait_for_mode`]) wakes. A second, different decision is a
    /// boot bug and is ignored (the first stands: links already open were
    /// configured for it).
    pub fn decide_mode(&self, mode: RadioLinkMode) {
        let first = self
            .mode
            .compare_exchange(0, mode.code(), Ordering::AcqRel, Ordering::Acquire);
        if let Err(held) = first
            && held != mode.code()
        {
            log::error!("radio links: mode already decided — {mode:?} ignored");
            return;
        }
        // The waiter is the slot's edge (a Bluetooth connection, the LAN
        // endpoint), on the doorbell it otherwise waits on for transmits (it
        // holds no link yet, so nothing else rings it; a ring it later finds
        // stale costs one quiet turn).
        for slot in self.slots() {
            slot.mode.store(mode.code(), Ordering::Release);
            slot.ring();
        }
    }

    /// The boot's mode for its radio links, if decided.
    #[must_use]
    pub fn mode(&self) -> Option<RadioLinkMode> {
        RadioLinkMode::from_code(self.mode.load(Ordering::Acquire))
    }

    /// Edge side: the boot's mode, once decided. Slot `slot`'s edge (a
    /// Bluetooth connection, or the LAN endpoint on a network slot) waits
    /// here before it opens its link, so no link is ever configured for a
    /// mode nobody chose (see [`super::radio_link_mode`]).
    pub async fn wait_for_mode(&self, slot: usize) -> RadioLinkMode {
        loop {
            if let Some(mode) = self.mode() {
                return mode;
            }
            self.slot(slot).doorbell().await;
        }
    }

    /// BLE task: start `id`'s lp-link session on Bluetooth slot `slot`, sized to the
    /// connection's ATT MTU and configured for the boot's mode, under `nonce`
    /// (random per connection: it is how the host learns this is a new
    /// session). The largest payload on success. `Err`: the MTU cannot
    /// carry a frame (the caller disconnects, see [`MtuTooSmall`]), or the
    /// mode is not decided yet — wait for [`Self::wait_for_mode`] first:
    /// nothing opens before it.
    ///
    /// # Panics
    /// If `slot >= RADIO_LINK_SLOTS` (a network slot opens with
    /// [`RadioLinkSlot::open_network`]).
    pub fn open(
        &self,
        slot: usize,
        id: LinkId,
        att_mtu: u16,
        nonce: u32,
    ) -> Result<u16, OpenRefused> {
        let mode = self.mode().ok_or(OpenRefused::ModeUndecided)?;
        self.radio[slot]
            .open_in(mode, id, att_mtu, nonce)
            .map_err(OpenRefused::MtuTooSmall)
    }

    /// Edge side: announce a link event. Waits if the server loop is
    /// behind (it drains events every frame).
    pub async fn announce(&self, event: RadioLinkEvent) {
        self.events.send(event).await;
    }

    /// The next link event, if any. One reader per boot: the mux while the
    /// engine runs, core-only otherwise.
    pub fn try_event(&self) -> Option<RadioLinkEvent> {
        self.events.try_receive().ok()
    }

    /// Some radio or LAN link is still reading a long message out of the
    /// static frame buffer: nothing may serialize into it yet. The USB link
    /// answers the same question for itself
    /// (`UsbLinkShared::frame_buf_in_use`).
    #[must_use]
    pub fn frame_buf_in_use(&self) -> bool {
        self.slots().any(RadioLinkSlot::external_in_flight)
    }

    /// The lp-link session `link` is in, while it is open.
    #[must_use]
    pub fn link_generation(&self, link: LinkId) -> Option<u32> {
        self.slots().find_map(|slot| slot.generation(link))
    }

    /// Queue one channel-3 message (the over-the-air update protocol) on
    /// radio or LAN link `link` and wake the edge serving it: in the link's
    /// send ring when it is at
    /// most [`SMALL_REPLY_BYTES`] (`R`, `N`, `M`, a login step), else — a
    /// read-back `D`, one 4 KiB chunk — as the link's external message out of
    /// the static frame buffer, when no radio link holds it. **The USB
    /// link's hold on that buffer is the caller's to check first**
    /// (`UsbLinkShared::frame_buf_in_use`): this port cannot see it.
    ///
    /// Call from task context only, on the task that writes the frame
    /// buffer: the server loop's (the mux, the engine's update hook) or
    /// core-only's, which has no transport.
    pub fn send_update(&self, link: LinkId, bytes: &[u8]) -> UpdateSend {
        let Some(slot) = self.slots().find(|s| s.generation(link).is_some()) else {
            return UpdateSend::NoSession;
        };
        let ring = bytes.len() <= SMALL_REPLY_BYTES;
        let room = slot
            .with_link(link, |l| {
                if l.state() != LinkState::Established {
                    return Err(UpdateSend::NoSession);
                }
                if ring {
                    return Ok(update_send_result(l.send(CH_UPDATE, bytes)));
                }
                if bytes.len() > l.config().max_message {
                    return Err(UpdateSend::TooBig);
                }
                // Room in principle: the frame buffer decides (below).
                Err(UpdateSend::Queued)
            })
            .unwrap_or(Err(UpdateSend::NoSession));
        let sent = match room {
            Ok(sent) => sent,
            Err(UpdateSend::Queued) if self.frame_buf_in_use() => UpdateSend::Later,
            Err(UpdateSend::Queued) => slot.send_update_external(link, bytes),
            Err(other) => other,
        };
        if sent == UpdateSend::Queued {
            slot.ring();
        }
        sent
    }
}

impl RadioLinkSlot {
    /// A large channel-3 message through the frame buffer, which no link
    /// holds (see [`RadioLinkPort::send_update`]): copied in, then queued as
    /// `id`'s external message.
    fn send_update_external(&self, id: LinkId, bytes: &[u8]) -> UpdateSend {
        // SAFETY: the frame buffer's writers all run on this task (see
        // `RadioLinkPort::send_update`), and no link reads it: no radio or
        // LAN slot has an external message in flight (checked by the caller,
        // under each slot's lock — a LAN link reads the buffer only while
        // one is, and only this task queues one), and the USB link's hold is
        // the caller's to have checked.
        let buf = unsafe { crate::serial::server_msg::frame_buf_mut() };
        let Some(dst) = buf.get_mut(..bytes.len()) else {
            return UpdateSend::TooBig;
        };
        dst.copy_from_slice(bytes);
        self.with_link(id, |l| {
            if l.state() != LinkState::Established {
                return UpdateSend::NoSession;
            }
            update_send_result(l.send_external(CH_UPDATE, bytes.len()))
        })
        .unwrap_or(UpdateSend::NoSession)
    }
}

/// A link's answer to one channel-3 send.
fn update_send_result(result: Result<(), SendError>) -> UpdateSend {
    match result {
        Ok(()) => UpdateSend::Queued,
        Err(SendError::Full) => UpdateSend::Later,
        Err(SendError::TooBig | SendError::BadChannel) => UpdateSend::TooBig,
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

impl SharedPort {
    /// The port itself, for an edge that keeps it (the relay driver).
    #[must_use]
    pub fn port(self) -> &'static RadioLinkPort {
        self.0
    }
}

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
    fn slots_are_bluetooth_then_network_and_say_their_trust() {
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

    /// The network slot: one link at a time, whichever edge; a relayed link
    /// says so; an edge's id-checked calls fail once its link is revoked,
    /// and the close request reaches the edge that served it, not the other.
    #[cfg(feature = "wifi")]
    #[test]
    fn the_network_slot_holds_one_link_and_addresses_its_edge() {
        let port = RadioLinkPort::leak();
        let index = RADIO_LINK_SLOTS;
        let slot = port.slot(index);
        let relay = port.mint_link();
        slot.open_network(relay, 1, fill, LinkTrust::Relayed, SlotEdge::Relay)
            .expect("an empty slot opens");
        assert_eq!(port.link_on(index, relay).trust, LinkTrust::Relayed);
        let lan = port.mint_link();
        assert_eq!(
            slot.open_network(lan, 2, fill, LinkTrust::Keyed, SlotEdge::Local),
            Err(SlotHeld)
        );
        assert_eq!(slot.poll_frame_for(lan, 0, |_| ()), Err(NotHeld));
        assert!(slot.on_datagram_for(relay, 0, &[0; 8]).is_ok());

        slot.revoke(relay, "taken over");
        assert_eq!(slot.take_close_request_for(SlotEdge::Local), None);
        assert_eq!(
            slot.take_close_request_for(SlotEdge::Relay),
            Some("taken over")
        );
        assert_eq!(slot.poll_frame_for(relay, 0, |_| ()), Err(NotHeld));
        assert!(!slot.close_link(relay), "already gone");
        slot.open_network(lan, 2, fill, LinkTrust::Keyed, SlotEdge::Local)
            .expect("free again");
        assert!(
            !slot.close_link(relay),
            "an old id never frees the new link"
        );
        assert_eq!(slot.link_id(), Some(lan));
    }

    /// A LAN link's SYN — its first frame — advertises the wider LAN window
    /// in update mode (core-only) and the serve window otherwise; a relayed
    /// link keeps the serve window in either mode, and so does a LAN link
    /// opened before the mode is decided.
    #[cfg(feature = "wifi")]
    #[test]
    fn a_lan_link_opened_in_update_mode_advertises_the_wide_window_in_its_syn() {
        use super::super::radio_link_mode::LAN_UPDATE_RX_WINDOW;
        let syn_window = |port: &'static RadioLinkPort, trust: LinkTrust, edge: SlotEdge| {
            let slot = port.slot(RADIO_LINK_SLOTS);
            let id = port.mint_link();
            slot.open_network(id, 7, fill, trust, edge).unwrap();
            let syn = slot
                .poll_frame_for(id, 0, <[u8]>::to_vec)
                .unwrap()
                .expect("a SYN first");
            slot.close_link(id);
            let header = lp_link::frame::Header::parse(&syn).unwrap();
            assert_eq!(header.kind, lp_link::frame::FrameKind::Syn);
            let body = &syn[lp_link::frame::HEADER_LEN..syn.len() - 4];
            lp_link::frame::SynBody::parse(body).unwrap().rx_window
        };
        let undecided = RadioLinkPort::leak();
        assert_eq!(syn_window(undecided, LinkTrust::Keyed, SlotEdge::Local), 2);
        for (mode, lan) in [
            (RadioLinkMode::Update, LAN_UPDATE_RX_WINDOW),
            (RadioLinkMode::Serve, 2),
        ] {
            let port = RadioLinkPort::leak();
            port.decide_mode(mode);
            assert_eq!(
                syn_window(port, LinkTrust::Keyed, SlotEdge::Local),
                lan,
                "{mode:?}"
            );
            assert_eq!(
                syn_window(port, LinkTrust::Relayed, SlotEdge::Relay),
                2,
                "{mode:?}"
            );
        }
        assert_eq!(LAN_UPDATE_RX_WINDOW, 8);
    }

    /// The LAN endpoint waits for the boot's mode like a Bluetooth
    /// connection, on its slot's doorbell.
    #[cfg(feature = "wifi")]
    #[test]
    fn the_lan_endpoint_waits_for_the_mode() {
        use core::future::Future as _;
        let port = RadioLinkPort::leak();
        let mut waiting = core::pin::pin!(port.wait_for_mode(RADIO_LINK_SLOTS));
        let waker = core::task::Waker::noop();
        let mut cx = core::task::Context::from_waker(waker);
        assert!(waiting.as_mut().poll(&mut cx).is_pending());
        port.decide_mode(RadioLinkMode::Update);
        assert_eq!(
            waiting.as_mut().poll(&mut cx),
            core::task::Poll::Ready(RadioLinkMode::Update)
        );
    }

    /// A granted challenge reserves the slot for its newcomer, and its
    /// parked frame is the new session's first.
    #[cfg(feature = "wifi")]
    #[test]
    fn a_granted_challenger_and_only_it_opens_the_slot_with_its_parked_frame() {
        let port = RadioLinkPort::leak();
        let slot = port.slot(RADIO_LINK_SLOTS);
        let holder = port.mint_link();
        slot.open_network(holder, 1, fill, LinkTrust::Keyed, SlotEdge::Local)
            .unwrap();
        let newcomer = port.mint_link();
        let syn = [0x5a; 84];
        slot.park_challenge(newcomer, SlotEdge::Relay, &syn)
            .unwrap();
        assert!(
            matches!(slot.parked_msg1(newcomer), Some(None)),
            "not a msg1"
        );
        slot.revoke(holder, "taken over");
        let stranger = port.mint_link();
        assert_eq!(
            slot.open_network(stranger, 3, fill, LinkTrust::Keyed, SlotEdge::Local),
            Err(SlotHeld),
            "a parked challenge reserves the slot"
        );
        assert_eq!(
            slot.take_over(newcomer, 0, 4, fill, LinkTrust::Relayed),
            Err(SlotHeld),
            "not granted yet"
        );
        slot.grant_challenge(newcomer);
        assert_eq!(slot.try_verdict(), Some(ChallengeVerdict::TakeOver));
        slot.take_over(newcomer, 0, 4, fill, LinkTrust::Relayed)
            .expect("granted");
        assert_eq!(slot.link_id(), Some(newcomer));
        assert_eq!(
            slot.counters().unwrap().frames_rx,
            0,
            "a damaged SYN is no frame"
        );
        assert_eq!(
            port.link_on(RADIO_LINK_SLOTS, newcomer).trust,
            LinkTrust::Relayed
        );
    }

    /// The cross-thread lease: one thread drops (revokes) and reopens the
    /// slot's link while another polls frames and checks what is in flight;
    /// every borrow runs inside the lock, so no two ever overlap and a copy
    /// never sees a link being torn down.
    #[test]
    fn a_revoke_and_a_copy_never_interleave_under_the_lock() {
        let (port, shared) = RadioLinkPort::leak_locked(test_lock);
        let slot = 0;
        port.decide_mode(RadioLinkMode::Serve);
        port.open(slot, port.mint_link(), 247, 1).expect("open");
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
            port.open(slot, id, 247, round).expect("reopen");
            port.slot(slot).drop_link(id);
            port.slot(slot).with_any_link(|_, link| link.ram_bytes());
        }
        // A borrow taken outside the lock would have overlapped (or panicked
        // the RefCell); how many SYNs the edge caught in between is luck.
        let _frames = edge.join().expect("no borrow panicked on the edge thread");
        assert_eq!(OVERLAPS.load(Ordering::SeqCst), 0, "two borrows overlapped");
    }

    #[cfg(feature = "wifi")]
    fn fill(buf: &mut [u8]) {
        buf.fill(7);
    }
}
