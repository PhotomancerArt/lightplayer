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
    #[cfg(not(feature = "desk_ble_params"))]
    let (interval, latency, timeout) = (PREFERRED_INTERVAL, 0u16, PREFERRED_SUPERVISION_TIMEOUT);
    #[cfg(feature = "desk_ble_params")]
    let (interval, latency, timeout) = desk::active();
    request(link, conn, stack, interval, latency, timeout).await;
}

/// Ask the central for `interval` (min = max), `latency`, `timeout`.
pub async fn request(
    link: lpc_shared::transport::LinkId,
    conn: &GattConnection<'_, '_, DefaultPacketPool>,
    stack: &BleStack,
    interval: Duration,
    latency: u16,
    timeout: Duration,
) {
    let params = RequestedConnParams {
        min_connection_interval: interval,
        max_connection_interval: interval,
        max_latency: latency,
        supervision_timeout: timeout,
        ..Default::default()
    };
    match conn.raw().update_connection_params(stack, &params).await {
        Ok(()) => log::info!(
            "[ble] {link}: asked for interval {} us, latency {latency}, timeout {} ms",
            interval.as_micros(),
            timeout.as_millis()
        ),
        Err(_) => log::warn!("[ble] {link}: connection-parameter request FAILED"),
    }
}

/// The desk's connection-parameter experiment (BLE M4 round 3, Run K) —
/// feature `desk_ble_params`, never shipped. The parameters come from
/// `/.lp/ble-exp.txt`, read once at boot, so a run changes them with a file
/// write and a reboot instead of a reflash:
///
/// ```text
/// interval_us latency timeout_ms [idle_after_ms idle_interval_us idle_latency]
/// ```
///
/// With `idle_after_ms` > 0 the link asks for the idle parameters after that
/// long with no traffic either way, and for the active ones again on the next
/// write from the central. Missing file: the shipped 15 ms / 0 / 4 s.
#[cfg(feature = "desk_ble_params")]
pub mod desk {
    use core::sync::atomic::{AtomicU32, Ordering};

    use embassy_time::Duration;

    static INTERVAL_US: AtomicU32 = AtomicU32::new(super::PREFERRED_INTERVAL.as_micros() as u32);
    static LATENCY: AtomicU32 = AtomicU32::new(0);
    static TIMEOUT_MS: AtomicU32 =
        AtomicU32::new(super::PREFERRED_SUPERVISION_TIMEOUT.as_millis() as u32);
    static IDLE_AFTER_MS: AtomicU32 = AtomicU32::new(0);
    static IDLE_INTERVAL_US: AtomicU32 =
        AtomicU32::new(super::PREFERRED_INTERVAL.as_micros() as u32);
    static IDLE_LATENCY: AtomicU32 = AtomicU32::new(0);

    /// Where the experiment's parameters live.
    pub const PATH: &str = "/.lp/ble-exp.txt";

    /// Take the parameters from the file's text (whitespace-separated).
    pub fn configure(text: &str) {
        let mut v = [0u32; 6];
        let mut n = 0;
        for word in text.split_whitespace() {
            if n < v.len()
                && let Ok(x) = word.parse::<u32>()
            {
                v[n] = x;
                n += 1;
            }
        }
        if n >= 3 {
            INTERVAL_US.store(v[0], Ordering::Relaxed);
            LATENCY.store(v[1], Ordering::Relaxed);
            TIMEOUT_MS.store(v[2], Ordering::Relaxed);
        }
        if n >= 6 {
            IDLE_AFTER_MS.store(v[3], Ordering::Relaxed);
            IDLE_INTERVAL_US.store(v[4], Ordering::Relaxed);
            IDLE_LATENCY.store(v[5], Ordering::Relaxed);
        }
        log::info!(
            "[ble-exp] active {} us / latency {} / {} ms; idle after {} ms -> {} us / latency {}",
            INTERVAL_US.load(Ordering::Relaxed),
            LATENCY.load(Ordering::Relaxed),
            TIMEOUT_MS.load(Ordering::Relaxed),
            IDLE_AFTER_MS.load(Ordering::Relaxed),
            IDLE_INTERVAL_US.load(Ordering::Relaxed),
            IDLE_LATENCY.load(Ordering::Relaxed)
        );
    }

    pub fn active() -> (Duration, u16, Duration) {
        (
            Duration::from_micros(u64::from(INTERVAL_US.load(Ordering::Relaxed))),
            LATENCY.load(Ordering::Relaxed) as u16,
            Duration::from_millis(u64::from(TIMEOUT_MS.load(Ordering::Relaxed))),
        )
    }

    pub fn idle() -> (Duration, u16, Duration) {
        (
            Duration::from_micros(u64::from(IDLE_INTERVAL_US.load(Ordering::Relaxed))),
            IDLE_LATENCY.load(Ordering::Relaxed) as u16,
            Duration::from_millis(u64::from(TIMEOUT_MS.load(Ordering::Relaxed))),
        )
    }

    /// `None` when the active/idle switch is off.
    pub fn idle_after() -> Option<Duration> {
        match IDLE_AFTER_MS.load(Ordering::Relaxed) {
            0 => None,
            ms => Some(Duration::from_millis(u64::from(ms))),
        }
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
