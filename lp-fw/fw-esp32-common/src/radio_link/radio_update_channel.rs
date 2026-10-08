//! Channel 3 — the over-the-air update protocol (`lpc-update`) — on the
//! radio links while the engine runs: what the link mux hands the core's
//! update hook.
//!
//! The USB link's half is `usb_link::usb_update_channel`. Two differences
//! make the radio half its own call:
//!
//! - **The tier rides with the message.** A radio link is untrusted, so the
//!   board's update session decides what it may do from the tier the
//!   **engine's server** holds for it — the tier a login on that link, or
//!   its key, **granted** (`LpServer::link_granted_tier`), never one the
//!   device's `open` setting alone gives: the session adds `open` itself, so
//!   its one access rule (and QY2's switch) applies the same way here as in
//!   core-only. The mux only supplies the tier; it holds no copy of the rule.
//! - **The device's `open` setting rides with it too**, as the server holds
//!   it now (`LpServer::device_open`). The session read `open` from the
//!   access file at boot; a board locked (or opened) over a link since then
//!   must not keep taking a core install over radio on the boot-time answer
//!   until the next reboot, so the hook rebuilds its session's access facts
//!   when this differs.
//! - **A relayed link says so.** Through the cloud relay the device's
//!   `open` never applies (the relay's second lock), so the message carries
//!   whether its link is relayed and the session holds such a link to its
//!   grant alone.
//! - **Links come and go.** A closed radio link is passed on, so the session
//!   forgets it (`BoardSession::link_down`).
//!
//! The mux queues each channel-3 message as its link's events are taken and
//! hands them over from its upkeep, which holds the server; then one
//! [`RadioUpdate::Pass`], so a hook that queued answers the link could not
//! take yet sends them as room appears. A monolithic image installs no hook
//! and never answers channel 3 (DM25).
//!
//! The hook is core code. It runs in task context (the server loop's) and
//! answers through `RadioLinkPort::send_update`; it must not block.

use lpc_access::{OpenTo, Tier};
use lpc_shared::transport::LinkId;

/// One call into the core's radio update hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioUpdate<'a> {
    /// One channel-3 message from `link`, the tier a login or a key
    /// granted that link on the engine's server (`None`: neither did), and
    /// who the device is open to right now. `relayed`: the link came
    /// through the cloud relay, where `open` never applies (the session
    /// holds it to its grant alone).
    Message {
        link: LinkId,
        granted: Option<Tier>,
        open: OpenTo,
        relayed: bool,
        bytes: &'a [u8],
    },
    /// `link` closed: forget it.
    Closed { link: LinkId },
    /// A pass over the radio links: send what the hook still holds.
    Pass,
}

/// The core's radio update hook ([`super::LinkMuxTransport::with_update_hook`]).
pub type RadioUpdateHook = fn(RadioUpdate<'_>);
