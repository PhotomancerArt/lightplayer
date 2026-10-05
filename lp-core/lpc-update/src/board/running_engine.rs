//! The running engine's subset of the session (DM13).
//!
//! While the engine runs, the session is its channel-3 hook:
//!
//! - `Q` → `M` (state `running`);
//! - `G` → read-back ([`super::read_back`]);
//! - `O`:
//!   - a **core install** (by hashes) passes the same checks as core-only
//!     (flags first, then the boot state, chip/layout/loader, access, the
//!     refused build, fit). The engine then writes the progress record with
//!     stage **pending**, erases its own engine header, and asks for a reset:
//!     "the flash is the update-pending state". Core-only boots, finds its
//!     own pending transfer, and waits for a host holding that build;
//!   - an **engine install of its own, valid engine** (both hashes its own)
//!     is a no-op: the session answers `M`;
//! - an unknown host message type → `N`/`U`, as in core-only; any other known
//!   message that does not apply while the engine runs (`D`, `Z`, `L`) is
//!   ignored. The server's own login holds the link's tier
//!   ([`BoardSession::on_message_with_tier`]).
//!
//! **A cut between the record and the header erase** leaves a valid engine
//! beside a pending record. The firmware boots the valid engine (a valid
//! engine wins), whose session clears the record at start
//! ([`BoardSession::new`]): the offer is simply lost, and the host offers
//! again.

use crate::transfer_record::{RecordStage, TransferRecord};

use super::board_session::BoardSession;
use super::session_output::Effect;
use super::update_target::UpdateTarget;

impl BoardSession {
    /// Accept a core install while the engine runs: pending record, header
    /// erased, reset — in that order.
    pub(super) fn hand_over_to_core_only<T: UpdateTarget>(
        &mut self,
        target: &mut T,
        record: TransferRecord,
    ) {
        let record = TransferRecord {
            stage: RecordStage::Pending,
            ..record
        };
        let addr = self.facts.progress_record_addr;
        let done = target
            .erase_sector(addr)
            .and_then(|()| target.program(addr, &record.encode_header()))
            .and_then(|()| target.erase_engine_header());
        if done.is_err() {
            return self.push_effect(Effect::FlashFault);
        }
        self.engine_valid = false;
        self.reset();
    }
}
