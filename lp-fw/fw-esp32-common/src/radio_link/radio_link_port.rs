//! The seam between a radio stack (the chip crate's BLE task) and the link
//! mux ([`super::LinkMuxTransport`]): channels, and the rules for using them.
//!
//! This crate may not hold a radio stack (no esp-*, no BLE host — see the
//! seam rules in `Cargo.toml`), so the two halves meet here, on plain
//! embassy-sync channels:
//!
//! - **Links.** The radio side mints a [`LinkId`] per connection
//!   ([`RadioLinkPort::mint_link`]: monotonic, never reused, never
//!   [`LinkId::PRIMARY`]) and announces it with [`RadioLinkEvent::Opened`]
//!   once it can deliver the link's replies, and [`RadioLinkEvent::Closed`]
//!   when it is gone. Every radio link is [`LinkTrust::Untrusted`].
//! - **Incoming.** Complete `M!` lines, tagged with their link
//!   ([`RadioLinkPort::deliver_line`]).
//! - **Outgoing, one frame in flight overall.** The mux serializes a frame
//!   into the shared static frame buffer (`serial::server_msg`) — the same
//!   buffer USB uses — and hands the radio side a [`RadioWriteRequest`] on
//!   the link's *slot*. The radio side reads the frame only through
//!   [`RadioLinkPort::copy_frame`], which copies synchronously and only while
//!   the mux's lease on that generation is live, so a mux that gave up
//!   waiting (and revoked the lease) can reuse the buffer at once without a
//!   late reader ever seeing the next frame's bytes.
//! - **Close.** The mux asks the radio side to drop a link (the login
//!   deadline, a write that did not finish) with
//!   [`RadioLinkSlot::request_close`]; the radio side disconnects and reports
//!   [`RadioLinkEvent::Closed`] as for any other disconnect.
//!
//! Everything runs on the one thread executor, so "synchronously" above means
//! "with no `.await` in between", which is what the lease relies on.

use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
use lpc_shared::transport::{Link, LinkId, LinkTrust};
use lpc_wire::TransportError;

/// How many radio links can be open at once. The BLE task accepts at most
/// this many connections (DD12: two allowed, nothing gates on the second).
pub const RADIO_LINK_SLOTS: usize = 2;

/// Incoming `M!` lines waiting for the server loop, across all radio links.
const INCOMING_DEPTH: usize = 8;
/// Opened/closed notices. Two per slot outstanding is the worst case the
/// radio side can produce before the server loop drains them.
const EVENT_DEPTH: usize = 2 * RADIO_LINK_SLOTS + 2;

/// A radio link's lifecycle, as the radio side reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioLinkEvent {
    /// `link` can now carry frames both ways; its writes arrive on `slot`.
    Opened { link: LinkId, slot: usize },
    /// `link` is gone (disconnected, or closed at the mux's request).
    Closed { link: LinkId },
}

/// One frame for one radio link: `len` bytes of the shared frame buffer,
/// leased under `generation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioWriteRequest {
    pub link: LinkId,
    pub generation: u32,
    pub len: usize,
}

/// Why the mux closed a link: a fixed phrase for the log line.
pub type CloseReason = &'static str;

/// The per-link half of the port: where that link's frames and close
/// requests arrive.
pub struct RadioLinkSlot {
    write_request: Channel<CriticalSectionRawMutex, RadioWriteRequest, 1>,
    close_request: Signal<CriticalSectionRawMutex, CloseReason>,
}

impl RadioLinkSlot {
    const fn new() -> Self {
        Self {
            write_request: Channel::new(),
            close_request: Signal::new(),
        }
    }

    /// Radio side: forget anything a previous link on this slot left behind.
    /// Call before announcing a new link on it.
    pub fn reset(&self) {
        self.write_request.clear();
        self.close_request.reset();
    }

    /// Radio side: the next frame to send on this slot's link.
    pub async fn next_write(&self) -> RadioWriteRequest {
        self.write_request.receive().await
    }

    /// Radio side: resolves when the mux wants this slot's link dropped.
    pub async fn close_requested(&self) -> CloseReason {
        self.close_request.wait().await
    }

    /// Mux side: ask the radio side to drop this slot's link.
    pub fn request_close(&self, reason: CloseReason) {
        self.close_request.signal(reason);
    }

    #[cfg(test)]
    pub(crate) fn take_close_request(&self) -> Option<CloseReason> {
        self.close_request.try_take()
    }

    #[cfg(test)]
    pub(crate) fn has_pending_write(&self) -> bool {
        !self.write_request.is_empty()
    }
}

/// Both halves' shared state. The firmware uses [`RADIO_LINK_PORT`]; tests
/// build their own.
pub struct RadioLinkPort {
    slots: [RadioLinkSlot; RADIO_LINK_SLOTS],
    write_result: Channel<CriticalSectionRawMutex, (u32, Result<(), TransportError>), 1>,
    incoming: Channel<CriticalSectionRawMutex, (LinkId, String), INCOMING_DEPTH>,
    events: Channel<CriticalSectionRawMutex, RadioLinkEvent, EVENT_DEPTH>,
    /// `LEASED | (generation & GENERATION_MASK)` while a radio write owns the
    /// frame buffer, 0 when none does.
    frame_lease: AtomicU32,
    next_link: AtomicU32,
}

const LEASED: u32 = 0x8000_0000;
const GENERATION_MASK: u32 = !LEASED;

/// The firmware's one port.
pub static RADIO_LINK_PORT: RadioLinkPort = RadioLinkPort::new();

impl Default for RadioLinkPort {
    fn default() -> Self {
        Self::new()
    }
}

impl RadioLinkPort {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [RadioLinkSlot::new(), RadioLinkSlot::new()],
            write_result: Channel::new(),
            incoming: Channel::new(),
            events: Channel::new(),
            frame_lease: AtomicU32::new(0),
            // 0 is `LinkId::PRIMARY` (the USB cable).
            next_link: AtomicU32::new(1),
        }
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

    /// Slot `index`'s channels.
    ///
    /// # Panics
    /// If `index >= RADIO_LINK_SLOTS`.
    #[must_use]
    pub fn slot(&self, index: usize) -> &RadioLinkSlot {
        &self.slots[index]
    }

    /// Radio side: announce a link event. Waits if the server loop is
    /// behind (it drains events every frame).
    pub async fn announce(&self, event: RadioLinkEvent) {
        self.events.send(event).await;
    }

    /// Radio side: hand one complete `M!` line from `link` to the server
    /// loop. `false`: the queue was full and the line was dropped — the
    /// caller logs it (the USB link drops the same way, for the same
    /// reason: the server loop is not keeping up).
    pub fn deliver_line(&self, link: LinkId, line: String) -> bool {
        self.incoming.try_send((link, line)).is_ok()
    }

    /// Radio side: copy `dst.len()` bytes of `request`'s frame, starting at
    /// `offset`, into `dst`. `false` (and `dst` untouched) when the mux no
    /// longer holds the frame for this request — it timed out and moved on —
    /// or the span is out of range.
    pub fn copy_frame(&self, request: &RadioWriteRequest, offset: usize, dst: &mut [u8]) -> bool {
        if self.frame_lease.load(Ordering::Acquire) != lease_word(request.generation) {
            return false;
        }
        let Some(end) = offset.checked_add(dst.len()) else {
            return false;
        };
        if end > request.len {
            return false;
        }
        let frame = crate::serial::server_msg::frame_bytes(request.len);
        dst.copy_from_slice(&frame[offset..end]);
        true
    }

    /// Radio side: report how `request` went. Never waits: at most one
    /// request is outstanding, so anything already in the result channel is
    /// a stale answer the mux stopped waiting for, and is replaced.
    pub fn finish_write(&self, request: &RadioWriteRequest, result: Result<(), TransportError>) {
        self.write_result.clear();
        let _ = self.write_result.try_send((request.generation, result));
    }

    // ---- mux side (crate-internal) ----

    pub(crate) fn try_event(&self) -> Option<RadioLinkEvent> {
        self.events.try_receive().ok()
    }

    pub(crate) fn try_line(&self) -> Option<(LinkId, String)> {
        self.incoming.try_receive().ok()
    }

    pub(crate) fn lease_frame(&self, generation: u32) {
        self.frame_lease
            .store(lease_word(generation), Ordering::Release);
    }

    pub(crate) fn revoke_frame(&self) {
        self.frame_lease.store(0, Ordering::Release);
    }

    /// Queue `request` on `slot`, discarding a request a previous, abandoned
    /// write left there (its lease is already revoked).
    pub(crate) fn submit_write(&self, slot: usize, request: RadioWriteRequest) {
        let channel = &self.slots[slot].write_request;
        channel.clear();
        let _ = channel.try_send(request);
    }

    /// Take back `slot`'s request if the radio side never picked it up.
    pub(crate) fn withdraw_write(&self, slot: usize) {
        self.slots[slot].write_request.clear();
    }

    /// The result for `generation`, discarding stale ones.
    pub(crate) async fn write_result(&self, generation: u32) -> Result<(), TransportError> {
        loop {
            let (got, result) = self.write_result.receive().await;
            if got == generation {
                return result;
            }
            log::warn!(
                "radio link: discarding stale write result generation={got} (awaiting {generation})"
            );
        }
    }
}

fn lease_word(generation: u32) -> u32 {
    LEASED | (generation & GENERATION_MASK)
}
