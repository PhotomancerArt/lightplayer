//! The update channel (lp-link channel 3) and the core-only loop.
//!
//! The host's words (`lp-cli`'s `OtaServe`), all little-endian:
//! - host → board `O` core_len:u32 engine_len:u32 build_id:[u8;48] — an offer
//! - board → host `Q` — "what do you have?" (answered with an offer)
//! - board → host `R` kind:u8 offset:u32 len:u32 — a request (`C`ore/`E`ngine)
//! - host → board `D` kind:u8 offset:u32 bytes… — the answer
//!
//! One request is in flight at a time; every chunk is one flash sector.
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

struct Offer {
    core_len: u32,
    engine_len: u32,
    build_id: [u8; 48],
}

impl Offer {
    fn parse(m: &[u8]) -> Option<Self> {
        if m.len() != 57 || m[0] != b'O' {
            return None;
        }
        let mut build_id = [0u8; 48];
        build_id.copy_from_slice(&m[9..57]);
        Some(Self {
            core_len: u32::from_le_bytes(m[1..5].try_into().ok()?),
            engine_len: u32::from_le_bytes(m[5..9].try_into().ok()?),
            build_id,
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

/// Installed into the engine's link transport while the engine runs: an
/// offer of a different build erases the engine's header and resets, so the
/// next boot is core-only. The flash is the "update pending" state — nothing
/// in RAM, and a cut right after the erase lands in the same place. Called
/// from engine code; it lives in core, which the engine may always call.
pub fn on_update_while_running(data: &[u8]) {
    if let Some(offer) = Offer::parse(data)
        && offer.build_id != crate::build_id()
    {
        let mut flash = SplitFlash::take();
        let state = BootState::read(&mut flash);
        flash.erase(state.engine_extent().start);
        esp_println::println!(
            "[OTA] offer of a different build — engine erased, resetting into core-only"
        );
        system_reset();
    }
}

/// The core-only loop: keep the board reachable, confirm a trial core once
/// its link is up, take an offer and do what it asks. Never returns; every
/// completed step ends in a reset.
pub async fn core_only(
    usb_link: &'static UsbLinkShared,
    mut watchdog: crate::recovery::watchdog::WatchdogFeeder,
    mut state: BootState,
    engine_crashing: bool,
) -> ! {
    let mut flash = SplitFlash::take();
    let mut buf = Box::new(SectorBuf([0xff; SECTOR as usize]));
    let mut plan = Plan::Idle;
    let mut queried = false;
    // Core-only is a complete boot for the recovery ledger — unless the core
    // is here because the engine keeps crashing, in which case the count
    // must stay up so the next boot stays here too.
    if !engine_crashing {
        lp_recovery::mark_boot_complete();
    }
    esp_println::println!(
        "[OTA] core-only: core @{:#x} ({} B), engine room {} B",
        state.core_off,
        state.core_len,
        state.engine_extent().len()
    );
    loop {
        watchdog.feed(embassy_time::Instant::now().as_millis());
        if usb_link.is_established() {
            // A trial core's proof of life: it booted, ran its radios and got
            // a host on its link. Nothing else happens before this.
            state.confirm(&mut flash);
            if !queried {
                queried = send(usb_link, &[b'Q']);
            }
        }
        while let Some(event) = usb_link.with_link(|link| link.recv()) {
            match event {
                LinkEvent::Up { .. } => queried = false,
                LinkEvent::Message { channel, data } if channel == CH_UPDATE => {
                    plan = step(plan, &data, &state, &mut flash, &mut buf, usb_link);
                }
                _ => {}
            }
        }
        embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
    }
}

fn step(
    plan: Plan,
    msg: &[u8],
    state: &BootState,
    flash: &mut SplitFlash,
    buf: &mut SectorBuf,
    usb_link: &'static UsbLinkShared,
) -> Plan {
    if let Some(offer) = Offer::parse(msg) {
        if plan != Plan::Idle || state.on_trial() {
            return plan; // mid-transfer (a link re-up), or not confirmed yet
        }
        if offer.build_id != crate::build_id() {
            let Some(dest) =
                state
                    .layout
                    .next_core_offset(state.core_off, state.core_len, offer.core_len)
            else {
                esp_println::println!(
                    "[OTA] new core ({} B) does not fit beside this one ({} B) — refused",
                    offer.core_len,
                    state.core_len
                );
                return plan;
            };
            // The engine dies first, so no cut from here on leaves this core
            // starting an engine whose region is half overwritten.
            flash.erase(state.engine_extent().start);
            esp_println::println!(
                "[OTA] new build: core {} B → {dest:#x}, engine erased",
                offer.core_len
            );
            request(usb_link, b'C', 0, offer.core_len);
            return Plan::Core {
                dest,
                len: offer.core_len,
                next: 0,
            };
        }
        // Same build, and core-only: no valid engine here. Fetch it.
        let room = state.engine_extent();
        if offer.engine_len > room.len() {
            esp_println::println!(
                "[OTA] engine ({} B) does not fit ({} B) — refused",
                offer.engine_len,
                room.len()
            );
            return plan;
        }
        flash.erase(room.start);
        esp_println::println!(
            "[OTA] same build, no engine: fetching {} B → {:#x}",
            offer.engine_len,
            room.start
        );
        let first = if offer.engine_len > SECTOR { SECTOR } else { 0 };
        request(usb_link, b'E', first, offer.engine_len);
        return Plan::Engine {
            dest: room.start,
            len: offer.engine_len,
            next: first,
        };
    }
    if msg.len() < 6 || msg[0] != b'D' {
        return plan;
    }
    let kind = msg[1];
    let off = u32::from_le_bytes(msg[2..6].try_into().unwrap_or_default());
    let data = &msg[6..];
    match plan {
        Plan::Core { dest, len, next } if kind == b'C' && off == next => {
            flash.write_sector(buf, dest + off, data);
            let next = off + data.len() as u32;
            if next < len {
                request(usb_link, b'C', next, len);
                return Plan::Core { dest, len, next };
            }
            state.write_trial_record(flash, buf, dest, len);
            esp_println::println!(
                "[OTA] core written ({len} B) and named on trial — resetting into it"
            );
            system_reset();
        }
        Plan::Engine { dest, len, next } if kind == b'E' && off == next => {
            flash.write_sector(buf, dest + off, data);
            if off == 0 {
                // A new engine deserves a fair start: clear the crash count.
                lp_recovery::mark_boot_complete();
                esp_println::println!("[OTA] engine written ({len} B), header last — resetting");
                system_reset();
            }
            let after = off + data.len() as u32;
            let next = if after < len { after } else { 0 };
            request(usb_link, b'E', next, len);
            Plan::Engine { dest, len, next }
        }
        _ => plan,
    }
}

fn request(usb_link: &'static UsbLinkShared, kind: u8, off: u32, total: u32) {
    let len = SECTOR.min(total - off);
    let mut m = Vec::with_capacity(10);
    m.extend_from_slice(&[b'R', kind]);
    m.extend_from_slice(&off.to_le_bytes());
    m.extend_from_slice(&len.to_le_bytes());
    send(usb_link, &m);
}

fn send(usb_link: &'static UsbLinkShared, m: &[u8]) -> bool {
    let ok = usb_link.with_link(|link| link.send(CH_UPDATE, m).is_ok());
    usb_link.ring();
    ok
}
