//! The station task: the join policy driven over the radio, on `lp-net`.
//!
//! [`StationPolicy`] (sans-IO, `fw-esp32-common`) decides; this task does
//! what it asks with [`EspStation`] and the IP stack, and feeds back what
//! happened. It takes the network file from the board
//! ([`super::station_probes::STATION_BOARD`]) and publishes the policy there
//! after every step, for the server's probes.
//!
//! - **The one waiter on the stack's config.** The services learn the
//!   address from [`super::net_address`], not from the stack (its single
//!   waker slot made several waiters spin).
//! - **DHCP starts on link-up** (plan MD9): the stack has no IPv4 config
//!   until the station associates, and loses it when the link goes. An
//!   embassy-net DHCP client started before link-up backs off, which is
//!   where the experiments' 10–12 s came from.
//! - **DHCP retries every second** ([`dhcp_config`]), not smoltcp's 10 s.
//!   The first DISCOVER goes out the moment the station associates, often
//!   before the access point has finished the WPA handshake, and is lost;
//!   smoltcp's default sent the next one 10 s later, which is exactly the
//!   policy's [`ADDRESS_TIMEOUT_MS`], so the first join after every boot
//!   failed and the board got its address on the rejoin ~22 s after boot
//!   (silicon, the G1 desk numbers: `connecting … frames in 267 out 1`).
//!
//! [`ADDRESS_TIMEOUT_MS`]: fw_esp32_common::net::station_policy::ADDRESS_TIMEOUT_MS
//! - **A client's scan** (the server's probe answered `scanning`) runs when
//!   no attempt is under way, and records what was heard for the next ask.
//!   It is the one scan a board with nothing saved ever makes, and only on
//!   request; afterwards the radio goes back to ESP-NOW's channel.
//! - Every radio call is bounded ([`EspStation`]'s limits), so an emulated
//!   board, whose radio never finishes a scan, still answers. A scan that
//!   got no answer is not recorded: the scan probe keeps saying `scanning`
//!   rather than claim the radio heard nothing.

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;

use embassy_futures::select::{Either4, select4};
use embassy_net::{ConfigV4, DhcpConfig, Stack};
use embassy_time::{Duration, Instant, Timer};
use fw_esp32_common::net::{
    ConnectOutcome, StationAction, StationControl, StationEvent, StationPolicy, StationSettings,
};
use lpc_access::NetworkFile;
use lpc_wire::{ConnectStep, StationState};

use super::esp_station::EspStation;
use super::station_probes::STATION_BOARD;

/// How often a joined station reads its signal.
const SIGNAL_EVERY: Duration = Duration::from_secs(10);

/// The task. `host` is the board's LAN name (`lp-xxxx.local`).
#[embassy_executor::task]
pub async fn station_task(mut control: EspStation, stack: Stack<'static>, host: String) {
    let mut policy = StationPolicy::new(host);
    let mut file = NetworkFile::none();
    let mut queue: VecDeque<StationAction> = VecDeque::new();
    loop {
        if let Some(new) = STATION_BOARD.take_settings() {
            let was_using = policy.uses_wifi();
            file = new;
            let settings = StationSettings::from_file(&file);
            queue.extend(policy.handle(now_ms(), StationEvent::SettingsChanged(settings)));
            if was_using && !policy.uses_wifi() {
                // Wi-Fi turned off, or the last network forgotten: once
                // left, the radio goes back to ESP-NOW's channel.
                queue.push_back(StationAction::Disconnect);
            }
        }
        while let Some(action) = queue.pop_front() {
            for event in run(&mut control, stack, &file, &policy, action).await {
                queue.extend(policy.handle(now_ms(), event));
            }
            STATION_BOARD.publish(&policy);
        }
        STATION_BOARD.publish(&policy);

        if STATION_BOARD.take_scan_wanted() && !attempting(policy.state()) {
            let heard = scan(&mut control).await;
            if !policy.uses_wifi() {
                control.restore_espnow_channel();
            }
            queue.extend(policy.handle(now_ms(), StationEvent::ScanDone(heard)));
            continue;
        }

        let connected = matches!(policy.state(), StationState::Connected { .. });
        let getting_address = matches!(
            policy.state(),
            StationState::Connecting {
                step: ConnectStep::GettingAddress,
                ..
            }
        );
        let mut wake = policy.next_wake().map(Instant::from_millis);
        if connected {
            let signal = Instant::now() + SIGNAL_EVERY;
            wake = Some(wake.map_or(signal, |at| at.min(signal)));
        }
        let event = select4(
            STATION_BOARD.wait(),
            // No deadline is no timer at all.
            sleep_until(wake),
            async {
                if connected {
                    control.wait_link_lost().await;
                } else {
                    core::future::pending::<()>().await;
                }
            },
            async {
                if getting_address {
                    stack.wait_config_up().await;
                    stack
                        .config_v4()
                        .map(|config| config.address.address().octets())
                } else {
                    core::future::pending().await
                }
            },
        )
        .await;
        match event {
            Either4::First(()) => {}
            Either4::Second(()) => {
                // Only a joined station has a signal to read: asking while
                // searching makes esp-radio log an error every tick (silicon,
                // 2026-10-06).
                if connected && let Some(rssi) = control.rssi() {
                    policy.handle(now_ms(), StationEvent::Signal(rssi));
                }
                queue.extend(policy.handle(now_ms(), StationEvent::Tick));
            }
            Either4::Third(()) => {
                stack.set_config_v4(ConfigV4::None);
                super::net_address::publish(None);
                queue.extend(policy.handle(now_ms(), StationEvent::LinkLost));
            }
            Either4::Fourth(Some(ip)) => {
                log::info!("[wifi] address {}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
                super::net_address::publish(Some(ip));
                queue.extend(policy.handle(now_ms(), StationEvent::AddressAcquired(ip)));
            }
            Either4::Fourth(None) => {}
        }
    }
}

/// Run one action; what happened, for the policy.
async fn run(
    control: &mut EspStation,
    stack: Stack<'static>,
    file: &NetworkFile,
    policy: &StationPolicy,
    action: StationAction,
) -> Vec<StationEvent> {
    match action {
        StationAction::Scan => alloc::vec![StationEvent::ScanDone(scan(control).await)],
        StationAction::Connect { ssid } => {
            let Some(network) = file.network(&ssid) else {
                return alloc::vec![StationEvent::LinkLost];
            };
            stack.set_config_v4(ConfigV4::None);
            super::net_address::publish(None);
            log::info!("[wifi] trying {ssid}");
            let started = Instant::now();
            match control.connect(&ssid, &network.password).await {
                ConnectOutcome::Associated => {
                    log::info!(
                        "[wifi] associated with {ssid} in {} ms",
                        started.elapsed().as_millis()
                    );
                    stack.set_config_v4(ConfigV4::Dhcp(dhcp_config()));
                    alloc::vec![StationEvent::Associated]
                }
                ConnectOutcome::AuthFailed => alloc::vec![StationEvent::AuthFailed],
                ConnectOutcome::NotHeard => alloc::vec![StationEvent::NotHeard],
                ConnectOutcome::Ended => alloc::vec![StationEvent::LinkLost],
            }
        }
        StationAction::Disconnect => {
            control.disconnect().await;
            stack.set_config_v4(ConfigV4::None);
            super::net_address::publish(None);
            if !policy.uses_wifi() {
                control.restore_espnow_channel();
            }
            Vec::new()
        }
    }
}

/// Scan, and record what was heard for the scan probe. No answer is
/// recorded as nothing (the probe keeps saying `scanning`); the policy hears
/// an empty list and carries on.
async fn scan(control: &mut EspStation) -> Vec<lpc_wire::HeardNetwork> {
    match control.scan().await {
        Some(heard) => {
            STATION_BOARD.record_scan(now_ms(), heard.clone());
            heard
        }
        None => Vec::new(),
    }
}

/// The DHCP client's timing: DISCOVER (and the first REQUEST) retried every
/// second, so a first DISCOVER lost to the access point's WPA handshake costs
/// a second, not the address timeout (module doc).
fn dhcp_config() -> DhcpConfig {
    let mut config = DhcpConfig::default();
    config.retry_config.discover_timeout = smoltcp::time::Duration::from_secs(1);
    config.retry_config.initial_request_timeout = smoltcp::time::Duration::from_secs(1);
    config
}

/// Sleep until `at`, or forever with no deadline.
pub async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => Timer::at(at).await,
        None => core::future::pending().await,
    }
}

/// An attempt is under way: a client's scan waits for it.
fn attempting(state: &StationState) -> bool {
    matches!(state, StationState::Connecting { .. })
}

fn now_ms() -> u64 {
    Instant::now().as_millis()
}
