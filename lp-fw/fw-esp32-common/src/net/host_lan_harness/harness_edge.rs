//! One accepted connection on the harness: the std twin of the C6's
//! `lan_link_task` and `refuse_task` (`fw-esp32c6/src/net/lan_endpoint_task.rs`).
//!
//! The steps, and their order, are the C6's: upgrade the WebSocket, reset the
//! slot, mint a link id, open the slot's secure lp-link responder, announce
//! `Opened`; then carry frames — every frame the link has, one binary message
//! each, then wait for the first of a message, the mux's doorbell, the link's
//! next timer, or the mux's close request — until either side ends it; then
//! close the slot and announce `Closed`. A connection that finds every LAN
//! slots busy is upgraded and told close 1013.

extern crate std;

use alloc::vec;
use core::sync::atomic::{AtomicBool, Ordering};
use std::net::TcpStream;

use embassy_futures::select::{Either4, select4};

use super::harness_block_on::{block_on, until_micros};
use super::harness_counters::HarnessCounters;
use super::harness_entropy::{harness_entropy, harness_nonce};
use super::std_tcp_byte_stream::StdTcpByteStream;
use crate::net::ws::{CloseCode, RX_OVERHEAD, WsConnection};
use crate::radio_link::lan_link_config::LAN_MAX_FRAME;
use crate::radio_link::{RADIO_LINK_SLOTS, RadioLinkEvent, SharedPort, now_us};

/// WebSocket close 1013: try again later.
const TRY_AGAIN_LATER: CloseCode = CloseCode(1013);

/// The longest the edge waits before looking at the stop flag again, µs.
const STOP_CHECK_US: u64 = 50_000;

/// Serve `stream` on LAN slot `lan` until either end closes it or the
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
    block_on(async {
        let ws = match WsConnection::accept(stream, &mut ws_rx).await {
            Ok(ws) => ws,
            Err(_) => {
                log::info!("[lan] a request was not a link upgrade");
                return;
            }
        };
        let slot = port.slot(index);
        let id = port.mint_link();
        slot.reset();
        let payload = slot.open_lan(id, harness_nonce(), harness_entropy);
        counters.link_opened();
        log::info!("[lan] link {id}: secure session opening ({payload} B frames)");
        port.announce(RadioLinkEvent::Opened {
            link: id,
            slot: index,
        })
        .await;
        let reason = serve(ws, port, index, &mut frame_tx, stop).await;
        slot.close();
        port.announce(RadioLinkEvent::Closed { link: id }).await;
        log::info!("[lan] link {id}: closed ({reason})");
    });
    counters.link_closed();
}

/// Answer a connection that finds every LAN slot busy: upgrade it, then
/// WebSocket close 1013 ("try again later").
pub(super) fn refuse_connection(stream: TcpStream, counters: &HarnessCounters) {
    let Ok(stream) = StdTcpByteStream::new(stream) else {
        return;
    };
    let mut ws_rx = vec![0u8; 1024 + RX_OVERHEAD];
    block_on(async {
        if let Ok(ws) = WsConnection::accept(stream, &mut ws_rx).await {
            log::warn!("[lan] every LAN link is in use: a new one was told to try again later");
            counters.refused();
            ws.close(TRY_AGAIN_LATER).await;
        }
    });
}

/// Carry frames between the WebSocket and the slot's link until either side
/// ends it; why it ended.
async fn serve(
    mut ws: WsConnection<'_, StdTcpByteStream>,
    port: SharedPort,
    index: usize,
    frame_tx: &mut [u8],
    stop: &AtomicBool,
) -> &'static str {
    let slot = port.slot(index);
    let stopping = || stop.load(Ordering::SeqCst);
    loop {
        // Everything the link wants to send now, one message per frame.
        while let Some(len) = slot.poll_frame(now_us(), |frame| {
            let n = frame.len().min(frame_tx.len());
            frame_tx[..n].copy_from_slice(&frame[..n]);
            n
        }) {
            if ws.send(&frame_tx[..len]).await.is_err() {
                return "the peer stopped taking frames";
            }
        }
        if stopping() {
            ws.close(CloseCode::NORMAL).await;
            return "the harness stopped";
        }
        let check = now_us() + STOP_CHECK_US;
        let wake = slot.poll_timeout().map_or(check, |at| at.min(check));
        match select4(
            ws.recv(),
            slot.doorbell(),
            until_micros(wake, &stopping),
            slot.close_requested(),
        )
        .await
        {
            Either4::First(Ok(frame)) => slot.on_datagram(now_us(), frame),
            Either4::First(Err(_)) => return "the WebSocket closed",
            Either4::Second(()) | Either4::Third(()) => {}
            Either4::Fourth(reason) => {
                ws.close(CloseCode::NORMAL).await;
                return reason;
            }
        }
    }
}
