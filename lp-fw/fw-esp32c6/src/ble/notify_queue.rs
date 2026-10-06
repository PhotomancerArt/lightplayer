//! Board → host: the connection's lp-link frames out as notifications, one
//! frame per notification.
//!
//! **One frame, one notification, always.** The link's frames are sized to
//! the connection's ATT MTU when the link opens
//! (`fw_esp32_common::radio_link::radio_link_config`), so a frame is never
//! chunked here and a notification never carries two. The central's link
//! reads each notification as one datagram.
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
//! own: calling `notify` back to back keeps up to 8 values in the host plus
//! whatever the controller buffers, and the controller packs as many into
//! each connection event as the central allows. The link's own transmit
//! window (8 frames) is the bound on what is in flight; anything the air
//! loses the link resends.
//!
//! **Backpressure, never a silent drop.** When the outbound channel is full,
//! `notify` waits for room, and so does this pass; the link keeps its frames
//! meanwhile. The one way the host stack drops a notification silently is a
//! central that has not enabled notifications; the connection never opens
//! its link until it has (`nus_service::tx_subscribed`), and a central that
//! turns them off again is disconnected.

use fw_esp32_common::radio_link::{RadioLinkSlot, now_us};
use heapless::Vec;
use trouble_host::prelude::*;

use super::nus_service::{NUS_VALUE_MAX, NusServer};

/// Frames notified per pass before the connection looks at its GATT events
/// again (the central's ACKs arrive there). A full transmit window plus its
/// ACK fits.
const FRAMES_PER_PASS: usize = 12;

/// Why a pass stopped short.
pub enum NotifyFailed {
    /// The connection is gone.
    Disconnected,
    /// The host refused the notification.
    Refused,
}

/// Notify the frames `slot`'s link has ready now. `Ok(true)`: the pass
/// stopped at [`FRAMES_PER_PASS`] and more may be waiting.
pub async fn send_frames(
    slot: &RadioLinkSlot,
    server: &NusServer<'_>,
    conn: &GattConnection<'_, '_, DefaultPacketPool>,
) -> Result<bool, NotifyFailed> {
    for _ in 0..FRAMES_PER_PASS {
        let mut value: Vec<u8, NUS_VALUE_MAX> = Vec::new();
        // The copy happens inside the link's borrow; the notify below awaits
        // with the link free.
        let Some(fits) = slot.poll_frame(now_us(), |frame| value.extend_from_slice(frame).is_ok())
        else {
            return Ok(false);
        };
        if !fits {
            // Cannot happen: the link's frames are sized to one ATT value
            // (at most NUS_VALUE_MAX) when it opens. Not sent; the link
            // resends it and gives the session up if it never goes.
            log::error!("[ble] a frame larger than one notification — not sent");
            continue;
        }
        if !conn.raw().is_connected() {
            return Err(NotifyFailed::Disconnected);
        }
        if server.uart.tx.notify(conn, &value).await.is_err() {
            return Err(NotifyFailed::Refused);
        }
    }
    Ok(true)
}
