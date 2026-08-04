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
use core::fmt::Write as _;

use embassy_net::driver::{Driver, HardwareAddress};
use embassy_net::{Config, DhcpConfig, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
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
    Mutex::new(Cell::new(NetStatus {
        up: false,
        ip: None,
    }));

/// Snapshot of the current network status.
pub fn net_status() -> NetStatus {
    NET_STATUS.lock(Cell::get)
}

fn publish(status: NetStatus) {
    NET_STATUS.lock(|cell| cell.set(status));
}

/// The longest hostname the stack announces — embassy-net's DHCP option 12
/// cap (its `MAX_HOSTNAME_LEN` is private, so this mirrors it; the
/// `hostname` field's concrete type keeps the two honest at compile time).
pub const MAX_HOSTNAME_LEN: usize = 32;

/// Caller choices for [`init_stack`], resolved from `/.lp/net.json` by the
/// chip crate (which owns the filesystem and the medium decision). Both
/// `None`s mean the stock experience: DHCP, hostname derived from the
/// interface MAC.
#[derive(Default)]
pub struct NetOptions<'a> {
    /// Hostname to announce (DHCP option 12 today, mDNS later).
    /// `None` → `lp-<mac6>` derived from the driver's hardware address.
    pub hostname: Option<&'a str>,
    /// Static IPv4 configuration. `None` → DHCPv4.
    pub static_v4: Option<StaticConfigV4>,
}

#[cfg(feature = "server")]
impl<'a> NetOptions<'a> {
    /// Resolve `/.lp/net.json` settings into stack options: validated
    /// hostname override, static-IPv4 block if one parsed. The MEDIUM
    /// decision (`mode`) is deliberately not consumed here — which media an
    /// image has is a chip/board fact, so the chip crate gates on `mode`
    /// before it ever builds a driver.
    pub fn from_settings(settings: Option<&'a lpa_server::net_settings::NetSettingsFile>) -> Self {
        let Some(settings) = settings else {
            return Self::default();
        };
        NetOptions {
            hostname: settings.valid_hostname(),
            static_v4: settings.resolved_static_ipv4().map(|static_v4| {
                StaticConfigV4 {
                    address: Ipv4Cidr::new(static_v4.address, static_v4.prefix),
                    gateway: static_v4.gateway,
                    // `take(3)`: heapless `FromIterator` panics past capacity,
                    // and the settings reader does not cap the list.
                    dns_servers: static_v4.dns.iter().copied().take(3).collect(),
                }
            }),
        }
    }
}

/// The default hostname: `lp-` + the low three MAC bytes in hex — stable per
/// device, short enough for any label rule, and matching the discovery
/// milestone's `lp-<mac6>.local` convention.
pub fn default_hostname(mac: &[u8; 6]) -> heapless::String<MAX_HOSTNAME_LEN> {
    let mut name = heapless::String::new();
    // Infallible: "lp-" + 6 hex digits is 9 bytes into a 32-byte string.
    let _ = write!(name, "lp-{:02x}{:02x}{:02x}", mac[3], mac[4], mac[5]);
    name
}

/// Builds the embassy-net stack over any driver.
///
/// Thin by design — it pins down the decisions every LightPlayer image
/// should share (IPv4 config shape, announced hostname, socket count,
/// caller-supplied RNG seed) and nothing else. The seed comes in as a plain
/// `u64` because the RNG is a chip peripheral this crate must not name.
pub fn init_stack<D: Driver>(
    driver: D,
    resources: &'static mut StackResources<SOCKET_COUNT>,
    seed: u64,
    options: NetOptions<'_>,
) -> (Stack<'static>, Runner<'static, D>) {
    let hostname: heapless::String<MAX_HOSTNAME_LEN> = match options.hostname {
        // Length is pre-validated by the settings reader; a caller handing us
        // an oversized name anyway gets the derived default, not a panic.
        Some(name) => {
            heapless::String::try_from(name).unwrap_or_else(|_| derived_hostname(&driver))
        }
        None => derived_hostname(&driver),
    };
    let config = match options.static_v4 {
        Some(static_v4) => {
            log::info!(
                "net: hostname={hostname} ipv4=static {} gw={:?}",
                static_v4.address,
                static_v4.gateway
            );
            Config::ipv4_static(static_v4)
        }
        None => {
            log::info!("net: hostname={hostname} ipv4=dhcp");
            let mut dhcp = DhcpConfig::default();
            dhcp.hostname = Some(hostname);
            Config::dhcpv4(dhcp)
        }
    };
    embassy_net::new(driver, config, resources, seed)
}

fn derived_hostname<D: Driver>(driver: &D) -> heapless::String<MAX_HOSTNAME_LEN> {
    match driver.hardware_address() {
        HardwareAddress::Ethernet(mac) => default_hostname(&mac),
        // No MAC to derive from (unreachable for the media we ship today);
        // a fixed name beats none.
        _ => {
            let mut name = heapless::String::new();
            let _ = write!(name, "lp-device");
            name
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hostname_is_lp_plus_low_mac_bytes() {
        assert_eq!(
            default_hostname(&[0x32, 0x76, 0xf5, 0xec, 0xf6, 0x34]).as_str(),
            "lp-ecf634"
        );
    }
}

/// IPv4-config wait + status-update loop: publishes [`NetStatus`] transitions
/// and logs them through the normal firmware logger (i.e. onto the serial
/// wire). Works for DHCP and static configs alike — under a static config
/// `wait_config_up` resolves immediately and the loop simply reports it.
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
        publish(NetStatus {
            up: false,
            ip: None,
        });
        log::info!("net: IPv4 config lost, waiting for a new one");
    }
}
