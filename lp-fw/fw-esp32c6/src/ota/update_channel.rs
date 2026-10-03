//! The update channel (lp-link channel 3) and the core-only loop.
//!
//! The host's words (`lp-cli`'s `OtaServe`), all little-endian:
//! - host → board `O` core_len:u32 engine_len:u32 build_id:[u8;48]
//!   [ticket:[u8;16]] — an offer; the ticket is required over radio
//!   ([`super::update_ticket`])
//! - board → host `Q` — "what do you have?" (answered with an offer)
//! - board → host `R` kind:u8 offset:u32 len:u32 [flags:u8] — a request
//!   (`C`ore/`E`ngine); flag bit 0: this core takes `Z`
//! - host → board `D` kind:u8 offset:u32 bytes… — the answer
//! - host → board `Z` kind:u8 offset:u32 deflate… — the answer compressed:
//!   raw deflate against the 32 KiB written before it
//!   ([`super::update_window`] has the rule); a host sends `Z` only to a
//!   request that set the flag, and only when it is smaller
//! - board → host `F` build:u32 — refused: that build failed its trial here
//! - board → host `A` — refused: an untrusted link must log in (engine
//!   running) or bring this board's ticket (core-only)
//!
//! The board asks for one chunk at a time, and every chunk is one flash
//! sector. A host may send chunks AHEAD of the request: the board takes a `D`
//! only for the sector it is waiting for (anything else is ignored), and
//! lp-link's flow control bounds what waits in its inbox. Over BLE that is
//! what keeps the radio busy while the board erases and programs. The
//! transfer belongs to the link that asked for it; an offer of the same
//! build from another link (a reconnect after a drop) takes it over where it
//! stopped.
//!
//! **This is a contract a board in the field keeps forever**: an old core
//! must be able to take a new one. Change it only by adding.

use alloc::boxed::Box;
use alloc::vec::Vec;

use fw_esp32_common::usb_link::UsbLinkShared;
use lpc_wire::lp_link::{CH_UPDATE, LinkEvent};

use super::boot_state::BootState;
use super::split_flash::{SECTOR, SectorBuf, SplitFlash};
use super::system_reset::system_reset;
use super::update_ticket::{self, Ticket};
use super::update_window::UpdateWindow;

#[cfg(feature = "ble")]
use fw_esp32_common::radio_link::{RadioLinkEvent, RadioLinkPort};
#[cfg(feature = "ble")]
use lpc_shared::transport::LinkId;

struct Offer {
    core_len: u32,
    engine_len: u32,
    build_id: [u8; 48],
    ticket: Option<Ticket>,
}

impl Offer {
    fn parse(m: &[u8]) -> Option<Self> {
        if !(m.len() == 57 || m.len() == 73) || m[0] != b'O' {
            return None;
        }
        let mut build_id = [0u8; 48];
        build_id.copy_from_slice(&m[9..57]);
        let ticket = (m.len() == 73).then(|| {
            let mut t = [0u8; 16];
            t.copy_from_slice(&m[57..73]);
            t
        });
        Some(Self {
            core_len: u32::from_le_bytes(m[1..5].try_into().ok()?),
            engine_len: u32::from_le_bytes(m[5..9].try_into().ok()?),
            build_id,
            ticket,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Plan {
    Idle,
    /// Writing the new core at `dest`, front to back.
    Core {
        dest: u32,
        len: u32,
        next: u32,
        /// `lp_bootctl::build_hash` of the offered build, for its record.
        build: u32,
    },
    /// Writing this build's engine at `dest`: sector 1 onward, then sector 0
    /// (its header) LAST, so a cut never leaves a valid header over a
    /// partial engine.
    Engine {
        dest: u32,
        len: u32,
        next: u32,
    },
}

/// Which link a message came on, and where an answer goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Peer {
    /// The USB cable: trusted.
    Usb,
    /// A radio connection: untrusted.
    #[cfg(feature = "ble")]
    Radio { link: LinkId, slot: usize },
}

impl Peer {
    fn trusted(self) -> bool {
        matches!(self, Peer::Usb)
    }
}

/// Every link the core can answer on.
struct Links {
    usb: &'static UsbLinkShared,
    #[cfg(feature = "ble")]
    radio: Option<&'static RadioLinkPort>,
}

impl Links {
    fn send(&self, peer: Peer, m: &[u8]) -> bool {
        match peer {
            Peer::Usb => {
                let ok = self.usb.with_link(|link| link.send(CH_UPDATE, m).is_ok());
                self.usb.ring();
                ok
            }
            #[cfg(feature = "ble")]
            Peer::Radio { link, slot } => {
                let Some(port) = self.radio else {
                    return false;
                };
                let slot = port.slot(slot);
                let ok = slot
                    .with_link(link, |l| l.send(CH_UPDATE, m).is_ok())
                    .unwrap_or(false);
                slot.ring();
                ok
            }
        }
    }
}

/// Installed into the engine's link transports while the engine runs: an
/// offer of a different build erases the engine's header and resets, so the
/// next boot is core-only. The flash is the "update pending" state — nothing
/// in RAM, and a cut right after the erase lands in the same place. An offer
/// carrying a ticket stores it first (the radio path: the transport already
/// checked the link's tier). Called from engine code; it lives in core,
/// which the engine may always call.
pub fn on_update_while_running(data: &[u8]) {
    if let Some(offer) = Offer::parse(data)
        && offer.build_id != crate::build_id()
    {
        let mut flash = SplitFlash::take();
        let state = BootState::read(&mut flash);
        if !state.healthy {
            return; // logged by `read`; never act on a state we cannot trust
        }
        flash.protect(state.core_extent());
        if let Some(ticket) = offer.ticket {
            let mut buf = Box::new(SectorBuf([0xff; SECTOR as usize]));
            if !update_ticket::write(&mut flash, &mut buf, &ticket) {
                return;
            }
        }
        if !flash.erase(state.engine_extent().start) {
            return;
        }
        super::say!("[OTA] offer of a different build — engine erased, resetting into core-only");
        system_reset();
    }
}

/// The core-only loop: keep the board reachable, confirm a trial core once
/// a link is up, take an offer and do what it asks. Never returns; every
/// completed step ends in a reset.
pub async fn core_only(
    usb_link: &'static UsbLinkShared,
    #[cfg(feature = "ble")] radio_port: Option<&'static RadioLinkPort>,
    mut watchdog: crate::recovery::watchdog::WatchdogFeeder,
    mut state: BootState,
    engine_crashing: bool,
) -> ! {
    let links = Links {
        usb: usb_link,
        #[cfg(feature = "ble")]
        radio: radio_port,
    };
    let mut flash = SplitFlash::take();
    if state.healthy {
        flash.protect(state.core_extent());
    }
    let ticket = update_ticket::read(&mut flash);
    let mut buf = Box::new(SectorBuf([0xff; SECTOR as usize]));
    let mut plan = Plan::Idle;
    let mut window: Option<UpdateWindow> = None;
    let mut source: Option<Peer> = None;
    let mut queried = false;
    #[cfg(feature = "ble")]
    let mut radios: Vec<(LinkId, usize)> = Vec::new();
    // Every radio link from here on takes the wide update window.
    #[cfg(feature = "ble")]
    fw_esp32_common::radio_link::set_update_mode(true);
    // Core-only is a complete boot for the recovery ledger — unless the core
    // is here because the engine keeps crashing, in which case the count
    // must stay up so the next boot stays here too.
    if !engine_crashing {
        lp_recovery::mark_boot_complete();
    }
    super::say!(
        "[OTA] core-only: core @{:#x} ({} B), engine room {} B{}",
        state.core_off,
        state.core_len,
        state.engine_extent().len(),
        if ticket.is_some() {
            ", holding an update ticket"
        } else {
            ""
        }
    );
    loop {
        watchdog.feed(embassy_time::Instant::now().as_millis());
        if usb_link.is_established() {
            // A trial core's proof of life: it booted, ran its radios and got
            // a host on its link. Nothing else happens before this.
            state.confirm(&mut flash);
            if !queried {
                queried = links.send(Peer::Usb, &[b'Q']);
            }
        }
        while let Some(event) = usb_link.with_link(|link| link.recv()) {
            match event {
                LinkEvent::Up { .. } => queried = false,
                LinkEvent::Message { channel, data } if channel == CH_UPDATE => {
                    plan = step(
                        plan,
                        &data,
                        Peer::Usb,
                        &mut source,
                        ticket,
                        &links,
                        &state,
                        &mut flash,
                        &mut buf,
                        &mut window,
                    );
                }
                _ => {}
            }
        }
        #[cfg(feature = "ble")]
        if let Some(port) = radio_port {
            while let Some(event) = port.try_event() {
                match event {
                    RadioLinkEvent::Opened { link, slot } => {
                        super::say!("[OTA] radio link {link} opened (slot {slot})");
                        radios.push((link, slot));
                    }
                    RadioLinkEvent::Closed { link } => {
                        super::say!("[OTA] radio link {link} closed");
                        radios.retain(|r| r.0 != link);
                    }
                }
            }
            for &(link, slot) in &radios {
                let peer = Peer::Radio { link, slot };
                while let Some(Some(event)) = port.slot(slot).with_link(link, |l| l.recv()) {
                    match event {
                        LinkEvent::Up { .. } => {
                            // Proof of life over radio, as over USB.
                            state.confirm(&mut flash);
                            links.send(peer, &[b'Q']);
                        }
                        LinkEvent::Message { channel, data } if channel == CH_UPDATE => {
                            plan = step(
                                plan,
                                &data,
                                peer,
                                &mut source,
                                ticket,
                                &links,
                                &state,
                                &mut flash,
                                &mut buf,
                                &mut window,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}

#[allow(clippy::too_many_arguments)]
fn step(
    plan: Plan,
    msg: &[u8],
    peer: Peer,
    source: &mut Option<Peer>,
    ticket: Option<Ticket>,
    links: &Links,
    state: &BootState,
    flash: &mut SplitFlash,
    buf: &mut SectorBuf,
    window: &mut Option<UpdateWindow>,
) -> Plan {
    if let Some(offer) = Offer::parse(msg) {
        if !peer.trusted() && (ticket.is_none() || offer.ticket != ticket) {
            super::say!("[OTA] offer over an untrusted link without this board's ticket — refused");
            links.send(peer, b"A");
            return plan;
        }
        // An offer that continues the transfer in progress (a reconnect, or
        // another link): it takes the transfer over where it stopped.
        match plan {
            Plan::Idle => {}
            Plan::Core {
                len, next, build, ..
            } if lp_bootctl::build_hash(&offer.build_id) == build
                && offer.core_len == len =>
            {
                super::say!("[OTA] resuming the core at {next:#x}");
                *source = Some(peer);
                request(links, peer, b'C', next, len);
                return plan;
            }
            Plan::Engine { len, next, .. }
                if offer.build_id == crate::build_id() && offer.engine_len == len =>
            {
                super::say!("[OTA] resuming the engine at {next:#x}");
                *source = Some(peer);
                request(links, peer, b'E', next, len);
                return plan;
            }
            _ => return plan,
        }
        if state.on_trial() || !state.healthy {
            return plan; // not confirmed yet, or a state we cannot trust
        }
        let build = lp_bootctl::build_hash(&offer.build_id);
        if state.failed_build == Some(build) {
            super::say!("[OTA] that build failed its trial on this board — refused");
            let mut m = Vec::with_capacity(5);
            m.push(b'F');
            m.extend_from_slice(&build.to_le_bytes());
            links.send(peer, &m);
            return plan;
        }
        if offer.build_id != crate::build_id() {
            let Some(dest) =
                state
                    .layout
                    .next_core_offset(state.core_off, state.core_len, offer.core_len)
            else {
                super::say!(
                    "[OTA] new core ({} B) does not fit beside this one ({} B) — refused",
                    offer.core_len,
                    state.core_len
                );
                return plan;
            };
            // The engine dies first, so no cut from here on leaves this core
            // starting an engine whose region is half overwritten.
            if !flash.erase(state.engine_extent().start) {
                return plan;
            }
            super::say!(
                "[OTA] new build: core {} B → {dest:#x}, engine erased",
                offer.core_len
            );
            *source = Some(peer);
            request(links, peer, b'C', 0, offer.core_len);
            return Plan::Core {
                dest,
                len: offer.core_len,
                next: 0,
                build,
            };
        }
        // Same build, and core-only: no valid engine here. Fetch it.
        let room = state.engine_extent();
        if offer.engine_len > room.len() {
            super::say!(
                "[OTA] engine ({} B) does not fit ({} B) — refused",
                offer.engine_len,
                room.len()
            );
            return plan;
        }
        if !flash.erase(room.start) {
            return plan;
        }
        super::say!(
            "[OTA] same build, no engine: fetching {} B → {:#x}",
            offer.engine_len,
            room.start
        );
        let first = if offer.engine_len > SECTOR { SECTOR } else { 0 };
        *source = Some(peer);
        request(links, peer, b'E', first, offer.engine_len);
        return Plan::Engine {
            dest: room.start,
            len: offer.engine_len,
            next: first,
        };
    }
    // Data belongs to the link that asked for it: `D` raw, `Z` compressed
    // against the window (asked for with the request's flag).
    if *source != Some(peer) || msg.len() < 6 || !(msg[0] == b'D' || msg[0] == b'Z') {
        return plan;
    }
    let kind = msg[1];
    let off = u32::from_le_bytes(msg[2..6].try_into().unwrap_or_default());
    let (dest, len, next) = match plan {
        Plan::Core { dest, len, next, .. } if kind == b'C' => (dest, len, next),
        Plan::Engine { dest, len, next } if kind == b'E' => (dest, len, next),
        _ => return plan,
    };
    if off != next {
        return plan; // ahead of or behind the sector waited for (a host sending ahead)
    }
    let expect = SECTOR.min(len - off) as usize;
    let wrote = if msg[0] == b'Z' {
        let w = window.get_or_insert_with(UpdateWindow::new);
        match w.decode(kind, off, dest, expect, &msg[6..], flash) {
            Some(data) => flash.write_sector(buf, dest + off, data),
            None => {
                // Ask for this one uncompressed instead.
                request_raw(links, peer, kind, off, len);
                return plan;
            }
        }
    } else {
        let ok = msg.len() - 6 == expect && flash.write_sector(buf, dest + off, &msg[6..]);
        if ok && let Some(w) = window.as_mut() {
            w.written(kind, off, expect, Some(&msg[6..]));
        }
        ok
    };
    if !wrote {
        super::say!("[OTA] write at {:#x} failed — update abandoned", dest + off);
        return Plan::Idle;
    }
    if msg[0] == b'Z'
        && let Some(w) = window.as_mut()
    {
        w.written(kind, off, expect, None);
    }
    let n = expect as u32;
    match plan {
        Plan::Core { build, .. } => {
            let next = off + n;
            if next < len {
                request(links, peer, b'C', next, len);
                return Plan::Core {
                    dest,
                    len,
                    next,
                    build,
                };
            }
            log_decoding(window);
            state.write_trial_record(flash, buf, dest, len, build);
            super::say!("[OTA] core written ({len} B) and named on trial — resetting into it");
            system_reset();
        }
        Plan::Engine { .. } => {
            if off == 0 {
                log_decoding(window);
                // The update the ticket authorized is done.
                update_ticket::clear(flash);
                // A new engine deserves a fair start: clear the crash count.
                lp_recovery::mark_boot_complete();
                super::say!("[OTA] engine written ({len} B), header last — resetting");
                system_reset();
            }
            let after = off + n;
            let next = if after < len { after } else { 0 };
            request(links, peer, b'E', next, len);
            Plan::Engine { dest, len, next }
        }
        Plan::Idle => plan,
    }
}

fn log_decoding(window: &Option<UpdateWindow>) {
    if let Some(w) = window
        && w.decoded > 0
    {
        super::say!(
            "[OTA] {} chunks arrived compressed; decoding took {} ms",
            w.decoded,
            w.decode_us / 1000
        );
    }
}

/// A request, with the flag that says this core takes `Z` (compressed) chunks.
fn request(links: &Links, peer: Peer, kind: u8, off: u32, total: u32) {
    send_request(links, peer, kind, off, total, true);
}

/// A request for a chunk uncompressed (its `Z` did not decode).
fn request_raw(links: &Links, peer: Peer, kind: u8, off: u32, total: u32) {
    send_request(links, peer, kind, off, total, false);
}

fn send_request(links: &Links, peer: Peer, kind: u8, off: u32, total: u32, inflate: bool) {
    let len = SECTOR.min(total - off);
    let mut m = Vec::with_capacity(11);
    m.extend_from_slice(&[b'R', kind]);
    m.extend_from_slice(&off.to_le_bytes());
    m.extend_from_slice(&len.to_le_bytes());
    m.push(u8::from(inflate));
    links.send(peer, &m);
}
