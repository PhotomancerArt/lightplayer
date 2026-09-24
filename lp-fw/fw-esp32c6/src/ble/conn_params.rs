//! Connection parameters: ask for 15 ms / 4 s, and say what was granted.
//!
//! Both centrals measured (macOS, iOS Bluefy) open a connection at 30 ms with
//! a 720 ms supervision timeout, and both accepted a peripheral's request for
//! 15 ms / 4 s (spike Runs C and F). 15 ms halves a knob's round trip; 4 s
//! rides out a phone's radio briefly busy elsewhere instead of dropping the
//! link. The request goes out once per connection, shortly after connect.
//!
//! The spike never saw a "parameters updated" event arrive on its own, so the
//! outcome is read back from the connection a few seconds later and logged —
//! that line is the record of what the central actually granted.

use embassy_time::Duration;
use trouble_host::prelude::*;

use super::ble_task::BleStack;

/// Requested interval (min = max).
pub const PREFERRED_INTERVAL: Duration = Duration::from_millis(15);
/// Requested supervision timeout.
pub const PREFERRED_SUPERVISION_TIMEOUT: Duration = Duration::from_secs(4);

/// Ask the central for the preferred parameters. A refusal is logged; the
/// link carries on at whatever the central chose.
pub async fn request_preferred(
    link: lpc_shared::transport::LinkId,
    conn: &GattConnection<'_, '_, DefaultPacketPool>,
    stack: &BleStack,
) {
    let params = RequestedConnParams {
        min_connection_interval: PREFERRED_INTERVAL,
        max_connection_interval: PREFERRED_INTERVAL,
        max_latency: 0,
        supervision_timeout: PREFERRED_SUPERVISION_TIMEOUT,
        ..Default::default()
    };
    match conn.raw().update_connection_params(stack, &params).await {
        Ok(()) => log::info!(
            "[ble] {link}: asked for interval {} us, latency 0, timeout {} ms",
            PREFERRED_INTERVAL.as_micros(),
            PREFERRED_SUPERVISION_TIMEOUT.as_millis()
        ),
        Err(_) => log::warn!("[ble] {link}: connection-parameter request FAILED"),
    }
}

/// Log the parameters the connection is running at now.
pub fn log_granted(
    link: lpc_shared::transport::LinkId,
    conn: &GattConnection<'_, '_, DefaultPacketPool>,
    why: &str,
) {
    let raw = conn.raw();
    let p = raw.params();
    log::info!(
        "[ble] {link}: {why}: interval_us={} latency={} timeout_ms={} mtu={}",
        p.conn_interval.as_micros(),
        p.peripheral_latency,
        p.supervision_timeout.as_millis(),
        raw.att_mtu()
    );
}

/// Accept a central-initiated parameter request as-is, the way the spike did.
pub async fn accept_central_request(
    link: lpc_shared::transport::LinkId,
    request: ConnectionParamsRequest,
    stack: &BleStack,
) {
    let p = request.params();
    log::info!(
        "[ble] {link}: central asks interval {}..{} us latency {} timeout {} ms",
        p.min_connection_interval.as_micros(),
        p.max_connection_interval.as_micros(),
        p.max_latency,
        p.supervision_timeout.as_millis()
    );
    if request.accept(None, stack).await.is_err() {
        log::warn!("[ble] {link}: accepting the central's parameters FAILED");
    }
}
