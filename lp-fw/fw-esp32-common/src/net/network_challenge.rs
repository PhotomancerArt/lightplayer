//! A LAN connection that found the network slot held: park its first frame,
//! and wait for the mux to say whether it takes the slot over (Wi-Fi relay
//! plan D2; the rule is `radio_link::parked_handshake`'s).
//!
//! Shared by the C6's LAN endpoint (`fw-esp32c6/src/net/lan_endpoint_task.rs`)
//! and its std twin in the host harness, so the two cannot drift. The
//! relay's newcomers take the same verdict through the relay driver
//! (`net::relay`), event by event.

use core::future::Future;
use core::pin::pin;

use embassy_futures::select::{Either, Either3, select, select3};
use lpc_shared::transport::LinkId;

use crate::net::ws::{ByteStream, WsConnection};
use crate::radio_link::{ChallengeVerdict, RadioLinkEvent, RadioLinkPort, SlotEdge};

/// How a challenge ended for the LAN edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeOutcome {
    /// The newcomer proved the holder's key: open the slot with
    /// `RadioLinkSlot::take_over`.
    TakeOver,
    /// Turned away: close the WebSocket 1013 ("try again later").
    Busy,
    /// The newcomer left (or never spoke) before the verdict.
    PeerGone,
}

/// Read `id`'s first frame off `ws`, park it on network slot `index`,
/// announce the challenge, and wait for the verdict — reading (and
/// dropping) the newcomer's resent SYNs meanwhile — or for `deadline`. A
/// newcomer that gives up, or runs out the deadline, is withdrawn and
/// announced closed, so the mux forgets it too.
pub async fn challenge_over_ws<S: ByteStream>(
    ws: &mut WsConnection<'_, S>,
    port: &RadioLinkPort,
    index: usize,
    id: LinkId,
    deadline: impl Future<Output = ()>,
) -> ChallengeOutcome {
    let slot = port.slot(index);
    let mut deadline = pin!(deadline);
    let parked = match select(ws.recv(), &mut deadline).await {
        Either::First(Ok(first)) => slot.park_challenge(id, SlotEdge::Local, first).is_ok(),
        Either::First(Err(_)) => return ChallengeOutcome::PeerGone,
        Either::Second(()) => return ChallengeOutcome::Busy,
    };
    if !parked {
        return ChallengeOutcome::Busy;
    }
    port.announce(RadioLinkEvent::Challenged {
        link: id,
        slot: index,
    })
    .await;
    let outcome = loop {
        match select3(slot.verdict(), ws.recv(), &mut deadline).await {
            Either3::First(ChallengeVerdict::TakeOver) => return ChallengeOutcome::TakeOver,
            Either3::First(ChallengeVerdict::Busy) => return ChallengeOutcome::Busy,
            // A resent SYN: the parked one stands for it.
            Either3::Second(Ok(_)) => {}
            Either3::Second(Err(_)) => break ChallengeOutcome::PeerGone,
            Either3::Third(()) => break ChallengeOutcome::Busy,
        }
    };
    slot.withdraw_challenge(id);
    port.announce(RadioLinkEvent::Closed { link: id }).await;
    outcome
}
