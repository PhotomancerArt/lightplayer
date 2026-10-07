//! One BLE connection, served as one radio link for its whole life.
//!
//! The connection gets a fresh [`LinkId`] at connect, but the link *opens* —
//! gets its lp-link session, is announced to the mux — only once the central
//! has enabled notifications on TX. Before that, nothing the board sent would
//! arrive (the host stack skips notifications to an unsubscribed central
//! without saying so), so nothing is accepted either. At that moment the
//! board reads the connection's negotiated ATT MTU and sizes the session's
//! frames to one ATT value (`fw_esp32_common::radio_link::radio_link_config`);
//! a connection whose MTU cannot carry even a handshake frame is refused and
//! disconnected. The session's handshake then runs as ordinary traffic, and
//! the mux owes the link its hello when that session comes up — not at
//! subscribe.
//!
//! One task, one `select` loop, four things it waits on:
//!
//! - **GATT events:** each write to RX is one whole lp-link frame, fed to the
//!   link; a CCCD write opens (or, turned off, closes) the link; parameter
//!   requests from the central are accepted. A long write (Prepare …
//!   Execute) is refused: no frame ever needs one.
//! - **The mux's doorbell** for this slot: something was queued, transmit.
//! - **Close requests from the mux** (login deadline, a reply the central did
//!   not take in time): disconnect, logged with the reason.
//! - **Timers:** the link's own (resend, delayed ACK, keepalive, handshake),
//!   the connection-parameter request shortly after connect and its
//!   read-back, and the subscribe deadline — a central that never enables
//!   notifications is disconnected after the same 10 s a link gets to log in.
//!
//! Each turn of the loop starts by notifying whatever frames the link has
//! ready (`notify_queue`).

use embassy_futures::select::{Either4, select4};
use embassy_time::{Duration, Instant, Timer};
use fw_esp32_common::radio_link::{
    LOGIN_DEADLINE_MS, OpenRefused, RadioLinkEvent, RadioLinkMode, RadioLinkPort, now_us,
};
use lpc_shared::transport::LinkId;
use trouble_host::att::{AttClient, AttReq};
use trouble_host::prelude::*;

use super::ble_task::BleStack;
use super::conn_params;
use super::notify_queue::{self, NotifyFailed};
use super::nus_service::{NusServer, tx_subscribed};

/// When, after connect, to ask for the preferred parameters: after the
/// central's own opening procedures (MTU exchange, PHY and data-length
/// updates), which the spike saw finish well inside a second.
const PARAMS_REQUEST_AFTER: Duration = Duration::from_millis(1_000);
/// When, after the request, to read back what was granted.
const PARAMS_READBACK_AFTER: Duration = Duration::from_millis(3_000);

/// Serve `conn` on `slot` of `port` until it disconnects.
pub async fn serve(
    conn: GattConnection<'static, 'static, DefaultPacketPool>,
    slot: usize,
    port: &'static RadioLinkPort,
    server: &'static NusServer<'static>,
    stack: &'static BleStack,
) {
    let slot_port = port.slot(slot);
    slot_port.reset();
    let link = port.mint_link();
    let connected_at = Instant::now();
    log::info!(
        "[ble] {link}: connected (slot {slot}), heap used {} B",
        esp_alloc::HEAP.used()
    );
    conn_params::log_granted(link, &conn, "at connect");

    let mut opened = false;
    let mut closing = false;
    // The link had more frames ready than one pass sends.
    let mut more = false;
    let mut params_request_at = Some(connected_at + PARAMS_REQUEST_AFTER);
    let mut params_readback_at: Option<Instant> = None;
    let subscribe_deadline = connected_at + Duration::from_millis(LOGIN_DEADLINE_MS);
    // Run K's active/idle experiment (`desk_ble_params`): the last write from
    // the central, and whether the idle parameters are in force.
    #[cfg(feature = "desk_ble_params")]
    let mut last_rx = connected_at;
    #[cfg(feature = "desk_ble_params")]
    let mut idle_params = false;

    let reason = loop {
        if opened && !closing {
            match notify_queue::send_frames(slot_port, server, &conn).await {
                Ok(again) => more = again,
                Err(failed) => {
                    more = false;
                    if matches!(failed, NotifyFailed::Refused) {
                        log::warn!(
                            "[ble] {link}: notification refused by the host — disconnecting"
                        );
                    }
                    closing = true;
                    conn.raw().disconnect();
                }
            }
        }
        let link_timer = (opened && !closing)
            .then(|| slot_port.poll_timeout())
            .flatten()
            .map(|at| Instant::from_micros(if more { 0 } else { at }));
        let next_timer = [
            params_request_at,
            params_readback_at,
            (!opened && !closing).then_some(subscribe_deadline),
            link_timer,
        ]
        .into_iter();
        // Run K only; the shipped build's connection future carries none of it.
        #[cfg(feature = "desk_ble_params")]
        let next_timer = next_timer.chain([idle_deadline(idle_params, params_request_at, last_rx)]);
        let next_timer = next_timer.flatten().min();
        let timer = async move {
            match next_timer {
                Some(at) => Timer::at(at).await,
                None => core::future::pending::<()>().await,
            }
        };

        match select4(
            conn.next(),
            slot_port.doorbell(),
            slot_port.close_requested(),
            timer,
        )
        .await
        {
            Either4::First(GattConnectionEvent::Disconnected { reason }) => break reason,
            Either4::First(GattConnectionEvent::Gatt { event }) => {
                let (wrote, refuse) = rx_write(&event, server, link);
                if wrote {
                    #[cfg(feature = "desk_ble_params")]
                    {
                        last_rx = Instant::now();
                    }
                    if let GattEvent::Write(w) = &event {
                        if opened {
                            slot_port.on_datagram(now_us(), w.data());
                        } else {
                            log::debug!(
                                "[ble] {link}: {} B written before notifications were \
                                 enabled — dropped",
                                w.data().len()
                            );
                        }
                    }
                }
                let reply = match refuse {
                    Some(code) => event.reject(code),
                    None => event.accept(),
                };
                match reply {
                    Ok(reply) => reply.send().await,
                    Err(_) => log::warn!("[ble] {link}: GATT reply failed"),
                }
                #[cfg(feature = "desk_ble_params")]
                if wrote && idle_params {
                    idle_params = false;
                    let (i, l, t) = conn_params::desk::active();
                    log::info!("[ble-exp] {link}: write after idle — asking for active params");
                    conn_params::request(link, &conn, stack, i, l, t).await;
                    params_readback_at = Some(Instant::now() + PARAMS_READBACK_AFTER);
                }
                // A CCCD write (or any write) may have changed the
                // subscription; the table is the truth.
                let subscribed = tx_subscribed(server, conn.raw());
                if subscribed && !opened && !closing {
                    if open_link(port, slot, link, &conn).await {
                        opened = true;
                    } else {
                        closing = true;
                        conn.raw().disconnect();
                    }
                } else if !subscribed && opened && !closing {
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
            // The mux queued something: the next turn transmits it.
            Either4::Second(()) => {}
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
                } else if {
                    #[cfg(feature = "desk_ble_params")]
                    let due = idle_deadline(idle_params, params_request_at, last_rx)
                        .is_some_and(|at| now >= at);
                    #[cfg(not(feature = "desk_ble_params"))]
                    let due = false;
                    due
                } {
                    #[cfg(feature = "desk_ble_params")]
                    {
                        idle_params = true;
                        let (i, l, t) = conn_params::desk::idle();
                        log::info!("[ble-exp] {link}: idle — asking for idle params");
                        conn_params::request(link, &conn, stack, i, l, t).await;
                        params_readback_at = Some(Instant::now() + PARAMS_READBACK_AFTER);
                    }
                } else if !opened && !closing && now >= subscribe_deadline {
                    log::warn!(
                        "[ble] {link}: notifications not enabled within {} s — disconnecting",
                        LOGIN_DEADLINE_MS / 1000
                    );
                    closing = true;
                    conn.raw().disconnect();
                }
                // Otherwise the link's own timer: the next turn transmits.
            }
        }
    };

    // The link's RAM goes back to the heap here, whatever closed it.
    slot_port.close();
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

/// The central enabled notifications: start the connection's lp-link session
/// at its negotiated ATT MTU, in the boot's radio link mode, and announce the
/// link. `false`: the MTU cannot carry a frame and the caller disconnects.
async fn open_link(
    port: &'static RadioLinkPort,
    slot: usize,
    link: LinkId,
    conn: &GattConnection<'_, '_, DefaultPacketPool>,
) -> bool {
    // The boot decides what its radio links are for (serving the wire, or
    // core-only taking an update with a wider receive window) before this
    // task has ever run; should a central ever subscribe first, its link
    // waits here rather than open with a window nobody chose.
    let mode = port.wait_for_mode(slot).await;
    let att_mtu = conn.raw().att_mtu();
    // Random per connection: it is how the central learns this is a new
    // session (the RNG is the one the login challenges draw from).
    let nonce = esp_hal::rng::Rng::new().random();
    match port.open(slot, link, att_mtu, nonce) {
        Ok(max_payload) => {
            log::info!(
                "[ble] {link}: notifications on — link open (ATT MTU {att_mtu}, frames \
                 {max_payload} B + 8, {}, link RAM {} B, heap used {} B)",
                match mode {
                    RadioLinkMode::Serve => "serving",
                    RadioLinkMode::Update => "update mode",
                },
                port.slot(slot).ram_bytes().unwrap_or(0),
                esp_alloc::HEAP.used()
            );
            port.announce(RadioLinkEvent::Opened { link, slot }).await;
            true
        }
        Err(OpenRefused::ModeUndecided) => {
            // `wait_for_mode` returned, so the mode is decided: unreachable.
            log::error!("[ble] {link}: radio link mode undecided — disconnecting");
            false
        }
        Err(OpenRefused::MtuTooSmall(_)) => {
            // D5 (plan `ble-on-lp-link`): below the Bluetooth minimum not
            // even the handshake frame fits one ATT value. Refused rather
            // than opened on a link that can never carry a frame.
            log::error!(
                "[ble] {link}: ATT MTU {att_mtu} cannot carry an lp-link frame — disconnecting"
            );
            false
        }
    }
}

/// Whether a GATT event wrote RX, and the ATT error to refuse it with.
///
/// Every frame fits one ATT value, so a plain write (with or without
/// response) is the only write a central makes. A long write (`Prepare
/// Write` … `Execute Write`), which trouble-host reports as an "other" event
/// and would otherwise answer with success while dropping the bytes
/// (docs/defects/2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md),
/// is refused at its first segment, so a central that tries one fails loudly
/// instead of losing it.
fn rx_write(
    event: &GattEvent<'_, '_, DefaultPacketPool>,
    server: &NusServer<'_>,
    link: LinkId,
) -> (bool, Option<AttErrorCode>) {
    let rx_handle = server.uart.rx.handle;
    match event {
        GattEvent::Write(w) if w.handle() == rx_handle => (true, None),
        GattEvent::Other(other) => match other.payload().incoming() {
            AttClient::Request(AttReq::PrepareWrite { handle, offset, .. })
                if handle == rx_handle =>
            {
                log::warn!(
                    "[ble] {link}: long write refused at offset {offset}: every frame is one \
                     ATT value"
                );
                (false, Some(AttErrorCode::REQUEST_NOT_SUPPORTED))
            }
            _ => (false, None),
        },
        _ => (false, None),
    }
}

/// Run K's active/idle switch: when to ask for the idle parameters, if at all.
#[cfg(feature = "desk_ble_params")]
fn idle_deadline(
    idle_params: bool,
    params_request_at: Option<Instant>,
    last_rx: Instant,
) -> Option<Instant> {
    conn_params::desk::idle_after()
        .filter(|_| !idle_params && params_request_at.is_none())
        .map(|after| last_rx + after)
}
