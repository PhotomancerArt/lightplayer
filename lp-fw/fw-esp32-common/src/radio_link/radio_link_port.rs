//! The seam between a radio stack (the chip crate's BLE task) and the link
//! mux ([`super::LinkMuxTransport`]): one lp-link [`Link`] per open radio
//! link, and the rules for sharing it.
//!
//! This crate may not hold a radio stack (no esp-*, no BLE host — see the
//! seam rules in `Cargo.toml`), so the two halves meet here:
//!
//! - **Mode.** The boot decides once what its radio links are for
//!   ([`RadioLinkPort::decide_mode`]: serving the wire, or taking an update
//!   in core-only — [`super::radio_link_mode`]). No link opens before that
//!   ([`OpenRefused::ModeUndecided`]); the radio side waits for it
//!   ([`RadioLinkPort::wait_for_mode`]).
//! - **Links.** The radio side mints a [`LinkId`] per connection
//!   ([`RadioLinkPort::mint_link`]: monotonic, never reused, never
//!   [`LinkId::PRIMARY`]). Once it can deliver frames to the central (the
//!   central enabled notifications), it opens the connection's lp-link session
//!   on its slot ([`RadioLinkPort::open`], sized to the connection's ATT MTU,
//!   configured for the boot's mode) and announces it with
//!   [`RadioLinkEvent::Opened`]; when the connection is
//!   gone it closes the slot ([`RadioLinkSlot::close`], which frees the link)
//!   and announces [`RadioLinkEvent::Closed`]. Every radio link is
//!   [`LinkTrust::Untrusted`].
//! - **Frames.** One lp-link frame is one ATT operation (Datagram framing):
//!   each write the central makes to RX goes to [`RadioLinkSlot::on_datagram`]
//!   whole, and each frame [`RadioLinkSlot::poll_frame`] hands out is one
//!   notification. The radio side runs the link's timers
//!   ([`RadioLinkSlot::poll_timeout`]) and wakes on the mux's doorbell.
//! - **Messages.** The mux takes whole wire messages off the link and queues
//!   replies onto it (`with_link`, crate-internal); core-only takes the
//!   link's events itself ([`RadioLinkSlot::recv`]). Channel 3, the update
//!   protocol, is answered through [`RadioLinkPort::send_update`] by either
//!   (the running engine's update hook, or core-only). A long reply stays in the
//!   shared static frame buffer (`serial::server_msg`) as an lp-link
//!   *external* message and the link cuts its frames from there, so while a
//!   slot's link has one in flight ([`RadioLinkSlot::external_in_flight`]) no
//!   one may serialize into that buffer; the radio side signals when it
//!   stops ([`RadioLinkSlot::released`]).
//! - **Close.** The mux asks the radio side to drop a link (the login
//!   deadline, a reply the central did not take in time) with
//!   [`RadioLinkSlot::request_close`]; the radio side disconnects and reports
//!   [`RadioLinkEvent::Closed`] as for any other disconnect.
//!
//! Everything runs on the one thread executor, so a plain [`RefCell`] is the
//! lock, exactly as on the USB link (`usb_link::UsbLinkShared`): every borrow
//! is taken inside a synchronous call and dropped before any `.await`. A
//! critical-section mutex would mask interrupts for every frame's checksum,
//! and the RMT refill (the LEDs) cannot wait. That is why the port is leaked
//! on the heap ([`RadioLinkPort::leak`]) rather than a `static`: a `RefCell`
//! is not `Sync`.

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

use super::radio_link_config::{MtuTooSmall, SMALL_REPLY_BYTES, radio_link_config};
use super::radio_link_mode::RadioLinkMode;
use crate::update_send::UpdateSend;

/// How many radio links can be open at once. The BLE task accepts at most
/// this many connections (DD12: two allowed, nothing gates on the second).
/// The GATT server's connection table, the host's resources and the
/// connection-task pool all follow this one constant.
pub const RADIO_LINK_SLOTS: usize = 2;

/// Opened/closed notices. Two per slot outstanding is the worst case the
/// radio side can produce before the server loop drains them.
const EVENT_DEPTH: usize = 2 * RADIO_LINK_SLOTS + 2;

/// A radio link's lifecycle, as the radio side reports it.
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
}

/// One connection slot: its link, and the signals both halves wait on.
pub struct RadioLinkSlot {
    /// Boxed: an idle slot costs a pointer, not a whole `Link` struct (the
    /// port is on the heap of every BLE image, connected or not).
    link: RefCell<Option<Box<SlotLink>>>,
    /// Mux → radio side: something was queued; transmit now rather than at
    /// the link's next timer.
    doorbell: Signal<CriticalSectionRawMutex, ()>,
    /// Radio side → mux: this slot's link is no longer reading the frame
    /// buffer (possibly stale: the mux checks again).
    released: Signal<CriticalSectionRawMutex, ()>,
    close_request: Signal<CriticalSectionRawMutex, CloseReason>,
}

impl RadioLinkSlot {
    const fn new() -> Self {
        Self {
            link: RefCell::new(None),
            doorbell: Signal::new(),
            released: Signal::new(),
            close_request: Signal::new(),
        }
    }

    // ---- radio side ----

    /// Forget anything a previous link on this slot left behind. Call before
    /// a new connection uses the slot.
    pub fn reset(&self) {
        *self.link.borrow_mut() = None;
        self.doorbell.reset();
        self.close_request.reset();
        self.released.signal(());
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
        *self.link.borrow_mut() = Some(Box::new(SlotLink {
            id,
            link: LpLink::new(cfg, nonce),
        }));
        Ok(max_payload)
    }

    /// The connection is gone: free its link (its RAM goes back to the heap
    /// now) and wake a mux waiting for the frame buffer.
    pub fn close(&self) {
        *self.link.borrow_mut() = None;
        self.released.signal(());
    }

    /// One write the central made to RX: one whole lp-link frame. Ignored
    /// while the slot holds no link.
    pub fn on_datagram(&self, now: Micros, frame: &[u8]) {
        if let Some(slot) = self.link.borrow_mut().as_mut() {
            slot.link.on_datagram(now, frame);
        }
    }

    /// The next frame to notify, handed to `take` (copy it out: the borrow
    /// ends when `take` returns); `None` when the link has nothing to send
    /// now. A long reply's fragments are read from the frame buffer here.
    pub fn poll_frame<R>(&self, now: Micros, take: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let mut guard = self.link.borrow_mut();
        let slot = guard.as_mut()?;
        let taken = slot
            .link
            .poll_transmit_with(now, &mut read_frame_buf)
            .map(take);
        if !slot.link.external_in_flight() {
            self.released.signal(());
        }
        taken
    }

    /// When the link next needs [`Self::poll_frame`] for a timer (retransmit,
    /// delayed ACK, keepalive, SYN), or `None` with no link.
    pub fn poll_timeout(&self) -> Option<Micros> {
        self.link.borrow().as_ref()?.link.poll_timeout()
    }

    /// The heap the slot's link holds right now (`Link::ram_bytes`), or `None`
    /// with no link.
    pub fn ram_bytes(&self) -> Option<usize> {
        Some(self.link.borrow().as_ref()?.link.ram_bytes())
    }

    /// Resolves when the mux queued something for this slot's link.
    pub async fn doorbell(&self) {
        self.doorbell.wait().await;
    }

    /// Resolves when the mux wants this slot's link dropped.
    pub async fn close_requested(&self) -> CloseReason {
        self.close_request.wait().await
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

    /// Ask the radio side to drop this slot's link.
    pub fn request_close(&self, reason: CloseReason) {
        self.close_request.signal(reason);
    }

    /// Whatever link this slot holds is still reading a reply out of the
    /// frame buffer: nothing may serialize into it yet.
    pub fn external_in_flight(&self) -> bool {
        self.link
            .borrow()
            .as_ref()
            .is_some_and(|slot| slot.link.external_in_flight())
    }

    /// Run `f` on `id`'s link; `None` when the slot holds no link or another
    /// connection's. Never call it from inside another, and never hold what
    /// `f` returns across an `.await` (see the module docs).
    pub(crate) fn with_link<R>(
        &self,
        id: LinkId,
        f: impl FnOnce(&mut LpLink<SelectiveRepeat>) -> R,
    ) -> Option<R> {
        let mut guard = self.link.borrow_mut();
        let slot = guard.as_mut().filter(|slot| slot.id == id)?;
        Some(f(&mut slot.link))
    }

    /// Run `f` on whatever link the slot holds.
    pub(crate) fn with_any_link<R>(
        &self,
        f: impl FnOnce(LinkId, &mut LpLink<SelectiveRepeat>) -> R,
    ) -> Option<R> {
        let mut guard = self.link.borrow_mut();
        let slot = guard.as_mut()?;
        Some(f(slot.id, &mut slot.link))
    }

    /// Drop `id`'s link from the slot now (the mux closed it): it stops
    /// reading the frame buffer and frees its RAM before the radio side has
    /// even disconnected.
    pub(crate) fn drop_link(&self, id: LinkId) {
        let mut guard = self.link.borrow_mut();
        if guard.as_ref().is_some_and(|slot| slot.id == id) {
            *guard = None;
        }
        drop(guard);
        self.released.signal(());
    }

    /// Wake the radio side to transmit what was just queued.
    pub(crate) fn ring(&self) {
        self.doorbell.signal(());
    }

    /// Resolves when the radio side may have stopped reading the frame buffer
    /// (check [`Self::external_in_flight`] again).
    pub(crate) async fn released(&self) {
        self.released.wait().await;
    }

    #[cfg(test)]
    pub(crate) fn take_close_request(&self) -> Option<CloseReason> {
        self.close_request.try_take()
    }
}

/// Both halves' shared state. The firmware leaks one ([`RadioLinkPort::leak`]).
pub struct RadioLinkPort {
    slots: [RadioLinkSlot; RADIO_LINK_SLOTS],
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
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [const { RadioLinkSlot::new() }; RADIO_LINK_SLOTS],
            events: Channel::new(),
            // 0 is `LinkId::PRIMARY` (the USB cable).
            next_link: AtomicU32::new(1),
            mode: AtomicU8::new(0),
        }
    }

    /// The firmware's one port, for the radio side and the mux to share.
    #[must_use]
    pub fn leak() -> &'static Self {
        Box::leak(Box::new(Self::new()))
    }

    /// A fresh id for a new radio connection: monotonic, never reused.
    pub fn mint_link(&self) -> LinkId {
        LinkId::new(self.next_link.fetch_add(1, Ordering::Relaxed))
    }

    /// The link as the server sees it: a radio link is never trusted.
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
    /// If `index >= RADIO_LINK_SLOTS`.
    #[must_use]
    pub fn slot(&self, index: usize) -> &RadioLinkSlot {
        &self.slots[index]
    }

    /// Every slot, in index order.
    pub fn slots(&self) -> impl Iterator<Item = &RadioLinkSlot> {
        self.slots.iter()
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
        // The waiter is the slot's connection, on the doorbell it otherwise
        // waits on for transmits (it holds no link yet, so nothing else
        // rings it; a ring it later finds stale costs one quiet turn).
        for slot in &self.slots {
            slot.ring();
        }
    }

    /// The boot's mode for its radio links, if decided.
    #[must_use]
    pub fn mode(&self) -> Option<RadioLinkMode> {
        RadioLinkMode::from_code(self.mode.load(Ordering::Acquire))
    }

    /// Radio side: the boot's mode, once decided. Slot `slot`'s connection
    /// waits here before it opens its link, so no link is ever configured
    /// for a mode nobody chose (see [`super::radio_link_mode`]).
    pub async fn wait_for_mode(&self, slot: usize) -> RadioLinkMode {
        loop {
            if let Some(mode) = self.mode() {
                return mode;
            }
            self.slots[slot].doorbell().await;
        }
    }

    /// Radio side: start `id`'s lp-link session on slot `slot`, sized to the
    /// connection's ATT MTU and configured for the boot's mode, under `nonce`
    /// (random per connection: it is how the host learns this is a new
    /// session). The largest payload on success. `Err`: the MTU cannot
    /// carry a frame (the caller disconnects, see [`MtuTooSmall`]), or the
    /// mode is not decided yet — wait for [`Self::wait_for_mode`] first:
    /// nothing opens before it.
    pub fn open(
        &self,
        slot: usize,
        id: LinkId,
        att_mtu: u16,
        nonce: u32,
    ) -> Result<u16, OpenRefused> {
        let mode = self.mode().ok_or(OpenRefused::ModeUndecided)?;
        self.slots[slot]
            .open_in(mode, id, att_mtu, nonce)
            .map_err(OpenRefused::MtuTooSmall)
    }

    /// Radio side: announce a link event. Waits if the server loop is
    /// behind (it drains events every frame).
    pub async fn announce(&self, event: RadioLinkEvent) {
        self.events.send(event).await;
    }

    /// The next link event, if any. One reader per boot: the mux while the
    /// engine runs, core-only otherwise.
    pub fn try_event(&self) -> Option<RadioLinkEvent> {
        self.events.try_receive().ok()
    }

    /// Some radio link is still reading a long message out of the static
    /// frame buffer: nothing may serialize into it yet. The USB link answers
    /// the same question for itself (`UsbLinkShared::frame_buf_in_use`).
    #[must_use]
    pub fn frame_buf_in_use(&self) -> bool {
        self.slots.iter().any(RadioLinkSlot::external_in_flight)
    }

    /// The lp-link session `link` is in, while it is open.
    #[must_use]
    pub fn link_generation(&self, link: LinkId) -> Option<u32> {
        self.slots.iter().find_map(|slot| slot.generation(link))
    }

    /// Queue one channel-3 message (the over-the-air update protocol) on
    /// `link` and wake the radio side: in the link's send ring when it is at
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
        let Some(slot) = self.slots.iter().find(|s| s.generation(link).is_some()) else {
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
        // `RadioLinkPort::send_update`), and no link reads it: no radio slot
        // has an external message in flight (checked by the caller), and the
        // USB link's hold is the caller's to have checked.
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

/// Where a radio link reads an external message's bytes: the static frame
/// buffer the mux serialized the reply into, which it keeps unchanged while
/// the link has the message in flight.
fn read_frame_buf(offset: usize, out: &mut [u8]) {
    let bytes = crate::serial::server_msg::frame_bytes(offset + out.len());
    out.copy_from_slice(&bytes[offset..]);
}
