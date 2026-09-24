//! One BLE connection, served as one radio link for its whole life.
//!
//! The connection gets a fresh [`LinkId`] at connect, but the link *opens* —
//! is announced to the mux, gets its hello, may carry requests — only once
//! the central has enabled notifications on TX. Before that, nothing the
//! board sent would arrive (the host stack skips notifications to an
//! unsubscribed central without saying so), so nothing is accepted either.
//!
//! One task, one `select` loop, four things it waits on:
//!
//! - **GATT events:** RX writes are re-joined into `M!` lines and handed to
//!   the server loop; a CCCD write opens (or, turned off, closes) the link;
//!   parameter requests from the central are accepted.
//! - **Frames from the mux** for this slot, sent as notifications
//!   (`notify_queue`), the result reported back.
//! - **Close requests from the mux** (login deadline, a write that missed its
//!   deadline): disconnect, logged with the reason.
//! - **Timers:** the connection-parameter request shortly after connect, its
//!   read-back, and the subscribe deadline — a central that never enables
//!   notifications is disconnected after the same 10 s a link gets to log in.

use embassy_futures::select::{Either4, select4};
use embassy_time::{Duration, Instant, Timer};
use fw_esp32_common::radio_link::line_joiner::LineJoiner;
use fw_esp32_common::radio_link::{
    LOGIN_DEADLINE_MS, RADIO_LINE_CAP, RADIO_LINK_PORT, RadioLinkEvent,
};
use trouble_host::prelude::*;

use super::ble_task::BleStack;
use super::conn_params;
use super::notify_queue;
use super::nus_service::{NusServer, tx_subscribed};

/// When, after connect, to ask for the preferred parameters: after the
/// central's own opening procedures (MTU exchange, PHY and data-length
/// updates), which the spike saw finish well inside a second.
const PARAMS_REQUEST_AFTER: Duration = Duration::from_millis(1_000);
/// When, after the request, to read back what was granted.
const PARAMS_READBACK_AFTER: Duration = Duration::from_millis(3_000);

/// Serve `conn` on `slot` until it disconnects.
pub async fn serve(
    conn: GattConnection<'static, 'static, DefaultPacketPool>,
    slot: usize,
    server: &'static NusServer<'static>,
    stack: &'static BleStack,
) {
    let port = &RADIO_LINK_PORT;
    let slot_port = port.slot(slot);
    slot_port.reset();
    let link = port.mint_link();
    let connected_at = Instant::now();
    log::info!(
        "[ble] {link}: connected (slot {slot}), heap used {} B",
        esp_alloc::HEAP.used()
    );
    conn_params::log_granted(link, &conn, "at connect");

    let mut joiner = LineJoiner::new(RADIO_LINE_CAP);
    let mut opened = false;
    let mut closing = false;
    let mut params_request_at = Some(connected_at + PARAMS_REQUEST_AFTER);
    let mut params_readback_at: Option<Instant> = None;
    let subscribe_deadline = connected_at + Duration::from_millis(LOGIN_DEADLINE_MS);

    let reason = loop {
        let next_timer = [
            params_request_at,
            params_readback_at,
            (!opened && !closing).then_some(subscribe_deadline),
        ]
        .into_iter()
        .flatten()
        .min();
        let timer = async move {
            match next_timer {
                Some(at) => Timer::at(at).await,
                None => core::future::pending::<()>().await,
            }
        };
        let writing = opened;
        let write = async move {
            if writing {
                slot_port.next_write().await
            } else {
                core::future::pending().await
            }
        };

        match select4(conn.next(), write, slot_port.close_requested(), timer).await {
            Either4::First(GattConnectionEvent::Disconnected { reason }) => break reason,
            Either4::First(GattConnectionEvent::Gatt { event }) => {
                let rx_bytes = match &event {
                    GattEvent::Write(w) if w.handle() == server.uart.rx.handle => {
                        let mut bytes =
                            heapless::Vec::<u8, { super::nus_service::NUS_VALUE_MAX }>::new();
                        let _ = bytes.extend_from_slice(w.data());
                        Some(bytes)
                    }
                    _ => None,
                };
                match event.accept() {
                    Ok(reply) => reply.send().await,
                    Err(_) => log::warn!("[ble] {link}: GATT reply failed"),
                }
                if let Some(bytes) = rx_bytes {
                    let report = joiner.push(&bytes, |line| {
                        if !opened {
                            log::warn!(
                                "[ble] {link}: {} B line before notifications were enabled — dropped",
                                line.len()
                            );
                        } else if !port.deliver_line(link, line) {
                            log::warn!("[ble] {link}: incoming queue full, dropping an M! line");
                        }
                    });
                    if report.dropped_any() {
                        log::warn!(
                            "[ble] {link}: dropped {} over-long and {} non-UTF-8 lines",
                            report.overflowed_lines,
                            report.invalid_utf8_lines
                        );
                    }
                }
                // A CCCD write (or any write) may have changed the
                // subscription; the table is the truth.
                let subscribed = tx_subscribed(server, conn.raw());
                if subscribed && !opened {
                    opened = true;
                    log::info!("[ble] {link}: notifications on — link open");
                    port.announce(RadioLinkEvent::Opened { link, slot }).await;
                } else if !subscribed && opened {
                    log::warn!("[ble] {link}: notifications turned off — disconnecting");
                    closing = true;
                    conn.raw().disconnect();
                }
            }
            Either4::First(GattConnectionEvent::RequestConnectionParams(request)) => {
                conn_params::accept_central_request(link, request, stack).await;
            }
            Either4::First(GattConnectionEvent::ConnectionParamsUpdated { .. }) => {
                conn_params::log_granted(link, &conn, "parameters updated");
            }
            Either4::First(GattConnectionEvent::PhyUpdated { .. }) => {
                log::debug!("[ble] {link}: PHY updated");
            }
            Either4::First(GattConnectionEvent::DataLengthUpdated {
                max_tx_octets,
                max_rx_octets,
                ..
            }) => {
                log::debug!("[ble] {link}: data length tx={max_tx_octets} rx={max_rx_octets}");
            }
            Either4::Second(request) => {
                let result = notify_queue::send_frame(port, &request, server, &conn).await;
                port.finish_write(&request, result);
            }
            Either4::Third(reason) => {
                log::warn!("[ble] {link}: closing at the server's request ({reason})");
                closing = true;
                conn.raw().disconnect();
            }
            Either4::Fourth(()) => {
                let now = Instant::now();
                if params_request_at.is_some_and(|at| now >= at) {
                    params_request_at = None;
                    conn_params::request_preferred(link, &conn, stack).await;
                    params_readback_at = Some(Instant::now() + PARAMS_READBACK_AFTER);
                } else if params_readback_at.is_some_and(|at| now >= at) {
                    params_readback_at = None;
                    conn_params::log_granted(link, &conn, "granted");
                } else if !opened && now >= subscribe_deadline {
                    log::warn!(
                        "[ble] {link}: notifications not enabled within {} s — disconnecting",
                        LOGIN_DEADLINE_MS / 1000
                    );
                    closing = true;
                    conn.raw().disconnect();
                }
            }
        }
    };

    if opened {
        port.announce(RadioLinkEvent::Closed { link }).await;
    }
    // `{:?}` prints nothing in this build profile (`-Zfmt-debug=none`), so
    // the HCI status code itself: 0x08 supervision timeout, 0x13 remote user
    // terminated, 0x16 local host terminated.
    log::info!(
        "[ble] {link}: disconnected, reason=0x{:02x}, heap used {} B",
        reason.into_inner(),
        esp_alloc::HEAP.used()
    );
}
