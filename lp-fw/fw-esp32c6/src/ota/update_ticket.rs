//! The update ticket (OTA spike): how an update offered over an untrusted
//! link survives the resets that leave the board without its login.
//!
//! Login lives in the engine (`lpa-server`'s access gate). The engine takes
//! an offer from a radio link only when that link holds the edit tier, and
//! only with a ticket: 16 bytes the host chose. It stores them here, then
//! erases its header and resets. From then on the board is core-only (or a
//! new core on trial), with no login at all, and it accepts an offer over an
//! untrusted link only when it carries this ticket. USB stays trusted and
//! needs none.
//!
//! The ticket lives until the update it authorized completes (the new engine
//! is written), so a reconnect, a power cut or a rollback mid-update can
//! still be finished over the same radio by the same host.

use lp_bootctl::UPDATE_TICKET_SECTOR;

use super::split_flash::{SectorBuf, SplitFlash};

pub type Ticket = [u8; 16];

const MAGIC: [u8; 4] = *b"LPTK";

/// The stored ticket, or `None` (erased, or not one).
pub fn read(flash: &mut SplitFlash) -> Option<Ticket> {
    let mut raw = [0u8; 20];
    if !flash.read(UPDATE_TICKET_SECTOR, &mut raw) || raw[..4] != MAGIC {
        return None;
    }
    let mut ticket = [0u8; 16];
    ticket.copy_from_slice(&raw[4..]);
    Some(ticket)
}

/// Store `ticket` (the fence must already be set).
pub fn write(flash: &mut SplitFlash, buf: &mut SectorBuf, ticket: &Ticket) -> bool {
    let mut raw = [0u8; 20];
    raw[..4].copy_from_slice(&MAGIC);
    raw[4..].copy_from_slice(ticket);
    flash.write_sector(buf, UPDATE_TICKET_SECTOR, &raw)
}

/// The update it authorized is done: forget the ticket.
pub fn clear(flash: &mut SplitFlash) {
    if read(flash).is_some() {
        flash.erase(UPDATE_TICKET_SECTOR);
    }
}
