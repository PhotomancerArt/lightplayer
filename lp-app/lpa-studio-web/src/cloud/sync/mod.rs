//! Auto-publish: a signed-in account's projects exist in the cloud without
//! anybody asking for it (vision D2/D3/D7).
//!
//! There is no "share" button in this slice and no sync affordance in the
//! product surfaces. A project is created, renamed or saved; some time
//! later the service holds it, and the address already in the bar is the
//! link. The one place outcomes are visible is diagnostic: the driver
//! records every trip's conclusion in [`sync_status`] and the `/account`
//! page renders the ledger. Five pieces:
//!
//! - [`sync_queue`] — *when*. The debounce, the retry cadence, and the
//!   in-flight bookkeeping, as a pure state machine.
//! - [`sidecar_producer`] — *what the listing says*. Name and format off the
//!   container manifest; no preview (see that module's docs).
//! - [`sync_trip`] — *one attempt*. Publish or push, runtime-neutral, tested
//!   against the in-process service.
//! - [`sync_status`] — *what just happened*. The per-tab ledger of trip
//!   conclusions, including the ones that make no network traffic.
//! - `sync_engine` (wasm only) — the driver that wires the four to the OPFS
//!   library, `FetchCloudPort`, and the browser's timers.
//!
//! # The rules that do not bend
//!
//! **Never block a save on the network.** Every trip runs off the UI's
//! critical path; the library is the source of truth and the cloud is a
//! place it syncs with, so an unreachable service is silence, not an error.
//!
//! **Signed out is a no-op.** Not a queue that drains at sign-in — the
//! sign-in sweep re-derives everything worth doing from OPFS, which is why
//! nothing here is persisted.

pub mod sidecar_producer;
pub mod sync_queue;
pub mod sync_status;
pub mod sync_trip;

#[cfg(target_arch = "wasm32")]
pub mod sync_engine;

use super::CloudSession;

/// Whether this session syncs at all.
///
/// Only a signed-in account has a cloud to converge on. `Pending` is not yet
/// an answer and `Unreachable` is the offline case wearing a session state;
/// both stay quiet, and the sweep at sign-in catches up whatever was missed.
pub fn syncs(session: &CloudSession) -> bool {
    session.me().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_signed_in_session_syncs() {
        assert!(!syncs(&CloudSession::Pending));
        assert!(!syncs(&CloudSession::Unreachable));
        assert!(!syncs(&CloudSession::Anonymous { options: None }));
    }
}
