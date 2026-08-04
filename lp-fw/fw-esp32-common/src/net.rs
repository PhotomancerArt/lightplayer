//! Chip-agnostic embassy-net glue: stack construction, the shared network
//! status, and the DHCP wait/status loop (networking M2-P2, first increment).
//!
//! ## The seam is `embassy_net::driver::Driver`, not a trait of ours
//!
//! Chip crates own the driver: EMAC vs wifi, RMII pins, clock topology and
//! MAC derivation are all chip/board facts, and this crate is forbidden from
//! holding chip facts (seam rules in `Cargo.toml`; ADR
//! `2026-07-29-per-chip-fw-toolchains`). embassy-net already abstracts the
//! driver behind `embassy_net::driver::Driver`, so inventing a LightPlayer
//! trait on top would be a second seam saying the same thing. What lives here
//! is only what is truly driver-agnostic: stack construction, the
//! [`NetStatus`] the rest of the firmware can read, and the DHCP wait loop.
//!
//! ## Why the embassy tasks are NOT here
//!
//! `#[embassy_executor::task]` functions cannot be generic, and
//! `embassy_net::Runner<'_, D>` carries the driver type. So the runner task
//! (and the thin status task that calls [`dhcp_status_loop`]) are declared in
//! the chip crate against a concrete driver type alias, and this module keeps
//! the generic/concrete split honest: [`init_stack`] is generic over the
//! driver, [`dhcp_status_loop`] takes `embassy_net::Stack<'static>`, which is
//! deliberately not generic in embassy-net 0.9.

use core::cell::Cell;

use embassy_net::{Config, Ipv4Cidr, Runner, Stack, StackResources, driver::Driver};
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};

/// Socket slots in the [`StackResources`] the host firmware allocates.
///
/// The DHCP client occupies one; the remaining three are headroom for the
/// later phases this feature exists for (HTTP control plane, discovery — the
/// `tcp`/`dns` features are already on for them). Control traffic is light,
/// so this is sized for "a couple of concurrent conversations", not
/// throughput. The count is a constant here so the chip crate's static and
/// [`init_stack`]'s signature cannot drift apart.
pub const SOCKET_COUNT: usize = 4;

/// What the rest of the firmware may know about the network, cheaply.
///
/// Written only by [`dhcp_status_loop`]; read via [`net_status`]. `up` means
/// "IPv4 config acquired", not "link pulse seen" — on a fixed-link board the
/// PHY is assumed up unconditionally, so a lease is the first *evidence* of
/// connectivity we actually have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetStatus {
    /// True once DHCP has produced an IPv4 config, false after it is lost.
    pub up: bool,
    /// The leased address + prefix, while `up`.
    pub ip: Option<Ipv4Cidr>,
}

/// The one shared status cell. A blocking critical-section mutex around a
/// `Cell`, not an async mutex: readers are things like heartbeat/log paths
/// that must not await, and the payload is a couple of words — the critical
/// section is a handful of cycles.
static NET_STATUS: Mutex<CriticalSectionRawMutex, Cell<NetStatus>> =
    Mutex::new(Cell::new(NetStatus { up: false, ip: None }));

/// Snapshot of the current network status.
pub fn net_status() -> NetStatus {
    NET_STATUS.lock(Cell::get)
}

fn publish(status: NetStatus) {
    NET_STATUS.lock(|cell| cell.set(status));
}

/// Builds the embassy-net stack over any driver, configured for DHCPv4.
///
/// Thin by design — it pins down the three decisions every LightPlayer image
/// should share (DHCP config, socket count, caller-supplied RNG seed) and
/// nothing else. The seed comes in as a plain `u64` because the RNG is a chip
/// peripheral this crate must not name.
pub fn init_stack<D: Driver>(
    driver: D,
    resources: &'static mut StackResources<SOCKET_COUNT>,
    seed: u64,
) -> (Stack<'static>, Runner<'static, D>) {
    embassy_net::new(driver, Config::dhcpv4(Default::default()), resources, seed)
}

/// DHCP wait + status-update loop: publishes [`NetStatus`] transitions and
/// logs them through the normal firmware logger (i.e. onto the serial wire).
///
/// Runs forever; the chip crate wraps it in an `#[embassy_executor::task]`.
/// Losing the config (lease expiry with no renewal, cable pulled mid-lease)
/// is handled by looping back to the wait rather than by any recovery logic:
/// embassy-net's DHCP socket keeps soliciting on its own, so the loop's only
/// job is to keep the status and the log truthful.
pub async fn dhcp_status_loop(stack: Stack<'static>) {
    loop {
        stack.wait_config_up().await;
        // `config_v4` is `Some` immediately after `wait_config_up` resolves;
        // the `if let` guards the (theoretical) race where the config is lost
        // between the wake and the read.
        if let Some(cfg) = stack.config_v4() {
            publish(NetStatus {
                up: true,
                ip: Some(cfg.address),
            });
            let ip = cfg.address;
            match cfg.gateway {
                Some(gw) => log::info!("net: ip={ip} gw={gw}"),
                None => log::info!("net: ip={ip} gw=none"),
            }
        }
        stack.wait_config_down().await;
        publish(NetStatus { up: false, ip: None });
        log::info!("net: IPv4 config lost, waiting for DHCP");
    }
}
