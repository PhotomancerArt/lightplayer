//! One accepted connection on the harness: the std twin of the C6's
//! `lan_link_task` and `refuse_task` (`fw-esp32c6/src/net/lan_endpoint_task.rs`).
//!
//! The steps, and their order, are the C6's: upgrade the WebSocket, mint a
//! link id, open the network slot's secure lp-link responder, announce
//! `Opened`; then carry frames — every frame the link has, one binary
//! message each, then wait for the first of a message, the mux's doorbell,
//! the link's next timer, or the mux's close request — until either side
//! ends it; then close the slot and announce `Closed`. When the relay holds
//! the network slot, the connection is a challenge
//! ([`crate::net::network_challenge`]): it takes the slot only with the
//! holder's own key. A connection that finds the LAN endpoint busy is
//! upgraded and told close 1013.

extern crate std;

use alloc::vec;
use core::sync::atomic::{AtomicBool, Ordering};
use std::net::TcpStream;

use embassy_futures::select::{Either4, select4};
use lpc_shared::transport::{LinkId, LinkTrust};

use super::harness_block_on::{block_on, until_micros};
use super::harness_counters::HarnessCounters;
use super::harness_entropy::{harness_entropy, harness_nonce};
use super::std_tcp_byte_stream::StdTcpByteStream;
use crate::net::network_challenge::{ChallengeOutcome, challenge_over_ws};
use crate::net::ws::{CloseCode, RX_OVERHEAD, WsConnection};
use crate::radio_link::lan_link_config::LAN_MAX_FRAME;
use crate::radio_link::{RADIO_LINK_SLOTS, RadioLinkEvent, SharedPort, SlotEdge, now_us};

/// WebSocket close 1013: try again later.
const TRY_AGAIN_LATER: CloseCode = CloseCode(1013);

/// The longest the edge waits before looking at the stop flag again, µs.
const STOP_CHECK_US: u64 = 50_000;

/// How long a challenger waits for its verdict, µs: past lp-link's own 2 s
/// key-lookup limit, with room for the server loop's tick.
const CHALLENGE_WAIT_US: u64 = 5_000_000;

/// Serve `stream` on network slot `lan` until either end closes it or the
/// harness stops. Returns once the slot is free.
pub(super) fn serve_connection(
    stream: TcpStream,
    port: SharedPort,
    lan: usize,
    stop: &AtomicBool,
    counters: &HarnessCounters,
) {
    let index = RADIO_LINK_SLOTS + lan;
    let Ok(stream) = StdTcpByteStream::new(stream) else {
        return;
    };
    let mut ws_rx = vec![0u8; LAN_MAX_FRAME + RX_OVERHEAD];
    let mut frame_tx = vec![0u8; LAN_MAX_FRAME];
    let served = block_on(async {
        let mut ws = match WsConnection::accept(stream, &mut ws_rx).await {
            Ok(ws) => ws,
            Err(_) => {
                log::info!("[lan] a request was not a link upgrade");
                return false;
            }
        };
        let slot = port.slot(index);
        let id = port.mint_link();
        let opened = slot.open_network(
            id,
            harness_nonce(),
            harness_entropy,
            LinkTrust::Keyed,
            SlotEdge::Local,
        );
        let payload = match opened {
            Ok(payload) => payload,
            Err(_) => {
                let stopping = || stop.load(Ordering::SeqCst);
                let deadline = until_micros(now_us() + CHALLENGE_WAIT_US, &stopping);
                match challenge_over_ws(&mut ws, &port, index, id, deadline).await {
                    ChallengeOutcome::TakeOver => {
                        match slot.take_over(
                            id,
                            now_us(),
                            harness_nonce(),
                            harness_entropy,
                            LinkTrust::Keyed,
                        ) {
                            Ok(payload) => {
                                counters.took_over();
                                payload
                            }
                            Err(_) => {
                                busy(ws, counters).await;
                                return false;
                            }
                        }
                    }
                    ChallengeOutcome::Busy => {
                        busy(ws, counters).await;
                        return false;
                    }
                    ChallengeOutcome::PeerGone => return false,
                }
            }
        };
        counters.link_opened();
        log::info!("[lan] link {id}: secure session opening ({payload} B frames)");
        port.announce(RadioLinkEvent::Opened {
            link: id,
            slot: index,
        })
        .await;
        let reason = serve(ws, port, index, id, &mut frame_tx, stop).await;
        slot.close_link(id);
        port.announce(RadioLinkEvent::Closed { link: id }).await;
        log::info!("[lan] link {id}: closed ({reason})");
        true
    });
    if served {
        counters.link_closed();
    }
}

/// Answer a connection that finds the LAN endpoint busy: upgrade it, then
/// WebSocket close 1013 ("try again later").
pub(super) fn refuse_connection(stream: TcpStream, counters: &HarnessCounters) {
    let Ok(stream) = StdTcpByteStream::new(stream) else {
        return;
    };
    let mut ws_rx = vec![0u8; 1024 + RX_OVERHEAD];
    block_on(async {
        if let Ok(ws) = WsConnection::accept(stream, &mut ws_rx).await {
            log::warn!("[lan] every LAN link is in use: a new one was told to try again later");
            busy(ws, counters).await;
        }
    });
}

/// Tell a connection the network link is in use.
async fn busy(ws: WsConnection<'_, StdTcpByteStream>, counters: &HarnessCounters) {
    counters.refused();
    ws.close(TRY_AGAIN_LATER).await;
}

/// Carry frames between the WebSocket and the slot's link `id` until either
/// side ends it (or the mux revokes the link, or another takes it over);
/// why it ended.
async fn serve(
    mut ws: WsConnection<'_, StdTcpByteStream>,
    port: SharedPort,
    index: usize,
    id: LinkId,
    frame_tx: &mut [u8],
    stop: &AtomicBool,
) -> &'static str {
    let slot = port.slot(index);
    let stopping = || stop.load(Ordering::SeqCst);
    loop {
        // Everything the link wants to send now, one message per frame.
        loop {
            let polled = slot.poll_frame_for(id, now_us(), |frame| {
                let n = frame.len().min(frame_tx.len());
                frame_tx[..n].copy_from_slice(&frame[..n]);
                n
            });
            match polled {
                Ok(Some(len)) => {
                    if ws.send(&frame_tx[..len]).await.is_err() {
                        return "the peer stopped taking frames";
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    // Revoked by the mux (its close request says why), or
                    // taken over.
                    let reason = slot
                        .try_close_request_for(SlotEdge::Local)
                        .unwrap_or("closed by the board");
                    ws.close(CloseCode::NORMAL).await;
                    return reason;
                }
            }
        }
        if stopping() {
            ws.close(CloseCode::NORMAL).await;
            return "the harness stopped";
        }
        let check = now_us() + STOP_CHECK_US;
        let wake = slot.poll_timeout_for(id).map_or(check, |at| at.min(check));
        match select4(
            ws.recv(),
            slot.doorbell_for(SlotEdge::Local),
            until_micros(wake, &stopping),
            slot.close_requested_for(SlotEdge::Local),
        )
        .await
        {
            Either4::First(Ok(frame)) => {
                if slot.on_datagram_for(id, now_us(), frame).is_err() {
                    // Revoked: its close request says why (next pass).
                }
            }
            Either4::First(Err(_)) => return "the WebSocket closed",
            Either4::Second(()) | Either4::Third(()) => {}
            Either4::Fourth(reason) => {
                ws.close(CloseCode::NORMAL).await;
                return reason;
            }
        }
    }
}
