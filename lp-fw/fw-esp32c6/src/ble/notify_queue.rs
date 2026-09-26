//! Board → host: one server frame out as a run of notifications (plan Q6).
//!
//! **What trouble-host 0.6 does with a notification** (read from its source,
//! `attribute.rs` `Characteristic::notify` → `connection_manager.rs` `send`):
//! `notify(..).await` copies the value into a packet from the host's packet
//! pool and pushes it onto the host's outbound channel, and returns as soon
//! as it is *queued* — not when it has been sent, let alone acknowledged. The
//! host's TX runner drains that channel into the controller as the
//! controller grants ACL buffers. The channel is `L2CAP_TX_QUEUE_SIZE` deep
//! (8, trouble-host's default; shared by every connection), and each queued
//! value holds one of the pool's 16 packets.
//!
//! So "several notifications per connection event" needs no queue of our
//! own: calling `notify` back to back, without waiting on anything else
//! between calls, keeps up to 8 values (~2 KB) in the host plus whatever the
//! controller buffers, and the controller packs as many into each connection
//! event as the central allows. The spike's "awaited one at a time" was
//! already this; its 5–12 KB/s on a Mac against 22–48 KB/s on Bluefy
//! (Run F) is the central setting the pace, not the board.
//!
//! **Backpressure, never a silent drop.** When the outbound channel is full,
//! `notify` waits for room, and so this write — and the mux's send above it,
//! which the server loop awaits — waits too, up to the mux's deadline. The
//! one way the host stack drops a notification silently is a central that
//! has not enabled notifications; the connection never opens its link until
//! it has (`nus_service::tx_subscribed`), and a central that turns them off
//! again is disconnected.

use fw_esp32_common::radio_link::line_chunker::chunk_spans;
use fw_esp32_common::radio_link::{RadioLinkPort, RadioWriteRequest};
use heapless::Vec;
use lpc_wire::TransportError;
use trouble_host::prelude::*;

use super::nus_service::{NUS_VALUE_MAX, NusServer};

/// Send `request`'s frame to the central on `conn`, chunked to its MTU.
pub async fn send_frame(
    port: &RadioLinkPort,
    request: &RadioWriteRequest,
    server: &NusServer<'_>,
    conn: &GattConnection<'_, '_, DefaultPacketPool>,
) -> Result<(), TransportError> {
    let att_mtu = conn.raw().att_mtu();
    for (offset, len) in chunk_spans(request.len, att_mtu, NUS_VALUE_MAX) {
        if !conn.raw().is_connected() {
            return Err(TransportError::ConnectionLost);
        }
        let mut value: Vec<u8, NUS_VALUE_MAX> = Vec::new();
        // `len` is at most NUS_VALUE_MAX by construction.
        let _ = value.resize(len, 0);
        // The copy and the enqueue below happen with no await between them
        // and the lease check, so a frame the mux has abandoned is never read.
        if !port.copy_frame(request, offset, &mut value) {
            return Err(TransportError::Other("frame withdrawn by the mux".into()));
        }
        server
            .uart
            .tx
            .notify(conn, &value)
            .await
            .map_err(|_| TransportError::Other("notify refused by the host".into()))?;
    }
    Ok(())
}
