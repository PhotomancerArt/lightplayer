//! The wire pusher: N wires over S pooled transmitter slots, sequenced by a
//! dedicated thread on the ISR's core.
//!
//! # Why this exists
//!
//! With more wires than transmitter slots, someone must start the second wave
//! the moment the first completes — *mid-render*, or the render thread pays a
//! full wave of wire time per frame (measured: 23 fps instead of the
//! engine-bound 31 at five wires over four slots). The ISR cannot take the
//! job — a GPIO-matrix re-mux plus a two-block prefill inside the handler
//! blows the entry-delay margin at four coincident transmitters — so the
//! sequencing belongs to a thread on the ISR's own core. This module is that
//! thread's logic, chip-free so the host can test the real scheduler and Miri
//! can model all three actors (see `driver.rs`'s "pusher deployment" docs for
//! the actor contract this implements).
//!
//! # Shape
//!
//! * [`WireMailbox`] — one per wire, all atomics: the *only* state the
//!   posting core and the pusher share. The poster publishes a frame
//!   descriptor and sequence; the pusher publishes completion (with a
//!   [`WireOutcome`]) and close acks. The frame pointer crosses cores as an
//!   [`AtomicPtr`] on purpose — round-tripping it through an address-sized
//!   integer loses provenance.
//! * [`Pusher`] — the scheduler, owned by the pusher thread, everything else
//!   private and plain (no locks anywhere): per-wire started/in-flight
//!   bookkeeping, per-slot ownership, round-robin start order, the
//!   concurrency cap, and a [`PadOps`] callback pair for the chip's
//!   route/park pad operations.
//!
//! The firmware wraps [`Pusher::service`] in its idle loop (service until no
//! progress, then `waiti` behind a doorbell check) and implements [`PadOps`]
//! with GPIO-matrix writes; a host test wraps it in a plain loop against
//! [`MockRmt`](crate::MockRmt). Neither changes the scheduler.
//!
//! # Timeouts live with the poster
//!
//! This crate has no clock, so the pusher never times anything out. A poster
//! that has waited too long for [`WireMailbox::completed_outcome`] escalates
//! with [`WireMailbox::request_abort`]; the admission timeout of the old
//! inline path is subsumed by the same mechanism (a frame that never won a
//! slot is cancelled by the abort request rather than refused at start).

use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicU8, AtomicUsize};

use crate::driver::Ws281xDriver;
use crate::hw::RmtHw;

/// `active_channel` value meaning "no slot is carrying this wire".
const NO_CHANNEL: u8 = 0xFF;

/// Slot-table value meaning "no owner" / "no pad".
const NONE: u8 = 0xFF;

/// How a posted frame ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WireOutcome {
    /// Reached `tx_end` (truncated or not — the counters say which).
    Transmitted = 0,
    /// Aborted by [`WireMailbox::request_abort`] while on the wire.
    Aborted = 1,
    /// Never started: cancelled by an abort or close request while queued.
    Cancelled = 2,
    /// `start_frame` refused it (a defect — the pusher only starts on a slot
    /// it believes idle and configured).
    StartFailed = 3,
}

impl WireOutcome {
    fn from_u8(raw: u8) -> Self {
        match raw {
            0 => Self::Transmitted,
            1 => Self::Aborted,
            2 => Self::Cancelled,
            _ => Self::StartFailed,
        }
    }
}

/// `true` when sequence `a` is strictly after `b`, in wrapping u32 space.
///
/// u32 rather than u64 because the deployment target (Xtensa LX6) has no
/// 64-bit atomics; at one frame per wire per ~30 ms the space wraps after
/// ~4 years of uptime, and the wrapping compare stays correct across it.
#[inline]
fn seq_after(a: u32, b: u32) -> bool {
    a.wrapping_sub(b) as i32 > 0
}

/// One wire's shared mailbox: the complete poster⇄pusher surface.
///
/// **Single poster per wire** — the posting side is a single-threaded
/// executor holding one output per wire, and the sequence arithmetic leans on
/// it (posts are not atomic increments). The pusher is likewise the only
/// writer of `completed_seq`/`close_ack_seq`.
#[derive(Debug)]
pub struct WireMailbox {
    frame_ptr: AtomicPtr<u8>,
    frame_len: AtomicUsize,
    gpio: AtomicU8,
    /// Last frame the poster published. Store is `Release`: it is the
    /// publish edge for the descriptor fields above.
    posted_seq: AtomicU32,
    /// Last frame the pusher fully disposed of (any [`WireOutcome`]). Store
    /// is `Release`: the poster's `Acquire` of it is what makes the frame
    /// bytes reusable — see the completion-forwarding leg in `state.rs`.
    completed_seq: AtomicU32,
    /// Outcome of the `completed_seq` frame. Sound because at most one frame
    /// per wire is outstanding (the poster's wait-before-post discipline).
    result: AtomicU8,
    /// Poster: dispose of every frame up to and including this sequence,
    /// aborting it off the wire if it is transmitting. Hang recovery.
    abort_req_seq: AtomicU32,
    /// Poster: quiesce the wire — cancel queued, abort in-flight — through
    /// this sequence, then ack. The teardown edge of the wire lifecycle.
    close_req_seq: AtomicU32,
    close_ack_seq: AtomicU32,
    /// The slot channel currently carrying this wire, [`NO_CHANNEL`] when
    /// idle. Advisory (`Relaxed`): exists solely for the poster's defensive
    /// abort on the pusher-wedged path, which is already a defect state.
    active_channel: AtomicU8,
}

impl WireMailbox {
    /// An idle mailbox. `const` so a firmware can hold a `static` array.
    pub const fn new() -> Self {
        Self {
            frame_ptr: AtomicPtr::new(core::ptr::null_mut()),
            frame_len: AtomicUsize::new(0),
            gpio: AtomicU8::new(0),
            posted_seq: AtomicU32::new(0),
            completed_seq: AtomicU32::new(0),
            result: AtomicU8::new(WireOutcome::Transmitted as u8),
            abort_req_seq: AtomicU32::new(0),
            close_req_seq: AtomicU32::new(0),
            close_ack_seq: AtomicU32::new(0),
            active_channel: AtomicU8::new(NO_CHANNEL),
        }
    }

    /// Publish a frame for the pusher and return its sequence number.
    ///
    /// The caller must not post again until [`Self::completed_outcome`]
    /// reports this sequence done (or a close is acked) — one outstanding
    /// frame per wire is a load-bearing invariant, not a courtesy.
    ///
    /// # Safety
    ///
    /// `ptr..ptr+len` must stay alive, in place, and unmodified until this
    /// sequence completes ([`Self::completed_outcome`]), the frame is
    /// disposed of by an acked [`Self::request_close`], or an
    /// [`Self::request_abort`] covering it completes. This is
    /// `start_frame`'s byte contract, transferred through the mailbox.
    pub unsafe fn post(&self, gpio: u8, ptr: *const u8, len: usize) -> u32 {
        let seq = self.posted_seq.load(Relaxed).wrapping_add(1);
        self.frame_ptr.store(ptr.cast_mut(), Relaxed);
        self.frame_len.store(len, Relaxed);
        self.gpio.store(gpio, Relaxed);
        // The publish edge: everything above must be visible before the
        // pusher can observe the new sequence.
        self.posted_seq.store(seq, Release);
        seq
    }

    /// Has frame `seq` been disposed of, and how?
    ///
    /// `Acquire`: a `Some` answer is the poster's licence to reuse the frame
    /// bytes.
    pub fn completed_outcome(&self, seq: u32) -> Option<WireOutcome> {
        if seq_after(seq, self.completed_seq.load(Acquire)) {
            None
        } else {
            Some(WireOutcome::from_u8(self.result.load(Relaxed)))
        }
    }

    /// Ask the pusher to dispose of every frame up to `seq` (aborting an
    /// in-flight one). The poster's hang-recovery escalation; completion
    /// still arrives through [`Self::completed_outcome`].
    pub fn request_abort(&self, seq: u32) {
        self.abort_req_seq.store(seq, Release);
    }

    /// Ask the pusher to quiesce the wire entirely. Returns the sequence the
    /// ack must reach; poll [`Self::close_acked`] with it. After the ack the
    /// wire may be reused (sequences continue, nothing resets).
    pub fn request_close(&self) -> u32 {
        let seq = self.posted_seq.load(Relaxed);
        self.close_req_seq.store(seq, Release);
        seq
    }

    /// Has the pusher acked a close request through `seq`?
    pub fn close_acked(&self, seq: u32) -> bool {
        !seq_after(seq, self.close_ack_seq.load(Acquire))
    }

    /// The slot channel carrying this wire right now, if any. Advisory —
    /// for the defensive abort on a wedged pusher, nothing else.
    pub fn active_channel(&self) -> Option<u8> {
        match self.active_channel.load(Relaxed) {
            NO_CHANNEL => None,
            ch => Some(ch),
        }
    }

    /// The frame the poster last published, if the pusher has not disposed
    /// of it. Pusher-side helper.
    fn pending_for_pusher(&self, started: u32) -> Option<(u32, *const u8, usize, u8)> {
        let posted = self.posted_seq.load(Acquire);
        if !seq_after(posted, started) {
            return None;
        }
        Some((
            posted,
            self.frame_ptr.load(Relaxed),
            self.frame_len.load(Relaxed),
            self.gpio.load(Relaxed),
        ))
    }
}

impl Default for WireMailbox {
    fn default() -> Self {
        Self::new()
    }
}

/// The chip's pad operations, called only from the pusher thread.
///
/// `route_to` points slot `slot_channel`'s output signal at pad `gpio`;
/// `park` returns a pad to plain-GPIO, output-enabled, solid low. On the
/// classic ESP32 these are GPIO-matrix writes (`fw-esp32v3`'s
/// `route_rmt_to_gpio`/`park_gpio`); in host tests they are recorders.
pub trait PadOps {
    fn route_to(&mut self, slot_channel: u8, gpio: u8);
    fn park(&mut self, gpio: u8);
}

/// Per-slot scheduling state, private to the pusher.
#[derive(Debug, Clone, Copy)]
struct SlotState {
    /// The RMT channel this slot is.
    channel: u8,
    /// Wire index whose frame the slot last carried ([`NONE`] fresh).
    owner_wire: u8,
    /// Pad currently routed to this slot's signal ([`NONE`] fresh).
    bound_gpio: u8,
    /// A started frame has not yet been observed complete.
    busy: bool,
}

/// The scheduler: owned and driven by the pusher thread, nothing shared.
///
/// `N` is the driver's channel count, `W` the wire count (mailbox array
/// length). Slots are the subset of channels the block plan gave memory to,
/// passed at construction; `cap` bounds concurrent transmissions (the ISR
/// duty budget — 4 on the dual-core classic).
pub struct Pusher<'d, H: RmtHw, P: PadOps, const N: usize, const W: usize> {
    driver: &'d Ws281xDriver<H, N>,
    mailboxes: &'d [WireMailbox; W],
    pads: P,
    cap: usize,
    slots: [SlotState; N],
    slot_count: usize,
    /// Highest sequence disposed of per wire (started, cancelled, failed).
    started: [u32; W],
    /// Wire has a started, not-yet-completed frame, and on which channel.
    wire_channel: [u8; W],
    /// Round-robin scan origin for starts, so no wire starves behind
    /// lower-indexed siblings at two-wave occupancy.
    rr_next: usize,
    transmitting: usize,
}

impl<'d, H: RmtHw, P: PadOps, const N: usize, const W: usize> Pusher<'d, H, P, N, W> {
    /// A pusher over `slots` (channel numbers the plan configured), starting
    /// with every slot unowned and every wire idle.
    ///
    /// `cap` is clamped to the slot count; a zero cap schedules nothing.
    pub fn new(
        driver: &'d Ws281xDriver<H, N>,
        mailboxes: &'d [WireMailbox; W],
        pads: P,
        slot_channels: &[u8],
        cap: usize,
    ) -> Self {
        let mut slots = [SlotState {
            channel: 0,
            owner_wire: NONE,
            bound_gpio: NONE,
            busy: false,
        }; N];
        let slot_count = slot_channels.len().min(N);
        for (slot, &channel) in slots.iter_mut().zip(slot_channels) {
            slot.channel = channel;
        }
        Self {
            driver,
            mailboxes,
            pads,
            cap: cap.min(slot_count),
            slots,
            slot_count,
            started: [0; W],
            wire_channel: [NO_CHANNEL; W],
            rr_next: 0,
            transmitting: 0,
        }
    }

    /// One scheduling pass: harvest completions, service close and abort
    /// requests, then start queued frames onto free slots up to the cap.
    /// Returns whether anything happened — the firmware idles (`waiti`
    /// behind its doorbell) only after a pass that returns `false`.
    pub fn service(&mut self) -> bool {
        let mut progress = false;
        progress |= self.harvest_completions();
        progress |= self.service_requests();
        progress |= self.start_queued();
        progress
    }

    /// Are any frames on the wire right now? (Firmware uses this to decide
    /// how eagerly to idle; tests use it to drain.)
    pub fn transmitting(&self) -> usize {
        self.transmitting
    }

    fn harvest_completions(&mut self) -> bool {
        let mut progress = false;
        for wire in 0..W {
            let ch = self.wire_channel[wire];
            if ch != NO_CHANNEL && self.driver.is_complete(ch) {
                // The Acquire inside `is_complete` pairs with the ISR's
                // Release in `finish`; the Release in `finish_wire` then
                // forwards the whole chain to the poster, which may free
                // the bytes on observing it. See state.rs, "completion
                // forwarding".
                self.release_wire_slot(wire);
                self.finish_wire(wire, self.started[wire], WireOutcome::Transmitted);
                progress = true;
            }
        }
        progress
    }

    fn service_requests(&mut self) -> bool {
        let mut progress = false;
        // Copy the `'d` reference out of `self` so the mailbox borrows do
        // not pin `self` across the `&mut self` calls below.
        let mailboxes = self.mailboxes;
        for (wire, mailbox) in mailboxes.iter().enumerate() {
            let close_req = mailbox.close_req_seq.load(Acquire);
            if seq_after(close_req, mailbox.close_ack_seq.load(Relaxed)) {
                self.dispose_through(wire, close_req, WireOutcome::Cancelled);
                // A close is wire teardown: the poster releases the pad's
                // lease once the ack lands, so every binding to this wire's
                // pad must be forgotten NOW — a later takeover parking a pad
                // whose lease moved on would drive someone else's pin. The
                // pad is not parked here: the poster parks it itself after
                // the ack, while it still holds the lease.
                self.forget_wire_pads(wire);
                mailbox.close_ack_seq.store(close_req, Release);
                progress = true;
            }

            let abort_req = mailbox.abort_req_seq.load(Acquire);
            if seq_after(abort_req, self.started[wire])
                || (self.wire_channel[wire] != NO_CHANNEL
                    && !seq_after(self.started[wire], abort_req))
            {
                self.dispose_through(wire, abort_req, WireOutcome::Cancelled);
                progress = true;
            }
        }
        progress
    }

    /// Dispose of every frame on `wire` up to `seq`: abort one off the wire
    /// ([`WireOutcome::Aborted`]), drop a queued one as `queued_outcome`.
    /// A close also cancels posted-but-unstarted frames — acking a close and
    /// then starting the frame it covered would hand the transmitter freed
    /// bytes (the queued→closing rule the Miri harness encodes).
    fn dispose_through(&mut self, wire: usize, seq: u32, queued_outcome: WireOutcome) {
        let ch = self.wire_channel[wire];
        if ch != NO_CHANNEL && !seq_after(self.started[wire], seq) {
            // In flight and covered: off the wire first. From the pusher
            // thread the abort handshake is the safe teardown edge; the
            // frame bytes may be reclaimed the moment the poster sees the
            // completion this publishes.
            self.driver.abort(ch);
            self.release_wire_slot(wire);
            self.finish_wire(wire, self.started[wire], WireOutcome::Aborted);
        }
        // Anything still queued at or below `seq` is disposed of unstarted.
        if seq_after(seq, self.started[wire]) {
            self.finish_wire(wire, seq, queued_outcome);
        }
    }

    fn start_queued(&mut self) -> bool {
        let mut progress = false;
        let mailboxes = self.mailboxes;
        // The scan origin is captured once: `rr_next` moves as starts land,
        // and folding that movement into the scan index skips wires.
        let origin = self.rr_next;
        let mut scanned = 0;
        while self.transmitting < self.cap && scanned < W {
            let wire = (origin + scanned) % W;
            scanned += 1;
            if self.wire_channel[wire] != NO_CHANNEL {
                continue;
            }
            let mailbox = &mailboxes[wire];
            let Some((seq, ptr, len, gpio)) = mailbox.pending_for_pusher(self.started[wire]) else {
                continue;
            };
            // Covered by a pending close/abort: dispose, never start.
            let close_req = mailbox.close_req_seq.load(Acquire);
            let abort_req = mailbox.abort_req_seq.load(Acquire);
            if !seq_after(seq, close_req) || !seq_after(seq, abort_req) {
                self.finish_wire(wire, seq, WireOutcome::Cancelled);
                progress = true;
                continue;
            }
            let Some(slot_idx) = self.acquire_slot(wire as u8, gpio) else {
                // Every slot busy: a completion interrupt re-runs this pass.
                continue;
            };
            let ch = self.slots[slot_idx].channel;
            // SAFETY: forwarding `WireMailbox::post`'s contract — the poster
            // keeps the bytes alive until this sequence completes through
            // the mailbox.
            let started = unsafe {
                self.driver
                    .start_frame(ch, core::slice::from_raw_parts(ptr, len))
            };
            match started {
                Ok(()) => {
                    self.slots[slot_idx].busy = true;
                    self.wire_channel[wire] = ch;
                    self.started[wire] = seq;
                    self.transmitting += 1;
                    mailbox.active_channel.store(ch, Relaxed);
                    // Fairness: the next scan starts after this wire.
                    self.rr_next = (wire + 1) % W;
                }
                Err(_) => {
                    // Defect-grade surprise (the slot was believed idle);
                    // surface it to the poster rather than wedging the wire.
                    self.finish_wire(wire, seq, WireOutcome::StartFailed);
                }
            }
            progress = true;
        }
        progress
    }

    /// Find a slot for `wire`/`gpio` and route the pad. Preference order —
    /// the slot this wire already owns (zero matrix writes in the steady
    /// four-wire state), an unowned slot, then takeover of any idle slot. A
    /// busy slot is never taken.
    fn acquire_slot(&mut self, wire: u8, gpio: u8) -> Option<usize> {
        let usable = &self.slots[..self.slot_count];
        let mine = usable.iter().position(|s| s.owner_wire == wire && !s.busy);
        let pick = mine
            .or_else(|| usable.iter().position(|s| s.owner_wire == NONE && !s.busy))
            .or_else(|| usable.iter().position(|s| !s.busy))?;
        let slot = &mut self.slots[pick];
        if slot.owner_wire != wire || slot.bound_gpio != gpio {
            // Takeover: park the displaced pad first (its strand then holds
            // its latched frame), then point the slot's signal at ours.
            if slot.bound_gpio != NONE && slot.bound_gpio != gpio {
                self.pads.park(slot.bound_gpio);
            }
            self.pads.route_to(slot.channel, gpio);
            slot.bound_gpio = gpio;
            slot.owner_wire = wire;
        }
        Some(pick)
    }

    /// Mark `wire`'s slot no longer busy (frame completed or aborted).
    /// Ownership and routing stay — the steady-state preference.
    fn release_wire_slot(&mut self, wire: usize) {
        let ch = self.wire_channel[wire];
        if ch == NO_CHANNEL {
            return;
        }
        if let Some(slot) = self.slots[..self.slot_count]
            .iter_mut()
            .find(|s| s.channel == ch)
        {
            slot.busy = false;
        }
        self.wire_channel[wire] = NO_CHANNEL;
        self.transmitting -= 1;
        self.mailboxes[wire]
            .active_channel
            .store(NO_CHANNEL, Relaxed);
    }

    /// Drop every slot binding to `wire`'s pad (close-time teardown). The
    /// slots stay usable — merely unowned, their pads unbound.
    fn forget_wire_pads(&mut self, wire: usize) {
        for slot in &mut self.slots[..self.slot_count] {
            if slot.owner_wire == wire as u8 {
                slot.owner_wire = NONE;
                slot.bound_gpio = NONE;
            }
        }
    }

    /// Publish `wire`'s disposal of `seq` as `outcome`. The `Release` is the
    /// poster's licence to reuse the frame bytes.
    fn finish_wire(&mut self, wire: usize, seq: u32, outcome: WireOutcome) {
        self.started[wire] = seq;
        let mailbox = &self.mailboxes[wire];
        mailbox.result.store(outcome as u8, Relaxed);
        mailbox.completed_seq.store(seq, Release);
    }
}
